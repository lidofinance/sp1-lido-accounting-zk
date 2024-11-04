use anyhow;
use sp1_lido_accounting_zk_shared::io::eth_io::{BeaconChainSlot, HaveSlotWithBlock, ReferenceSlot};
use tree_hash::TreeHash;

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

#[derive(Hash, PartialEq, Eq, Clone)]
pub enum StateId {
    Head,
    Genesis,
    Finalized,
    Justified,
    Slot(BeaconChainSlot),
    Hash(Hash256),
}

impl StateId {
    fn as_str(&self) -> String {
        match self {
            Self::Head => "head".to_owned(),
            Self::Genesis => "genesis".to_owned(),
            Self::Finalized => "finalized".to_owned(),
            Self::Justified => "justified".to_owned(),
            Self::Slot(slot) => slot.0.to_string(),
            Self::Hash(hash) => hex::encode(hash),
        }
    }
}

const MAX_REFERENCE_LOOKBACK_ATTEMPTS: u32 = 60 /*m*/ * 60 /*s*/ / 12 /*sec _per_slot*/;
const LOG_LOOKBACK_ATTEMPT_DELAY: u32 = 20;
const LOG_LOOKBACK_ATTEMPT_INTERVAL: u32 = 10;

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

    #[allow(async_fn_in_trait)]
    async fn find_bc_slot_for_refslot(&self, target_slot: ReferenceSlot) -> anyhow::Result<BeaconChainSlot> {
        let mut attempt_slot: u64 = target_slot.0;
        let mut attempt_count: u32 = 0;
        let max_lookback_slot = target_slot.0 - u64::from(MAX_REFERENCE_LOOKBACK_ATTEMPTS);
        log::info!("Finding non-empty slot for reference slot {target_slot} searching from {target_slot} to {max_lookback_slot}");
        loop {
            log::debug!("Checking slot {attempt_slot}");
            let maybe_header = self
                .read_beacon_block_header(&StateId::Slot(BeaconChainSlot(attempt_slot)))
                .await;
            match maybe_header {
                Ok(bh) => {
                    let result = bh.bc_slot();
                    let hash = bh.tree_hash_root();
                    log::info!("Found non-empty slot {result} with hash {hash:#x}");
                    return Ok(result);
                }
                Err(error) => {
                    if attempt_count == MAX_REFERENCE_LOOKBACK_ATTEMPTS {
                        log::error!("Couldn't find non-empty slot for reference slot {target_slot} between {target_slot} and {max_lookback_slot}");
                        return Err(error);
                    }
                    if attempt_count >= LOG_LOOKBACK_ATTEMPT_DELAY && attempt_count % LOG_LOOKBACK_ATTEMPT_INTERVAL == 0
                    {
                        log::warn!("Cannot find non-empty slot for reference slot {target_slot} for {attempt_count} attempts; last checked slot {attempt_slot}")
                    }
                    attempt_count += 1;
                    attempt_slot -= 1;
                }
            }
        }
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
