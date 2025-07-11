use std::{num::ParseIntError, path::Path, sync::Arc, time::Duration};

use anyhow::anyhow;
use reqwest::{header::ACCEPT, Client, ClientBuilder, Response};
use serde::{Deserialize, Serialize};

use sp1_lido_accounting_zk_shared::{
    eth_consensus_layer::{BeaconBlockHeader, BeaconState, Root},
    io::eth_io::{BeaconChainSlot, HaveSlotWithBlock, ReferenceSlot},
};
use tree_hash::TreeHash;

use crate::prometheus_metrics;

use super::{
    file::{FileBasedBeaconStateReader, FileBeaconStateWriter},
    BeaconStateReader, RefSlotResolver, StateId,
};
use ssz::Decode;

pub use reqwest::Error as ReqwestError; // reexporting

#[derive(Debug)]
pub enum ConvertionError {
    FailedToParseIntField(ParseIntError),
    FailedToParseHashField(hex::FromHexError),
}

#[derive(Serialize, Deserialize)]
struct BeaconHeaderResponse {
    pub execution_optimistic: bool,
    pub finalized: bool,
    pub data: BeaconHeaderResponseData,
}

#[derive(Serialize, Deserialize)]
struct BeaconHeaderResponseData {
    pub root: Root,
    pub canonical: bool,
    pub header: BeaconHeaderResponseDataHeader,
}

#[derive(Serialize, Deserialize)]
struct BeaconHeaderResponseDataHeader {
    pub message: BeaconHeaderResponseDataHeaderMessage,
    pub signature: String,
}

#[derive(Serialize, Deserialize, Debug)]
struct BeaconHeaderResponseDataHeaderMessage {
    pub slot: String,
    pub proposer_index: String,
    pub parent_root: String,
    pub state_root: String,
    pub body_root: String,
}

const MAX_REFERENCE_LOOKBACK_ATTEMPTS: u32 = 60 /*m*/ * 60 /*s*/ / 12 /*sec _per_slot*/;
const LOG_LOOKBACK_ATTEMPT_DELAY: u32 = 20;
const LOG_LOOKBACK_ATTEMPT_INTERVAL: u32 = 10;

fn _strip_0x_prefix(value: &str) -> &str {
    value.strip_prefix("0x").unwrap_or(value)
}

impl TryFrom<BeaconHeaderResponseDataHeaderMessage> for BeaconBlockHeader {
    type Error = ConvertionError;

    fn try_from(value: BeaconHeaderResponseDataHeaderMessage) -> Result<Self, Self::Error> {
        let slot: u64 = value.slot.parse().map_err(ConvertionError::FailedToParseIntField)?;
        let proposer_index: u64 = value
            .proposer_index
            .parse()
            .map_err(ConvertionError::FailedToParseIntField)?;

        let mut parent_root: [u8; 32] = [0; 32];
        hex::decode_to_slice(_strip_0x_prefix(&value.parent_root), &mut parent_root)
            .map_err(ConvertionError::FailedToParseHashField)?;
        let mut state_root: [u8; 32] = [0; 32];
        hex::decode_to_slice(_strip_0x_prefix(&value.state_root), &mut state_root)
            .map_err(ConvertionError::FailedToParseHashField)?;
        let mut body_root: [u8; 32] = [0; 32];
        hex::decode_to_slice(_strip_0x_prefix(&value.body_root), &mut body_root)
            .map_err(ConvertionError::FailedToParseHashField)?;

        let result = BeaconBlockHeader {
            slot,
            proposer_index,
            parent_root: parent_root.into(),
            state_root: state_root.into(),
            body_root: body_root.into(),
        };
        Result::Ok(result)
    }
}

pub trait BeaconChainRPC {
    #[allow(async_fn_in_trait)]
    async fn get_finalized_slot(&self) -> anyhow::Result<BeaconChainSlot>;
}

pub struct ReqwestBeaconStateReader {
    consensus_layer_base_uri: String,
    beacon_state_base_uri: String,
    client: Client,
    metrics_reporter: Arc<prometheus_metrics::Service>,
}

impl ReqwestBeaconStateReader {
    fn normalize_url(base_url: &str) -> String {
        base_url.strip_suffix('/').unwrap_or(base_url).to_owned()
    }

    pub fn new(
        consensus_layer_base_uri: &str,
        beacon_state_base_uri: &str,
        metrics_reporter: Arc<prometheus_metrics::Service>,
    ) -> Result<Self, super::InitializationError> {
        let client = ClientBuilder::new().timeout(Duration::new(300, 0)).build()?;

        let res = Self {
            consensus_layer_base_uri: Self::normalize_url(consensus_layer_base_uri),
            beacon_state_base_uri: Self::normalize_url(beacon_state_base_uri),
            client,
            metrics_reporter,
        };
        Ok(res)
    }

    fn map_err<E: std::fmt::Debug>(label: &str, state_id: &StateId, err: E) -> anyhow::Error {
        let msg = format!("{}: {:#?}", label, err);
        tracing::debug!(state_id=?state_id, msg);
        anyhow!(msg)
    }

