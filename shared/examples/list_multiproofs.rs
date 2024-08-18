use log;
use sp1_lido_accounting_zk_shared::lido::LidoValidatorState;
use util::synthetic_beacon_state_reader::GenerationSpec;

use std::path::PathBuf;
use tree_hash::TreeHash;

mod util;

use crate::util::synthetic_beacon_state_reader::{BalanceGenerationMode, SyntheticBeaconStateCreator};
use sp1_lido_accounting_zk_shared::eth_consensus_layer::Hash256;
use sp1_lido_accounting_zk_shared::verification::FieldProof;
use sp1_lido_accounting_zk_shared::{
    beacon_state_reader::{BeaconStateReader, FileBasedBeaconStateReader},
    consts,
};

use simple_logger::SimpleLogger;

#[tokio::main]
async fn main() {
    SimpleLogger::new().env().init().unwrap();
    let file_store = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../temp");
    let creator = SyntheticBeaconStateCreator::new(&file_store, false, true);
    let reader: FileBasedBeaconStateReader = FileBasedBeaconStateReader::new(&file_store);
    let withdrawal_creds: Hash256 = consts::LIDO_WITHDRAWAL_CREDENTIALS.into();
    let old_slot = 100;
    let new_slot = 200;
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
        deposited_lido_validators: 0,
        exited_lido_validators: 0,
        future_deposit_lido_validators: 0,
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

    let beacon_state1 = reader
        .read_beacon_state(old_slot)
        .await
        .expect("Failed to read beacon state");
    let lido_state1 = LidoValidatorState::compute_from_beacon_state(&beacon_state1, &withdrawal_creds);

    let beacon_state2 = reader
        .read_beacon_state(new_slot)
        .await
        .expect("Failed to read beacon state");

    let highest_validator_index1 = lido_state1.max_validator_index as usize;
    let highest_validator_index2 = beacon_state2.validators.len() - 1;
    let new_validator_indices = Vec::from_iter(highest_validator_index1..highest_validator_index2);

    let validators_multiproof = beacon_state2
        .validators
        .get_field_multiproof(new_validator_indices.as_slice());

    let balances_multiproof = beacon_state2
        .balances
        .get_field_multiproof(new_validator_indices.as_slice());

    log::info!("New validators {}", new_validator_indices.len());
    log::debug!(
        "Validators proof hashes: {:?}",
        validators_multiproof.proof_hashes_hex()
    );
    log::debug!("Balances proof hashes: {:?}", balances_multiproof.proof_hashes_hex());

    log::info!("Validating validators proof");
    beacon_state2
        .validators
        .verify(&validators_multiproof, &new_validator_indices)
        .expect("Failed to validate multiproof");

    log::info!("Validating balances proof");
    beacon_state2
        .balances
        .verify(&balances_multiproof, &new_validator_indices)
        .expect("Failed to validate multiproof");

    log::info!("Successfully validated multiproofs");
}
