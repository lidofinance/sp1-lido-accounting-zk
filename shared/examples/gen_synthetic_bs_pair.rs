use hex::FromHex;
use log;
use serde_json::Value;

use std::path::PathBuf;
use tree_hash::TreeHash;

mod util;

use crate::util::synthetic_beacon_state_reader::{BalanceGenerationMode, SyntheticBeaconStateCreator};
use sp1_lido_accounting_zk_shared::beacon_state_reader::BeaconStateReader;
use sp1_lido_accounting_zk_shared::eth_consensus_layer::{BeaconBlockHeader, BeaconState, Hash256};
use sp1_lido_accounting_zk_shared::verification::{FieldProof, MerkleTreeFieldLeaves};

use simple_logger::SimpleLogger;

#[tokio::main]
async fn main() {
    SimpleLogger::new().env().init().unwrap();
    // Step 1. obtain SSZ-serialized beacon state
    // For now using a "synthetic" generator based on reference implementation (py-ssz)
    let total_validators_log2 = 8;
    let lido_validators_log2 = total_validators_log2 - 1;
    let creator1 = SyntheticBeaconStateCreator::new(
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../temp"),
        2_u64.pow(total_validators_log2),
        2_u64.pow(lido_validators_log2),
        BalanceGenerationMode::SEQUENTIAL,
        true,
        true,
        false,
    );
    let creator2 = SyntheticBeaconStateCreator::new(
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../temp"),
        2_u64.pow(6),
        2_u64.pow(5),
        BalanceGenerationMode::SEQUENTIAL,
        true,
        true,
        false,
    );

    let slot1 = 1000000;
    let slot2 = 1100000;

    creator1
        .evict_cache(slot1)
        .expect(&format!("Failed to evict cache for slot {}", slot1));

    creator1
        .evict_cache(slot2)
        .expect(&format!("Failed to evict cache for slot {}", slot2));

    creator1
        .create_beacon_state(slot1, true)
        .await
        .expect(&format!("Failed to create beacon state for slot {}", slot1));

    creator2
        .create_beacon_state_from_base(slot2, slot1, true)
        .await
        .expect(&format!(
            "Failed to create beacon state for slot {} from slot {}",
            slot2, slot1
        ));

    let reader1 = creator1.get_file_reader(slot1);
    let beacon_state1 = reader1
        .read_beacon_state(slot1)
        .await
        .expect("Failed to read beacon state");
    let beacon_block_header1 = reader1
        .read_beacon_block_header(slot1)
        .await
        .expect("Failed to read beacon block header");
    log::info!(
        "Read Beacon State for slot {:?}, with {} validators, beacon block hash: {}",
        beacon_state1.slot,
        beacon_state1.validators.to_vec().len(),
        hex::encode(beacon_block_header1.tree_hash_root())
    );

    let reader2 = creator2.get_file_reader(slot2);
    let beacon_state2 = reader2
        .read_beacon_state(slot1)
        .await
        .expect("Failed to read beacon state");
    let beacon_block_header2 = reader2
        .read_beacon_block_header(slot2)
        .await
        .expect("Failed to read beacon block header");
    log::info!(
        "Read Beacon State for slot {:?}, with {} validators, beacon block hash: {}",
        beacon_state2.slot,
        beacon_state2.validators.to_vec().len(),
        hex::encode(beacon_block_header2.tree_hash_root())
    );
}