    async fn send_request(&self, url: String, accept_header: &str, state_id: &StateId) -> anyhow::Result<Response> {
        tracing::debug!(state_id=?state_id, "Sending request to: {url}");
        let err_msg = format!("Failed to make request {url}");
        self.client
            .get(url)
            .header(ACCEPT, accept_header)
            .send()
            .await
            .inspect(|resp| {
                tracing::debug!(
                    state_id=?state_id,
                    "Received response with status {} and content length {}",
                    resp.status(),
                    resp.content_length().map(|v| v.to_string()).unwrap_or("[unknown]".to_string())
                )
            })
            .map_err(|e| Self::map_err(&err_msg, state_id, e))
    }

    async fn get_beacon_header_response_impl(&self, state_id: &StateId) -> anyhow::Result<BeaconHeaderResponse> {
        let url = format!(
            "{}/eth/v1/beacon/headers/{}",
            self.consensus_layer_base_uri,
            state_id.as_str()
        );
        let response = self.send_request(url, "application/json", state_id).await?;

        let res = response
            .error_for_status()
            .map_err(|err| Self::map_err("Unsuccessful status code", state_id, err))?
            .json::<BeaconHeaderResponse>()
            .await
            .map_err(|err| Self::map_err("Couldn't parse json", state_id, err))?;

        tracing::debug!("Read BeaconBlockHeader {:?}", res.data.header.message);

        Ok(res)
    }

    async fn get_beacon_header_response(&self, state_id: &StateId) -> anyhow::Result<BeaconHeaderResponse> {
        self.metrics_reporter
            .run_with_metrics_and_logs_async(
                prometheus_metrics::services::beacon_state_reader::READ_BEACON_BLOCK_HEADER,
                || self.get_beacon_header_response_impl(state_id),
            )
            .await
    }

    async fn read_bs_impl(&self, state_id: &StateId) -> anyhow::Result<BeaconState> {
        tracing::info!("Loading BeaconState for {}", state_id.as_str());
        let url = format!(
            "{}/eth/v2/debug/beacon/states/{}",
            self.beacon_state_base_uri,
            state_id.as_str()
        );
        let response = self.send_request(url, "application/octet-stream", state_id).await?;

        let bytes = response
            .error_for_status()
            .map_err(|err| Self::map_err("Unsuccessful status code", state_id, err))?
            .bytes()
            .await
            .map_err(|err| Self::map_err("Failed to get response body", state_id, err))?;

        BeaconState::from_ssz_bytes(&bytes)
            .inspect(
                |bs| tracing::info!(state_id=?state_id, slot=bs.slot, "Read BeaconState {} for {state_id:?}", bs.slot),
            )
            .map_err(|decode_err| Self::map_err("Couldn't decode BeaconState ssz", state_id, decode_err))
    }

    async fn read_bs(&self, state_id: &StateId) -> anyhow::Result<BeaconState> {
        self.metrics_reporter
            .run_with_metrics_and_logs_async(
                prometheus_metrics::services::beacon_state_reader::READ_BEACON_STATE,
                || self.read_bs_impl(state_id),
            )
            .await
    }

    async fn read_beacon_header(&self, state_id: &StateId) -> anyhow::Result<BeaconBlockHeader> {
        tracing::info!("Loading beacon header for {state_id:?}");
        let res = self.get_beacon_header_response(state_id).await?;

        res.data.header.message.try_into()
            .inspect(|bh: &BeaconBlockHeader| tracing::debug!(state_id=?state_id, slot=bh.slot, "Loaded BeaconBlockHeader {} for state {state_id:?}", bh.slot))
            .map_err(|err: ConvertionError| {
                Self::map_err(
                    "Failed to convert Beacon API response DTO to BeaconBlockHeader",
                    state_id,
                    err,
                )
            })
    }

    async fn find_bc_slot_for_refslot(&self, target_slot: ReferenceSlot) -> anyhow::Result<BeaconChainSlot> {
        let mut attempt_slot: u64 = target_slot.0;
        let mut attempt_count: u32 = 0;
        let max_lookback_slot = target_slot.0 - u64::from(MAX_REFERENCE_LOOKBACK_ATTEMPTS);
        tracing::info!(
            "Finding non-empty slot for reference slot {target_slot} searching from {target_slot} to {max_lookback_slot}"
        );
        loop {
            tracing::debug!("Checking slot {attempt_slot}");
            let maybe_header = self
                .read_beacon_block_header(&StateId::Slot(BeaconChainSlot(attempt_slot)))
                .await;
            match maybe_header {
                Ok(bh) => {
                    let result = bh.bc_slot();
                    let hash = bh.tree_hash_root();
                    tracing::info!("Found non-empty slot {result} with hash {hash:#x}");
                    return Ok(result);
                }
                Err(error) => {
                    if attempt_count == MAX_REFERENCE_LOOKBACK_ATTEMPTS {
                        tracing::error!("Couldn't find non-empty slot for reference slot {target_slot} between {target_slot} and {max_lookback_slot}");
                        return Err(error);
                    }
                    if attempt_count >= LOG_LOOKBACK_ATTEMPT_DELAY && attempt_count % LOG_LOOKBACK_ATTEMPT_INTERVAL == 0
                    {
                        tracing::warn!("Cannot find non-empty slot for reference slot {target_slot} for {attempt_count} attempts; last checked slot {attempt_slot}")
                    }
                    attempt_count += 1;
                    attempt_slot -= 1;
                }
            }
        }
    }
}

