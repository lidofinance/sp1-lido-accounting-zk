use crate::validator_delta::ValidatorDeltaCompute;
use alloy_sol_types::SolType;

use sp1_sdk::SP1PublicValues;

use sp1_lido_accounting_zk_shared::circuit_logic::input_verification::{InputVerifier, LogCycleTracker};
use sp1_lido_accounting_zk_shared::circuit_logic::report::ReportData;
use sp1_lido_accounting_zk_shared::eth_consensus_layer::{BeaconBlockHeader, BeaconState, Hash256, Slot};
use sp1_lido_accounting_zk_shared::io::eth_io::{
    LidoValidatorStateRust, PublicValuesRust, PublicValuesSolidity, ReportMetadataRust, ReportRust,
};
use sp1_lido_accounting_zk_shared::io::program_io::{ProgramInput, ValsAndBals};
use sp1_lido_accounting_zk_shared::lido::LidoValidatorState;
use sp1_lido_accounting_zk_shared::merkle_proof::{FieldProof, MerkleTreeFieldLeaves};
use sp1_lido_accounting_zk_shared::util::{u64_to_usize, usize_to_u64};

use anyhow::Result;

use tree_hash::TreeHash;

pub fn prepare_program_input(
    bs: &BeaconState,
    bh: &BeaconBlockHeader,
    old_bs: &BeaconState,
    lido_withdrawal_credentials: &Hash256,
    verify: bool,
) -> (ProgramInput, PublicValuesRust) {
    let beacon_block_hash = bh.tree_hash_root();

    log::info!(
        "Processing BeaconState. Current slot: {}, Previous Slot: {}, Block Hash: {}, Validator count:{}",
        bs.slot,
        old_bs.slot,
        hex::encode(beacon_block_hash),
        bs.validators.len()
    );
    let old_validator_state = LidoValidatorState::compute_from_beacon_state(old_bs, lido_withdrawal_credentials);
    let new_validator_state = LidoValidatorState::compute_from_beacon_state(bs, lido_withdrawal_credentials);

    log::info!(
        "Computed validator states. Old: deposited {}, pending {}, exited {}. New: deposited {}, pending {}, exited {}",
        old_validator_state.deposited_lido_validator_indices.len(),
        old_validator_state.pending_deposit_lido_validator_indices.len(),
        old_validator_state.exited_lido_validator_indices.len(),
        new_validator_state.deposited_lido_validator_indices.len(),
        new_validator_state.pending_deposit_lido_validator_indices.len(),
        new_validator_state.exited_lido_validator_indices.len(),
    );

    let report = ReportData::compute(
        bs.slot,
        bs.epoch(),
        &bs.validators,
        &bs.balances,
        lido_withdrawal_credentials,
    );

    let public_values: PublicValuesRust = PublicValuesRust {
        report: ReportRust {
            slot: report.slot,
            deposited_lido_validators: report.deposited_lido_validators,
            exited_lido_validators: report.exited_lido_validators,
            lido_cl_balance: report.lido_cl_balance,
        },
        metadata: ReportMetadataRust {
            slot: report.slot,
            epoch: report.epoch,
            lido_withdrawal_credentials: lido_withdrawal_credentials.to_fixed_bytes(),
            beacon_block_hash: beacon_block_hash.to_fixed_bytes(),
            state_for_previous_report: LidoValidatorStateRust {
                slot: old_validator_state.slot,
                merkle_root: old_validator_state.tree_hash_root().to_fixed_bytes(),
            },
            new_state: LidoValidatorStateRust {
                slot: new_validator_state.slot,
                merkle_root: new_validator_state.tree_hash_root().to_fixed_bytes(),
            },
        },
    };

    log::info!("Computed report and public values");
    log::debug!("Report {report:?}");
    log::debug!("Public values {public_values:?}");

    let validator_delta = ValidatorDeltaCompute::new(old_bs, &old_validator_state, bs, !verify).compute();
    log::info!(
        "Computed validator delta. Added: {}, lido changed: {}",
        validator_delta.all_added.len(),
        validator_delta.lido_changed.len(),
    );
    let added_indices: Vec<usize> = validator_delta.added_indices().map(|v| u64_to_usize(*v)).collect();
    let changed_indices: Vec<usize> = validator_delta
        .lido_changed_indices()
        .map(|v| u64_to_usize(*v))
        .collect();

    let added_validators_proof = bs.validators.get_serialized_multiproof(added_indices.as_slice());
    let changed_validators_proof = bs.validators.get_serialized_multiproof(changed_indices.as_slice());
    log::info!("Obtained added and changed validators multiproofs");

    let bs_indices = bs
        .get_leafs_indices(["validators", "balances"])
        .expect("Failed to get BeaconState field indices");
    let validators_and_balances_proof = bs.get_serialized_multiproof(bs_indices.as_slice());
    log::info!("Obtained validators and balances fields multiproof");

    log::info!("Creating program input");
    let program_input = ProgramInput {
        slot: bs.slot,
        beacon_block_hash,
        beacon_block_header: bh.into(),
        beacon_state: bs.into(),
        validators_and_balances: ValsAndBals {
            validators_and_balances_proof,
            lido_withdrawal_credentials: *lido_withdrawal_credentials,
            total_validators: usize_to_u64(bs.validators.len()),
            validators_delta: validator_delta,
            added_validators_inclusion_proof: added_validators_proof,
            changed_validators_inclusion_proof: changed_validators_proof,
            balances: bs.balances.clone(),
        },
        old_lido_validator_state: old_validator_state.clone(),
        new_lido_validator_state_hash: new_validator_state.tree_hash_root(),
    };

    if verify {
        verify_input_correctness(
            bs.slot,
            &program_input,
            &old_validator_state,
            &new_validator_state,
            lido_withdrawal_credentials,
        )
        .expect("Failed to verify input correctness");
    }

    (program_input, public_values)
}

fn verify_input_correctness(
    slot: Slot,
    program_input: &ProgramInput,
    old_state: &LidoValidatorState,
    new_state: &LidoValidatorState,
    lido_withdrawal_credentials: &Hash256,
) -> Result<()> {
    log::debug!("Verifying inputs");
    let cycle_tracker = LogCycleTracker {};
    let input_verifier = InputVerifier::new(&cycle_tracker);
    input_verifier.prove_input(program_input);
    log::debug!("Inputs verified");

    log::debug!("Verifying old_state + validator_delta = new_state");
    let delta = &program_input.validators_and_balances.validators_delta;
    let computed_new_state = old_state.merge_validator_delta(slot, delta, lido_withdrawal_credentials);
    assert_eq!(computed_new_state, *new_state);
    assert_eq!(
        computed_new_state.tree_hash_root(),
        program_input.new_lido_validator_state_hash
    );
    log::debug!("New state verified");
    Ok(())
}

pub fn verify_public_values(public_values: &SP1PublicValues, expected_public_values: &PublicValuesRust) -> Result<()> {
    let public_values_solidity: PublicValuesSolidity =
        PublicValuesSolidity::abi_decode(public_values.as_slice(), true).expect("Failed to parse public values");
    let public_values_rust: PublicValuesRust = public_values_solidity.into();

    assert!(public_values_rust == *expected_public_values);
    log::debug!(
        "Expected hash: {}",
        hex::encode(public_values_rust.metadata.beacon_block_hash)
    );
    log::debug!(
        "Computed hash: {}",
        hex::encode(public_values_rust.metadata.beacon_block_hash)
    );

    log::info!("Public values match!");

    Ok(())
}
