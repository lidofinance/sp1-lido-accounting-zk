use sp1_lido_accounting_dev_scripts::lido;
use sp1_lido_accounting_scripts::prometheus_metrics;
use sp1_lido_accounting_zk_shared::{io::eth_io::BeaconChainSlot, lido::LidoValidatorState};

use std::path::PathBuf;
use std::sync::Arc;
use tree_hash::TreeHash;

use sp1_lido_accounting_dev_scripts::synthetic::{
    BalanceGenerationMode, GenerationSpec, SyntheticBeaconStateCreator,
};
use sp1_lido_accounting_scripts::beacon_state_reader::{
    file::FileBasedBeaconStateReader, BeaconStateReader, StateId,
};
use sp1_lido_accounting_zk_shared::eth_consensus_layer::Hash256;
use sp1_lido_accounting_zk_shared::merkle_proof::FieldProof;

use simple_logger::SimpleLogger;

#[tokio::main]
async fn main() {
    SimpleLogger::new().env().init().unwrap();
    let env = std::env::var("EVM_CHAIN").expect("EVM_CHAIN not set");
    let ssz_folder = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../temp/")
        .join(env);
    let creator = SyntheticBeaconStateCreator::new(&ssz_folder, false, true)
        .expect("Failed to create synthetic beacon state creator");
    let reader: FileBasedBeaconStateReader = FileBasedBeaconStateReader::new(
        &ssz_folder,
        Arc::new(prometheus_metrics::build_service_metrics(
            "namespace",
            "file_reader",
        )),
    )
    .expect("Failed to create beacon state reader");
    let withdrawal_creds: Hash256 = lido::withdrawal_credentials::MAINNET.into();
    let old_slot = 100;
    let new_slot = 200;
    let base_state_spec = GenerationSpec {
        slot: old_slot,
        non_lido_validators: 2_u64.pow(10),
        deposited_lido_validators: 2_u64.pow(9),
        exited_lido_validators: 2_u64.pow(3),
        pending_deposit_lido_validators: 2_u64.pow(2),
        balances_generation_mode: BalanceGenerationMode::FIXED,
        shuffle: false,
        base_slot: None,
        overwrite: true,
    };
    let update_state_spec = GenerationSpec {
        slot: new_slot,
        non_lido_validators: 27,
        deposited_lido_validators: 8,
        exited_lido_validators: 0,
        pending_deposit_lido_validators: 2,
        balances_generation_mode: BalanceGenerationMode::FIXED,
        shuffle: false,
        base_slot: Some(base_state_spec.slot),
        overwrite: true,
    };

    creator
        .create_beacon_state(base_state_spec)
        .await
        .unwrap_or_else(|_| panic!("Failed to create beacon state for slot {}", old_slot));

    creator
        .create_beacon_state(update_state_spec)
        .await
        .unwrap_or_else(|_| panic!("Failed to create beacon state for slot {}", new_slot));

    let beacon_state1 = reader
        .read_beacon_state(&StateId::Slot(BeaconChainSlot(old_slot)))
        .await
        .expect("Failed to read beacon state");
    let lido_state1 =
        LidoValidatorState::compute_from_beacon_state(&beacon_state1, &withdrawal_creds);

    let beacon_state2 = reader
        .read_beacon_state(&StateId::Slot(BeaconChainSlot(new_slot)))
        .await
        .expect("Failed to read beacon state");
    let lido_state2 =
        LidoValidatorState::compute_from_beacon_state(&beacon_state1, &withdrawal_creds);

    let highest_validator_index1: usize = lido_state1
        .max_validator_index
        .try_into()
        .expect("Failed to convert max_validator_index to usize");
    let highest_validator_index2 = beacon_state2.validators.len() - 1;
    let new_validator_indices = Vec::from_iter(highest_validator_index1..highest_validator_index2);
    // let all_lido_validator_indices: Vec<usize> = lido_state2
    //     .deposited_lido_validator_indices
    //     .iter()
    //     .map(|v| usize::try_from(*v).unwrap())
    //     .collect();

    let new_validators_multiproof = beacon_state2
        .validators
        .get_members_multiproof(new_validator_indices.as_slice());

    tracing::info!("New validators {}", new_validator_indices.len());
    tracing::debug!(
        "Validators proof hashes: {:?}",
        new_validators_multiproof.proof_hashes_hex()
    );
    tracing::info!("Validating validators proof");
    let validators_to_prove: Vec<Hash256> = new_validator_indices
        .iter()
        .map(|idx| beacon_state2.validators[*idx].tree_hash_root())
        .collect();
    beacon_state2
        .validators
        .verify_instance(
            &new_validators_multiproof,
            &new_validator_indices,
            validators_to_prove.as_slice(),
        )
        .expect("Failed to validate multiproof");

    // let lido_balances_multiproof = beacon_state2
    //     .balances
    //     .get_field_multiproof(all_lido_validator_indices.as_slice());
    // tracing::info!("Lido validators {}", all_lido_validator_indices.len());
    // tracing::debug!(
    //     "Lido balances proof hashes: {:?}",
    //     lido_balances_multiproof.proof_hashes_hex()
    // );
    // tracing::info!("Validating balances proof");
    // beacon_state2
    //     .balances
    //     .verify(&lido_balances_multiproof, &new_validator_indices)
    //     .expect("Failed to validate multiproof");

    tracing::info!("Successfully validated multiproofs");
}
