use crate::eth_consensus_layer::BeaconState;
use anyhow::Result;

pub trait BeaconStateReader {
    async fn read_beacon_state(&self, slot: u64) -> Result<BeaconState>;
}

#[cfg(feature = "synthetic")]
pub mod local_synthetic;
