use std::{num::ParseIntError, path::Path, time::Duration};

use anyhow::{anyhow, Ok};
use log;
use reqwest::{header::ACCEPT, Client, ClientBuilder};
use serde::{Deserialize, Serialize};

use crate::eth_consensus_layer::{BeaconBlockHeader, BeaconState, Root};

use super::{
    file::{FileBasedBeaconStateReader, FileBeaconStateWriter},
    BeaconStateReader,
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

fn _strip_0x_prefix<'a>(value: &'a str) -> &'a str {
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
            slot: slot,
            proposer_index: proposer_index,
            parent_root: parent_root.into(),
            state_root: state_root.into(),
            body_root: body_root.into(),
        };
        Result::Ok(result)
    }
}

pub trait BeaconChainRPC {
    #[allow(async_fn_in_trait)]
    async fn get_finalized_slot(&self) -> anyhow::Result<u64>;
}

pub struct ReqwestBeaconStateReader {
    base_url: String,
    client: Client,
}

impl ReqwestBeaconStateReader {
    pub fn new(base_url: &str) -> Self {
        let client = ClientBuilder::new()
            .timeout(Duration::new(300, 0))
            .build()
            .expect("Failed to create http client");

        let normalized_url = base_url.strip_suffix("/").unwrap_or(&base_url);

        Self {
            base_url: normalized_url.to_owned(),
            client: client,
        }
    }

    fn map_err(label: &str, e: reqwest::Error) -> anyhow::Error {
        anyhow!("{}: {:#?}", label, e)
    }

    async fn read_bs(&self, block_id: &str) -> anyhow::Result<BeaconState> {
        log::info!("Loading beacon state for {block_id}");
        let url = format!("{}/eth/v2/debug/beacon/states/{}", self.base_url, block_id);
        log::debug!("Url: {url}");
        let response = self
            .client
            .get(url.clone())
            .header(ACCEPT, "application/octet-stream")
            .send()
            .await
            .map_err(|e| Self::map_err(&format!("Failed to make request {url}"), e))?;

        log::debug!(
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

        log::info!("Received response for {block_id} - {} bytes", bytes.len());
        BeaconState::from_ssz_bytes(&bytes)
            .map_err(|decode_err| anyhow::anyhow!("Couldn't decode ssz {:#?}", decode_err))
    }

    async fn read_beacon_header(&self, block_id: &str) -> anyhow::Result<BeaconBlockHeader> {
        let url = format!("{}/eth/v1/beacon/headers/{}", self.base_url, block_id);
        log::info!("Loading beacon header for {block_id}");

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

        log::debug!("Read BeaconBlockHeader {:?}", res.data.header.message);

        res.data.header.message.try_into().map_err(|e: ConvertionError| {
            anyhow::anyhow!(
                "Failed to convert Beacon API response DTO to BeaconBlockHeader {:#?}",
                e
            )
        })
    }
}

impl BeaconStateReader for ReqwestBeaconStateReader {
    async fn read_beacon_state(&self, slot: u64) -> anyhow::Result<BeaconState> {
        return self.read_bs(&slot.to_string()).await;
    }

    async fn read_beacon_block_header(&self, slot: u64) -> anyhow::Result<BeaconBlockHeader> {
        return self.read_beacon_header(&slot.to_string()).await;
    }
}

impl BeaconChainRPC for ReqwestBeaconStateReader {
    async fn get_finalized_slot(&self) -> anyhow::Result<u64> {
        self.read_beacon_header("finalized").await.map(|header| header.slot)
    }
}

pub struct CachedReqwestBeaconStateReader {
    rpc_reader: ReqwestBeaconStateReader,
    file_reader: FileBasedBeaconStateReader,
    file_writer: FileBeaconStateWriter,
}

impl CachedReqwestBeaconStateReader {
    pub fn new(base_url: &str, file_store: &Path) -> Self {
        Self {
            rpc_reader: ReqwestBeaconStateReader::new(base_url),
            file_reader: FileBasedBeaconStateReader::new(file_store),
            file_writer: FileBeaconStateWriter::new(file_store),
        }
    }
}

impl BeaconStateReader for CachedReqwestBeaconStateReader {
    async fn read_beacon_state(&self, slot: u64) -> anyhow::Result<BeaconState> {
        let try_from_file = self.file_reader.read_beacon_state(slot).await;
        if let core::result::Result::Ok(beacon_state) = try_from_file {
            return Ok(beacon_state);
        }
        let try_from_rpc = self.rpc_reader.read_beacon_state(slot).await;
        if let core::result::Result::Ok(beacon_state) = try_from_rpc {
            self.file_writer.write_beacon_state(&beacon_state)?;
            return Ok(beacon_state);
        } else {
            return try_from_rpc;
        }
    }

    async fn read_beacon_block_header(&self, slot: u64) -> anyhow::Result<BeaconBlockHeader> {
        let try_from_file = self.file_reader.read_beacon_block_header(slot).await;
        if let core::result::Result::Ok(block_header) = try_from_file {
            return Ok(block_header);
        }
        let try_from_rpc = self.rpc_reader.read_beacon_block_header(slot).await;
        if let core::result::Result::Ok(block_header) = try_from_rpc {
            self.file_writer.write_beacon_block_header(&block_header)?;
            return Ok(block_header);
        } else {
            return try_from_rpc;
        }
    }
}

impl BeaconChainRPC for CachedReqwestBeaconStateReader {
    async fn get_finalized_slot(&self) -> anyhow::Result<u64> {
        self.rpc_reader.get_finalized_slot().await
    }
}
