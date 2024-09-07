use anyhow;

use crate::eth_consensus_layer::{BeaconBlockHeader, BeaconState};

pub mod file;
#[cfg(feature = "reqwest")]
pub mod reqwest;
#[cfg(feature = "synthetic")]
pub mod synthetic;

pub trait BeaconStateReader {
    #[allow(async_fn_in_trait)]
    async fn read_beacon_state(&self, slot: u64) -> anyhow::Result<BeaconState>;
    #[allow(async_fn_in_trait)]
    async fn read_beacon_block_header(&self, slot: u64) -> anyhow::Result<BeaconBlockHeader>;
}
