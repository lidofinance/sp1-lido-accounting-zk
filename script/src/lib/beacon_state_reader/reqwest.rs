use std::{num::ParseIntError, path::Path, time::Duration};

use anyhow::anyhow;
use reqwest::{header::ACCEPT, Client, ClientBuilder};
use serde::{Deserialize, Serialize};

use sp1_lido_accounting_zk_shared::{
    eth_consensus_layer::{BeaconBlockHeader, BeaconState, Root},
    io::eth_io::{BeaconChainSlot, HaveSlotWithBlock},
};

use super::{
    file::{FileBasedBeaconStateReader, FileBeaconStateWriter},
    BeaconStateReader, StateId,
};
use ssz::Decode;

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
}

impl ReqwestBeaconStateReader {
    fn normalize_url(base_url: &str) -> String {
        base_url.strip_suffix('/').unwrap_or(base_url).to_owned()
    }

    pub fn new(consensus_layer_base_uri: &str, beacon_state_base_uri: &str) -> Self {
        let client = ClientBuilder::new()
            .timeout(Duration::new(300, 0))
            .build()
            .expect("Failed to create http client");

        Self {
            consensus_layer_base_uri: Self::normalize_url(consensus_layer_base_uri),
            beacon_state_base_uri: Self::normalize_url(beacon_state_base_uri),
            client,
        }
    }

    fn map_err(label: &str, e: reqwest::Error) -> anyhow::Error {
        anyhow!("{}: {:#?}", label, e)
    }

    async fn read_bs(&self, state_id: &StateId) -> anyhow::Result<BeaconState> {
        tracing::info!("Loading beacon state for {}", state_id.as_str());
        let url = format!(
            "{}/eth/v2/debug/beacon/states/{}",
            self.beacon_state_base_uri,
            state_id.as_str()
        );
        tracing::debug!("Url: {url}");
        let response = self
            .client
            .get(url.clone())
            .header(ACCEPT, "application/octet-stream")
            .send()
            .await
            .map_err(|e| Self::map_err(&format!("Failed to make request {url}"), e))?;

        tracing::debug!(
            "Received response with status {} and content length {}",
            response.status(),
            response
                .content_length()
                .map(|v| v.to_string())
                .unwrap_or("unknown".to_string())
        );

        let bytes = response
            .error_for_status()
            .map_err(|e| Self::map_err("Unsuccessful status code", e))?
            .bytes()
            .await
            .map_err(|e| Self::map_err("Failed to get response body", e))?;

        tracing::info!("Received response for {} - {} bytes", state_id.as_str(), bytes.len());
        BeaconState::from_ssz_bytes(&bytes)
            .map_err(|decode_err| anyhow::anyhow!("Couldn't decode ssz {:#?}", decode_err))
    }

    async fn read_beacon_header(&self, state_id: &StateId) -> anyhow::Result<BeaconBlockHeader> {
        let url = format!(
            "{}/eth/v1/beacon/headers/{}",
            self.consensus_layer_base_uri,
            state_id.as_str()
        );
        tracing::info!("Loading beacon header for {}", state_id.as_str());

        let response = self
            .client
            .get(url.clone())
            .header(ACCEPT, "application/json")
            .send()
            .await
            .map_err(|e| Self::map_err(&format!("Failed to make request {url}"), e))?;

        let res = response
            .error_for_status()
            .map_err(|e| Self::map_err("Unsuccessful status code", e))?
            .json::<BeaconHeaderResponse>()
            .await
            .map_err(|e| anyhow::anyhow!("Couldn't parse json {:#?}", e))?;

        tracing::debug!("Read BeaconBlockHeader {:?}", res.data.header.message);

        res.data.header.message.try_into().map_err(|e: ConvertionError| {
            anyhow::anyhow!(
                "Failed to convert Beacon API response DTO to BeaconBlockHeader {:#?}",
                e
            )
        })
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
    ) -> Result<Self, super::Error> {
        let result = Self {
            rpc_reader: ReqwestBeaconStateReader::new(consensus_layer_base_uri, beacon_state_base_uri),
            file_reader: FileBasedBeaconStateReader::new(file_store)?,
            file_writer: FileBeaconStateWriter::new(file_store)?,
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

impl BeaconChainRPC for CachedReqwestBeaconStateReader {
    async fn get_finalized_slot(&self) -> anyhow::Result<BeaconChainSlot> {
        self.rpc_reader.get_finalized_slot().await
    }
}
