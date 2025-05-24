use anyhow;
use sp1_lido_accounting_zk_shared::io::eth_io::{BeaconChainSlot, HaveSlotWithBlock, ReferenceSlot};
use tree_hash::TreeHash;

use sp1_lido_accounting_zk_shared::eth_consensus_layer::{BeaconBlockHeader, BeaconState, Hash256};

pub mod file;
pub mod reqwest;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("Failed to initialize due to io error {0:?}")]
    IoError(#[from] std::io::Error),
}

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

    pub fn get_permanent_str(&self) -> anyhow::Result<String> {
        match self {
            StateId::Slot(slot_id) => Ok(slot_id.to_string()),
            StateId::Hash(block_hash) => Ok(hex::encode(block_hash)),
            _ => Err(anyhow::anyhow!("Cannot read transient state ids from file reader")),
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
        tracing::info!("Finding non-empty slot for reference slot {target_slot} searching from {target_slot} to {max_lookback_slot}");
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
