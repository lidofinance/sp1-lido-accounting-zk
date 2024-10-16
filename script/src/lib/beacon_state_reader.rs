use anyhow;

use crate::beacon_state_reader::file::FileBasedBeaconStateReader;
use crate::beacon_state_reader::reqwest::{CachedReqwestBeaconStateReader, ReqwestBeaconStateReader};
use crate::consts::NetworkInfo;
use sp1_lido_accounting_zk_shared::eth_consensus_layer::{BeaconBlockHeader, BeaconState, Hash256};
use std::env;
use std::path::PathBuf;

pub mod file;
pub mod reqwest;
#[cfg(feature = "synthetic")]
pub mod synthetic;

pub enum StateId {
    Head,
    Genesis,
    Finalized,
    Justified,
    Slot(u64),
    Hash(Hash256),
}

impl StateId {
    fn as_str(&self) -> String {
        match self {
            Self::Head => "head".to_owned(),
            Self::Genesis => "genesis".to_owned(),
            Self::Finalized => "finalized".to_owned(),
            Self::Justified => "justified".to_owned(),
            Self::Slot(slot) => slot.to_string(),
            Self::Hash(hash) => hex::encode(hash),
        }
    }
}

pub trait BeaconStateReader {
    #[allow(async_fn_in_trait)]
    async fn read_beacon_state(&self, state_id: &StateId) -> anyhow::Result<BeaconState>;
    #[allow(async_fn_in_trait)]
    async fn read_beacon_block_header(&self, state_id: &StateId) -> anyhow::Result<BeaconBlockHeader>;
    #[allow(async_fn_in_trait)]
    async fn read_beacon_state_and_header(
        &self,
        state_id: &StateId,
    ) -> anyhow::Result<(BeaconBlockHeader, BeaconState)> {
        let bs = self.read_beacon_state(state_id).await?;
        let bh = self.read_beacon_block_header(state_id).await?;
        Ok((bh, bs))
    }
}

pub enum BeaconStateReaderEnum {
    File(FileBasedBeaconStateReader),
    RPC(ReqwestBeaconStateReader),
    RPCCached(CachedReqwestBeaconStateReader),
}

impl BeaconStateReaderEnum {
    fn read_env_var(env_var: &str, mode: &str) -> String {
        env::var(env_var).unwrap_or_else(|_| panic!("{env_var} must be specified for mode {mode}"))
    }

    pub fn new_from_env(network: &impl NetworkInfo) -> BeaconStateReaderEnum {
        let bs_reader_mode_var = env::var("BS_READER_MODE").expect("Failed to read BS_READER_MODE from env");

        match bs_reader_mode_var.to_lowercase().as_str() {
            "file" => {
                let file_store =
                    PathBuf::from(Self::read_env_var("BS_FILE_STORE", &bs_reader_mode_var)).join(network.as_str());
                BeaconStateReaderEnum::File(FileBasedBeaconStateReader::new(&file_store))
            }
            "rpc" => {
                let rpc_endpoint = Self::read_env_var("CONSENSUS_LAYER_RPC", &bs_reader_mode_var);
                let bs_endpoint = Self::read_env_var("BEACON_STATE_RPC", &bs_reader_mode_var);
                BeaconStateReaderEnum::RPC(ReqwestBeaconStateReader::new(&rpc_endpoint, &bs_endpoint))
            }
            "rpc_cached" => {
                let file_store =
                    PathBuf::from(Self::read_env_var("BS_FILE_STORE", &bs_reader_mode_var)).join(network.as_str());
                let rpc_endpoint = Self::read_env_var("CONSENSUS_LAYER_RPC", &bs_reader_mode_var);
                let bs_endpoint = Self::read_env_var("BEACON_STATE_RPC", &bs_reader_mode_var);
                BeaconStateReaderEnum::RPCCached(CachedReqwestBeaconStateReader::new(
                    &rpc_endpoint,
                    &bs_endpoint,
                    &file_store,
                ))
            }
            _ => {
                panic!("invalid value for BS_READER_MODE enviroment variable: expected 'file', 'rpc', or 'rpc_cached'")
            }
        }
    }
}

impl BeaconStateReader for BeaconStateReaderEnum {
    async fn read_beacon_state(&self, state_id: &StateId) -> anyhow::Result<BeaconState> {
        match self {
            Self::File(reader) => reader.read_beacon_state(state_id).await,
            Self::RPC(reader) => reader.read_beacon_state(state_id).await,
            Self::RPCCached(reader) => reader.read_beacon_state(state_id).await,
        }
    }

    async fn read_beacon_block_header(&self, state_id: &StateId) -> anyhow::Result<BeaconBlockHeader> {
        match self {
            Self::File(reader) => reader.read_beacon_block_header(state_id).await,
            Self::RPC(reader) => reader.read_beacon_block_header(state_id).await,
            Self::RPCCached(reader) => reader.read_beacon_block_header(state_id).await,
        }
    }
}
