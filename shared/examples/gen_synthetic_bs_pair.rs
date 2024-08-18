use log;
use util::synthetic_beacon_state_reader::GenerationSpec;

use std::path::PathBuf;
use tree_hash::TreeHash;

mod util;

use crate::util::synthetic_beacon_state_reader::{BalanceGenerationMode, SyntheticBeaconStateCreator};
use sp1_lido_accounting_zk_shared::beacon_state_reader::{BeaconStateReader, FileBasedBeaconStateReader};

use simple_logger::SimpleLogger;

#[tokio::main]
async fn main() {
    SimpleLogger::new().env().init().unwrap();
    let file_store = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../temp");

    let creator = SyntheticBeaconStateCreator::new(&file_store, false, true);
    let old_slot = 2000000;
    let new_slot = 2100000;
    let base_state_spec = GenerationSpec {
        slot: old_slot,
        non_lido_validators: 2_u64.pow(10),
        deposited_lido_validators: 2_u64.pow(9),
        exited_lido_validators: 2_u64.pow(3),
        future_deposit_lido_validators: 2_u64.pow(2),
        balances_generation_mode: BalanceGenerationMode::FIXED,
        shuffle: false,
        base_slot: None,
        overwrite: true,
    };
    let update_state_spec = GenerationSpec {
        slot: new_slot,
        non_lido_validators: 2_u64.pow(5),
        deposited_lido_validators: 2_u64.pow(4),
        exited_lido_validators: 2_u64.pow(2),
        future_deposit_lido_validators: 2_u64.pow(1),
        balances_generation_mode: BalanceGenerationMode::FIXED,
        shuffle: false,
        base_slot: Some(base_state_spec.slot),
        overwrite: true,
    };

    creator
        .create_beacon_state(base_state_spec)
        .await
        .expect(&format!("Failed to create beacon state for slot {}", old_slot));

    creator
        .create_beacon_state(update_state_spec)
        .await
        .expect(&format!("Failed to create beacon state for slot {}", new_slot));

    let bs_reader: FileBasedBeaconStateReader = FileBasedBeaconStateReader::new(&file_store);

    let beacon_state1 = bs_reader
        .read_beacon_state(old_slot)
        .await
        .expect("Failed to read beacon state");
    let beacon_block_header1 = bs_reader
        .read_beacon_block_header(old_slot)
        .await
        .expect("Failed to read beacon block header");
    log::info!(
        "Read Beacon State for slot {:?}, with {} validators, beacon block hash: {}",
        beacon_state1.slot,
        beacon_state1.validators.to_vec().len(),
        hex::encode(beacon_block_header1.tree_hash_root())
    );

    let beacon_state2 = bs_reader
        .read_beacon_state(new_slot)
        .await
        .expect("Failed to read beacon state");
    let beacon_block_header2 = bs_reader
        .read_beacon_block_header(new_slot)
        .await
        .expect("Failed to read beacon block header");
    log::info!(
        "Read Beacon State for slot {:?}, with {} validators, beacon block hash: {}",
        beacon_state2.slot,
        beacon_state2.validators.to_vec().len(),
        hex::encode(beacon_block_header2.tree_hash_root())
    );
}
