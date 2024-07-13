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
        2_u64.pow(10),
        2_u64.pow(5),
        BalanceGenerationMode::FIXED,
        true,
        true,
    );

    let slot = 111222333;
    let beacon_state = reader.read_beacon_state(slot).await;
    println!("Beacon State {:?}", beacon_state);
}
