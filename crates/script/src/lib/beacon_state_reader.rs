use std::future::Future;

use anyhow;
use sp1_lido_accounting_zk_shared::io::eth_io::{BeaconChainSlot, ReferenceSlot};

use sp1_lido_accounting_zk_shared::eth_consensus_layer::{BeaconBlockHeader, BeaconState, Hash256};

pub mod file;
pub mod reqwest;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("Failed to initialize due to io error {0:?}")]
    IoError(#[from] std::io::Error),
}

#[derive(Debug, Hash, PartialEq, Eq, Clone)]
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

pub trait BeaconStateReader {
    fn read_beacon_state(&self, state_id: &StateId) -> impl Future<Output = anyhow::Result<BeaconState>> + Send;
    fn read_beacon_block_header(
        &self,
        state_id: &StateId,
    ) -> impl Future<Output = anyhow::Result<BeaconBlockHeader>> + Send;
}

pub trait RefSlotResolver {
    fn find_bc_slot_for_refslot(
        &self,
        target_slot: ReferenceSlot,
    ) -> impl Future<Output = anyhow::Result<BeaconChainSlot>> + Send;
}
