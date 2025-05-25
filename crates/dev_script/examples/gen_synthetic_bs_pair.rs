use simple_logger::SimpleLogger;
use sp1_lido_accounting_zk_shared::io::eth_io::BeaconChainSlot;
use std::path::PathBuf;
use tree_hash::TreeHash;

use sp1_lido_accounting_dev_scripts::synthetic::{
    BalanceGenerationMode, GenerationSpec, SyntheticBeaconStateCreator,
};

use sp1_lido_accounting_scripts::beacon_state_reader::{
    file::FileBasedBeaconStateReader, BeaconStateReader, StateId,
};

fn small_problem_size(old_slot: u64, new_slot: u64) -> (GenerationSpec, GenerationSpec) {
    let base_state_spec = GenerationSpec {
        slot: old_slot,
        non_lido_validators: 2_u64.pow(11),
        deposited_lido_validators: 2_u64.pow(11),
        exited_lido_validators: 2_u64.pow(7),
        pending_deposit_lido_validators: 2_u64.pow(5),
        balances_generation_mode: BalanceGenerationMode::FIXED,
        shuffle: false,
        base_slot: None,
        overwrite: true,
    };
    let update_state_spec = GenerationSpec {
        slot: new_slot,
        non_lido_validators: 2_u64.pow(7),
        deposited_lido_validators: 2_u64.pow(7),
        exited_lido_validators: 2_u64.pow(3),
        pending_deposit_lido_validators: 2_u64.pow(3),
        balances_generation_mode: BalanceGenerationMode::FIXED,
        shuffle: false,
        base_slot: Some(base_state_spec.slot),
        overwrite: true,
    };
    (base_state_spec, update_state_spec)
}

fn large_problem_size(old_slot: u64, new_slot: u64) -> (GenerationSpec, GenerationSpec) {
    let base_state_spec = GenerationSpec {
        slot: old_slot,
        non_lido_validators: 2_u64.pow(16),
        deposited_lido_validators: 2_u64.pow(16),
        exited_lido_validators: 2_u64.pow(12),
        pending_deposit_lido_validators: 2_u64.pow(9),
        balances_generation_mode: BalanceGenerationMode::FIXED,
        shuffle: false,
        base_slot: None,
        overwrite: true,
    };
    let update_state_spec = GenerationSpec {
        slot: new_slot,
        non_lido_validators: 2_u64.pow(11),
        deposited_lido_validators: 2_u64.pow(11),
        exited_lido_validators: 2_u64.pow(5),
        pending_deposit_lido_validators: 2_u64.pow(5),
        balances_generation_mode: BalanceGenerationMode::FIXED,
        shuffle: false,
        base_slot: Some(base_state_spec.slot),
        overwrite: true,
    };
    (base_state_spec, update_state_spec)
}

#[tokio::main]
async fn main() {
    SimpleLogger::new().env().init().unwrap();
    let env = std::env::var("EVM_CHAIN").expect("EVM_CHAIN not set");
    let ssz_folder = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../temp/")
        .join(env);

    let creator = SyntheticBeaconStateCreator::new(&ssz_folder, false, true)
        .expect("Failed to create synthetic beacon state creator");
    let old_slot = 5000000;
    let new_slot = 5800000;

    let (base_state_spec, update_state_spec) = small_problem_size(old_slot, new_slot);
    // let (base_state_spec, update_state_spec) = large_problem_size(old_slot, new_slot);

    creator
        .create_beacon_state(base_state_spec)
        .await
        .unwrap_or_else(|_| panic!("Failed to create beacon state for slot {}", old_slot));

    creator
        .create_beacon_state(update_state_spec)
        .await
        .unwrap_or_else(|_| panic!("Failed to create beacon state for slot {}", new_slot));

    let bs_reader: FileBasedBeaconStateReader = FileBasedBeaconStateReader::new(&ssz_folder)
        .expect("Failed to create file based beacon state reader");

    let beacon_state1 = bs_reader
        .read_beacon_state(&StateId::Slot(BeaconChainSlot(old_slot)))
        .await
        .expect("Failed to read beacon state");
    let beacon_block_header1 = bs_reader
        .read_beacon_block_header(&StateId::Slot(BeaconChainSlot(old_slot)))
        .await
        .expect("Failed to read beacon block header");
    tracing::info!(
        "Read Beacon State for slot {:?}, with {} validators, beacon block hash: {}",
        beacon_state1.slot,
        beacon_state1.validators.to_vec().len(),
        hex::encode(beacon_block_header1.tree_hash_root())
    );

    let beacon_state2 = bs_reader
        .read_beacon_state(&StateId::Slot(BeaconChainSlot(new_slot)))
        .await
        .expect("Failed to read beacon state");
    let beacon_block_header2 = bs_reader
        .read_beacon_block_header(&StateId::Slot(BeaconChainSlot(new_slot)))
        .await
        .expect("Failed to read beacon block header");
    tracing::info!(
        "Read Beacon State for slot {:?}, with {} validators, beacon block hash: {}",
        beacon_state2.slot,
        beacon_state2.validators.to_vec().len(),
        hex::encode(beacon_block_header2.tree_hash_root())
    );
}
