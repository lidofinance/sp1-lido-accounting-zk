use sp1_lido_accounting_zk_shared::{io::eth_io::BeaconChainSlot, lido::LidoValidatorState};

use std::path::PathBuf;
use tree_hash::TreeHash;

use sp1_lido_accounting_scripts::beacon_state_reader::{
    file::FileBasedBeaconStateReader,
    synthetic::{BalanceGenerationMode, GenerationSpec, SyntheticBeaconStateCreator},
    BeaconStateReader, StateId,
};
use sp1_lido_accounting_scripts::consts;
use sp1_lido_accounting_zk_shared::eth_consensus_layer::Hash256;
use sp1_lido_accounting_zk_shared::merkle_proof::{FieldProof, RsMerkleHash};

use simple_logger::SimpleLogger;

#[tokio::main]
async fn main() {
    SimpleLogger::new().env().init().unwrap();
    let file_store = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../temp");
    let creator = SyntheticBeaconStateCreator::new(&file_store, false, true);
    let reader: FileBasedBeaconStateReader = FileBasedBeaconStateReader::new(&file_store);
    let withdrawal_creds: Hash256 = consts::lido_credentials::MAINNET.into();
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
    let lido_state1 = LidoValidatorState::compute_from_beacon_state(&beacon_state1, &withdrawal_creds);

    let beacon_state2 = reader
        .read_beacon_state(&StateId::Slot(BeaconChainSlot(new_slot)))
        .await
        .expect("Failed to read beacon state");
    let lido_state2 = LidoValidatorState::compute_from_beacon_state(&beacon_state1, &withdrawal_creds);

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

    log::info!("New validators {}", new_validator_indices.len());
    log::debug!(
        "Validators proof hashes: {:?}",
        new_validators_multiproof.proof_hashes_hex()
    );
    log::info!("Validating validators proof");
    let validators_to_prove: Vec<RsMerkleHash> = new_validator_indices
        .iter()
        .map(|idx| beacon_state2.validators[*idx].tree_hash_root().to_fixed_bytes())
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
    // log::info!("Lido validators {}", all_lido_validator_indices.len());
    // log::debug!(
    //     "Lido balances proof hashes: {:?}",
    //     lido_balances_multiproof.proof_hashes_hex()
    // );
    // log::info!("Validating balances proof");
    // beacon_state2
    //     .balances
    //     .verify(&lido_balances_multiproof, &new_validator_indices)
    //     .expect("Failed to validate multiproof");

    log::info!("Successfully validated multiproofs");
}
