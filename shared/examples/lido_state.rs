use hex::FromHex;
use log;
use serde_json::Value;

use sp1_lido_accounting_zk_shared::lido::LidoValidatorState;
use std::collections::HashSet;
use std::path::PathBuf;
use tree_hash::TreeHash;

mod util;
use crate::util::synthetic_beacon_state_reader::{BalanceGenerationMode, SyntheticBeaconStateCreator};
use sp1_lido_accounting_zk_shared::beacon_state_reader::BeaconStateReader;
use sp1_lido_accounting_zk_shared::eth_consensus_layer::{epoch, BeaconState, Hash256};
use sp1_lido_accounting_zk_shared::util::usize_to_u64;

use simple_logger::SimpleLogger;

fn hex_str_to_h256(hex_str: &str) -> Hash256 {
    <[u8; 32]>::from_hex(hex_str)
        .expect("Couldn't parse hex_str as H256")
        .into()
}

fn verify_state(beacon_state: &BeaconState, state: &LidoValidatorState, manifesto: &Value) {
    assert_eq!(state.slot, manifesto["report"]["slot"].as_u64().unwrap());
    assert_eq!(state.epoch, manifesto["report"]["epoch"].as_u64().unwrap());
    assert_eq!(
        usize_to_u64(state.deposited_lido_validator_indices.len()),
        manifesto["report"]["lido_deposited_validators"].as_u64().unwrap()
    );
    assert_eq!(
        usize_to_u64(state.exited_lido_validator_indices.len()),
        manifesto["report"]["lido_exited_validators"].as_u64().unwrap()
    );
    assert_eq!(
        usize_to_u64(state.future_deposit_lido_validator_indices.len()),
        manifesto["report"]["lido_future_deposit_validators"].as_u64().unwrap()
    );
    assert_eq!(
        state.max_validator_index,
        manifesto["report"]["total_validators"].as_u64().unwrap() - 1
    );

    let epoch = epoch(beacon_state.slot).unwrap();
    let withdrawal_creds = hex_str_to_h256(manifesto["report"]["lido_withdrawal_credentials"].as_str().unwrap());

    let deposited_hash_set: HashSet<u64> = HashSet::from_iter(state.deposited_lido_validator_indices.clone());
    let future_deposit_hash_set: HashSet<u64> = HashSet::from_iter(state.future_deposit_lido_validator_indices.clone());
    let exited_hash_set: HashSet<u64> = HashSet::from_iter(state.exited_lido_validator_indices.clone());

    for (idx, validator) in beacon_state.validators.iter().enumerate() {
        let validator_index = usize_to_u64(idx);

        if validator.withdrawal_credentials != withdrawal_creds {
            assert!(!deposited_hash_set.contains(&validator_index));
            assert!(!future_deposit_hash_set.contains(&validator_index));
            assert!(!exited_hash_set.contains(&validator_index));
        } else {
            if epoch >= validator.activation_eligibility_epoch {
                assert!(deposited_hash_set.contains(&validator_index));
            } else {
                assert!(future_deposit_hash_set.contains(&validator_index));
            }

            if epoch >= validator.exit_epoch {
                assert!(exited_hash_set.contains(&validator_index));
            }
        }
    }
}

#[tokio::main]
async fn main() {
    SimpleLogger::new().env().init().unwrap();
    // Step 1. obtain SSZ-serialized beacon state
    // For now using a "synthetic" generator based on reference implementation (py-ssz)
    let total_validators_log2 = 7;
    let lido_validators_log2 = total_validators_log2 - 1;
    let creator = SyntheticBeaconStateCreator::new(
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../temp"),
        2_u64.pow(total_validators_log2),
        2_u64.pow(lido_validators_log2),
        BalanceGenerationMode::SEQUENTIAL,
        true,
        true,
        true,
    );

    let slot = 123456;
    creator.evict_cache(slot).expect("Failed to evict cache");
    creator
        .create_beacon_state(slot, true)
        .await
        .expect("Failed to create beacon state");

    let reader = creator.get_file_reader(slot);

    let beacon_state = reader
        .read_beacon_state(slot)
        .await
        .expect("Failed to read beacon state");
    log::info!(
        "Read Beacon State for slot {:?}, with {} validators",
        beacon_state.slot,
        beacon_state.validators.to_vec().len(),
    );

    // Step 2: read manifesto
    let manifesto = creator
        .read_manifesto(slot)
        .await
        .expect("Failed to read manifesto json");
    let lido_withdrawal_creds = hex_str_to_h256(manifesto["report"]["lido_withdrawal_credentials"].as_str().unwrap());

    // Step 3: Compute lido state
    let lido_state = LidoValidatorState::compute_from_beacon_state(&beacon_state, &lido_withdrawal_creds);

    // Step 4: verify state
    verify_state(&beacon_state, &lido_state, &manifesto);

    // Step 5: ensure report merkle root computes
    let merkle_root = lido_state.tree_hash_root();
    log::info!("State merkle root {}", hex::encode(merkle_root));
    log::debug!(
        "Deposited validators: {:?}",
        lido_state.deposited_lido_validator_indices.to_vec()
    );
    log::debug!(
        "Future deposit validators: {:?}",
        lido_state.future_deposit_lido_validator_indices.to_vec()
    );
    log::debug!(
        "Exited validators: {:?}",
        lido_state.exited_lido_validator_indices.to_vec()
    );
}
