use eth_consensus_layer_ssz::BeaconState;
use std::path::PathBuf;

mod beacon_state_reader;
use crate::beacon_state_reader::{
    BalanceGenerationMode, BeaconStateReader, SyntheticBeaconStateReader,
};

use simple_logger::SimpleLogger;

#[tokio::main]
async fn main() {
    SimpleLogger::new().init().unwrap();
    // Step 1. obtain SSZ-serialized beacon state
    // For now using a "synthetic" generator based on reference implementation (py-ssz)
    let reader = SyntheticBeaconStateReader::new(
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../temp"),
        2_u64.pow(12),
        2_u64.pow(6),
        BalanceGenerationMode::RANDOM,
        true,
        true,
    );

    let slot = 1000000;
    let beacon_state = reader.read_beacon_state(slot).await;
    println!(
        "Beacon State {:?}, validators: {:?}",
        beacon_state.slot,
        beacon_state.validators.to_vec().len(),
    );

    // println!("Balances {:?}", beacon_state.balances.to_vec());
}