impl BeaconStateReader for ReqwestBeaconStateReader {
    async fn read_beacon_state(&self, state_id: &StateId) -> anyhow::Result<BeaconState> {
        self.read_bs(state_id).await
    }

    async fn read_beacon_block_header(&self, state_id: &StateId) -> anyhow::Result<BeaconBlockHeader> {
        self.read_beacon_header(state_id).await
    }
}

impl RefSlotResolver for ReqwestBeaconStateReader {
    async fn find_bc_slot_for_refslot(&self, target_slot: ReferenceSlot) -> anyhow::Result<BeaconChainSlot> {
        self.find_bc_slot_for_refslot(target_slot).await
    }

    async fn is_finalized_slot(&self, target_slot: BeaconChainSlot) -> anyhow::Result<bool> {
        let header = self.get_beacon_header_response(&StateId::Slot(target_slot)).await?;
        Ok(header.finalized)
    }
}

impl BeaconChainRPC for ReqwestBeaconStateReader {
    async fn get_finalized_slot(&self) -> anyhow::Result<BeaconChainSlot> {
        self.read_beacon_header(&StateId::Finalized)
            .await
            .map(|header| header.bc_slot())
    }
}

pub struct CachedReqwestBeaconStateReader {
    rpc_reader: ReqwestBeaconStateReader,
    file_reader: FileBasedBeaconStateReader,
    file_writer: FileBeaconStateWriter,
}

impl CachedReqwestBeaconStateReader {
    pub fn new(
        consensus_layer_base_uri: &str,
        beacon_state_base_uri: &str,
        file_store: &Path,
        fallthrough_file_stores: &[&Path],
        metric_reporter: Arc<prometheus_metrics::Service>,
    ) -> Result<Self, super::InitializationError> {
        let result = Self {
            rpc_reader: ReqwestBeaconStateReader::new(
                consensus_layer_base_uri,
                beacon_state_base_uri,
                Arc::clone(&metric_reporter),
            )?,
            file_reader: FileBasedBeaconStateReader::new_with_fallthrough(
                file_store,
                fallthrough_file_stores,
                Arc::clone(&metric_reporter),
            )?,
            file_writer: FileBeaconStateWriter::new(file_store, Arc::clone(&metric_reporter))?,
        };
        Ok(result)
    }
}

impl BeaconStateReader for CachedReqwestBeaconStateReader {
    async fn read_beacon_state(&self, state_id: &StateId) -> anyhow::Result<BeaconState> {
        let try_from_file = self.file_reader.read_beacon_state(state_id).await;
        if let core::result::Result::Ok(beacon_state) = try_from_file {
            return Ok(beacon_state);
        }
        let try_from_rpc = self.rpc_reader.read_beacon_state(state_id).await;
        if let core::result::Result::Ok(beacon_state) = try_from_rpc {
            self.file_writer.write_beacon_state(&beacon_state)?;
            Ok(beacon_state)
        } else {
            try_from_rpc
        }
    }

    async fn read_beacon_block_header(&self, state_id: &StateId) -> anyhow::Result<BeaconBlockHeader> {
        let try_from_file = self.file_reader.read_beacon_block_header(state_id).await;
        if let core::result::Result::Ok(block_header) = try_from_file {
            return Ok(block_header);
        }
        let try_from_rpc = self.rpc_reader.read_beacon_block_header(state_id).await;
        if let core::result::Result::Ok(block_header) = try_from_rpc {
            self.file_writer.write_beacon_block_header(&block_header)?;
            Ok(block_header)
        } else {
            try_from_rpc
        }
    }
}

impl RefSlotResolver for CachedReqwestBeaconStateReader {
    async fn find_bc_slot_for_refslot(&self, target_slot: ReferenceSlot) -> anyhow::Result<BeaconChainSlot> {
        self.rpc_reader.find_bc_slot_for_refslot(target_slot).await
    }

    async fn is_finalized_slot(&self, target_slot: BeaconChainSlot) -> anyhow::Result<bool> {
        self.rpc_reader.is_finalized_slot(target_slot).await
    }
}

impl BeaconChainRPC for CachedReqwestBeaconStateReader {
    async fn get_finalized_slot(&self) -> anyhow::Result<BeaconChainSlot> {
        self.rpc_reader.get_finalized_slot().await
    }
}
