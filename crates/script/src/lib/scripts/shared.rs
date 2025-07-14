use crate::validator_delta::{self, ValidatorDeltaCompute, ValidatorDeltaComputeBeaconStateProjection};
use alloy_sol_types::SolType;

use sp1_sdk::SP1PublicValues;

use sp1_lido_accounting_zk_shared::circuit_logic::input_verification::{self, InputVerifier, LogCycleTracker};
use sp1_lido_accounting_zk_shared::circuit_logic::report::ReportData;
use sp1_lido_accounting_zk_shared::eth_consensus_layer::{BeaconBlockHeader, BeaconState, BeaconStateFields, Hash256};
use sp1_lido_accounting_zk_shared::io::eth_io::{
    BeaconChainSlot, HaveEpoch, HaveSlotWithBlock, LidoValidatorStateRust, PublicValuesRust, PublicValuesSolidity,
    ReferenceSlot, ReportMetadataRust, ReportRust,
};
use sp1_lido_accounting_zk_shared::io::program_io::{
    ExecutionPayloadHeaderData, ProgramInput, ValsAndBals, WithdrawalVaultData,
};
use sp1_lido_accounting_zk_shared::lido::{self, LidoValidatorState};
use sp1_lido_accounting_zk_shared::merkle_proof::FieldProof;
use sp1_lido_accounting_zk_shared::util::{u64_to_usize, usize_to_u64, IntegerError};

use anyhow::Result;

use tree_hash::TreeHash;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("Failed to compute validators delta: {0:?}")]
    ValidatorDeltaComputeError(#[from] validator_delta::Error),

    #[error("Validator states mismatch: {actual:?} != {expected:?}")]
    ValidatorStateMismatch {
        expected: Box<LidoValidatorState>,
        actual: Box<LidoValidatorState>,
    },

    #[error("Failed to merge validator delta: {0:?}")]
    MergeValidatorDeltaFailed(#[from] lido::Error),

    #[error("State hash in porgram input {program_input} != computed state hash {computed}")]
    ValidatorStateHashMismatch { computed: Hash256, program_input: Hash256 },

    #[error(transparent)]
    FailedToProveInput(#[from] input_verification::Error),

    #[error(transparent)]
    FailedToComputeReport(#[from] IntegerError),
}

pub fn prepare_program_input(
    reference_slot: ReferenceSlot,
    bs: &BeaconState,
    bh: &BeaconBlockHeader,
    old_bs: &BeaconState,
    lido_withdrawal_credentials: &Hash256,
    lido_withdrawal_vault_data: WithdrawalVaultData,
    verify: bool,
) -> Result<(ProgramInput, PublicValuesRust), Error> {
    tracing::info!("Preparing program input");
    let beacon_block_hash = bh.tree_hash_root();

    tracing::info!(
        "Processing BeaconState. Current slot: {}, Previous Slot: {}, Block Hash: {}, Validator count:{}",
        bs.slot,
        old_bs.slot,
        hex::encode(beacon_block_hash),
        bs.validators.len()
    );
    let old_validator_state = LidoValidatorState::compute_from_beacon_state(old_bs, lido_withdrawal_credentials);
    let new_validator_state = LidoValidatorState::compute_from_beacon_state(bs, lido_withdrawal_credentials);

    tracing::info!(
        old_deposited = old_validator_state.deposited_lido_validator_indices.len(),
        old_pending = old_validator_state.pending_deposit_lido_validator_indices.len(),
        old_exited = old_validator_state.exited_lido_validator_indices.len(),
        new_deposited = new_validator_state.deposited_lido_validator_indices.len(),
        new_pending = new_validator_state.pending_deposit_lido_validator_indices.len(),
        new_exited = new_validator_state.exited_lido_validator_indices.len(),
        "Computed validator states. Old: deposited {}, pending {}, exited {}. New: deposited {}, pending {}, exited {}",
        old_validator_state.deposited_lido_validator_indices.len(),
        old_validator_state.pending_deposit_lido_validator_indices.len(),
        old_validator_state.exited_lido_validator_indices.len(),
        new_validator_state.deposited_lido_validator_indices.len(),
        new_validator_state.pending_deposit_lido_validator_indices.len(),
        new_validator_state.exited_lido_validator_indices.len(),
    );

    tracing::info!(validators_count = bs.validators.len(), "Computing report");
    let report = ReportData::compute(
        reference_slot,
        bs.epoch(),
        &bs.validators,
        &bs.balances,
        lido_withdrawal_credentials,
    )?;

    tracing::info!("Forming public values");
    let public_values: PublicValuesRust = PublicValuesRust {
        report: ReportRust {
            reference_slot: report.slot,
            deposited_lido_validators: report.deposited_lido_validators,
            exited_lido_validators: report.exited_lido_validators,
            lido_cl_balance: report.lido_cl_balance,
            lido_withdrawal_vault_balance: lido_withdrawal_vault_data.balance,
        },
        metadata: ReportMetadataRust {
            bc_slot: bs.bc_slot(),
            epoch: report.epoch,
            lido_withdrawal_credentials: lido_withdrawal_credentials.0,
            beacon_block_hash: beacon_block_hash.0,
            state_for_previous_report: LidoValidatorStateRust {
                slot: old_validator_state.bc_slot(),
                merkle_root: old_validator_state.tree_hash_root().0,
            },
            new_state: LidoValidatorStateRust {
                slot: new_validator_state.slot,
                merkle_root: new_validator_state.tree_hash_root().0,
            },
            withdrawal_vault_data: lido_withdrawal_vault_data.clone().into(),
        },
    };

    tracing::info!("Computed report and public values");
    tracing::debug!("Report {report:?}");
    tracing::debug!("Public values {public_values:?}");

    tracing::info!("Computing validators and balances struct for program input");
    let validators_and_balances =
        compute_validators_and_balances(bs, old_bs, &old_validator_state, lido_withdrawal_credentials, verify)?;

    tracing::info!("Obtaining execution header data");
    let execution_header_data: ExecutionPayloadHeaderData = (&bs.latest_execution_payload_header).into();
    tracing::debug!("Obtained BeaconState.latest_execution_header.state_root proof");

    tracing::info!("Creating program input");
    let program_input = ProgramInput {
        reference_slot,
        bc_slot: bs.bc_slot(),
        beacon_block_hash,
        beacon_block_header: bh.into(),
        latest_execution_header_data: execution_header_data,
        beacon_state: bs.into(),
        validators_and_balances,
        old_lido_validator_state: old_validator_state.clone(),
        new_lido_validator_state_hash: new_validator_state.tree_hash_root(),

        withdrawal_vault_data: lido_withdrawal_vault_data,
    };

    if verify {
        verify_input_correctness(
            bs.bc_slot(),
            &program_input,
            &old_validator_state,
            &new_validator_state,
            lido_withdrawal_credentials,
        )?;
    }

    tracing::info!("Program input ready");
    Ok((program_input, public_values))
}

// TODO: could be private, but used in input tampering tests
pub fn compute_validators_and_balances_test_public(
    bs: &BeaconState,
    old_bs: &BeaconState,
    old_validator_state: &LidoValidatorState,
    lido_withdrawal_credentials: &Hash256,
    verify: bool,
) -> Result<ValsAndBals, Error> {
    compute_validators_and_balances(bs, old_bs, old_validator_state, lido_withdrawal_credentials, verify)
}

fn compute_validators_and_balances(
    bs: &BeaconState,
    old_bs: &BeaconState,
    old_validator_state: &LidoValidatorState,
    lido_withdrawal_credentials: &Hash256,
    verify: bool,
) -> Result<ValsAndBals, Error> {
    let validator_delta = ValidatorDeltaCompute::new(
        ValidatorDeltaComputeBeaconStateProjection::from_bs(old_bs),
        old_validator_state,
        ValidatorDeltaComputeBeaconStateProjection::from_bs(bs),
        !verify,
    )
    .compute()?;
    tracing::info!(
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
    tracing::info!("Obtained validators multiproofs for added and changed validators");

    let validators_and_balances_proof =
        bs.get_serialized_multiproof(&[BeaconStateFields::validators, BeaconStateFields::balances]);
    tracing::info!("Obtained multiproofs for BeaconState.validators and BeaconState.balances fields");

    Ok(ValsAndBals {
        validators_and_balances_proof,
        lido_withdrawal_credentials: *lido_withdrawal_credentials,
        total_validators: usize_to_u64(bs.validators.len()),
        validators_delta: validator_delta,
        added_validators_inclusion_proof: added_validators_proof,
        changed_validators_inclusion_proof: changed_validators_proof,
        balances: bs.balances.clone(),
    })
}

fn verify_input_correctness(
    slot: BeaconChainSlot,
    program_input: &ProgramInput,
    old_state: &LidoValidatorState,
    new_state: &LidoValidatorState,
    lido_withdrawal_credentials: &Hash256,
) -> Result<(), Error> {
    tracing::info!("Verifying program input correctness");
    tracing::debug!("Verifying inputs");
    let cycle_tracker = LogCycleTracker {};
    let input_verifier = InputVerifier::new(&cycle_tracker);
    input_verifier.prove_input(program_input)?;
    tracing::debug!("Inputs verified");

    tracing::debug!("Verifying old_state + validator_delta = new_state");
    let delta = &program_input.validators_and_balances.validators_delta;
    let computed_new_state = old_state.merge_validator_delta(slot, delta, lido_withdrawal_credentials)?;
    if computed_new_state != *new_state {
        tracing::error!("Validator states mismatch");
        return Err(Error::ValidatorStateMismatch {
            expected: Box::new(computed_new_state.clone()),
            actual: Box::new(new_state.clone()),
        });
    }
    let computed_hash = computed_new_state.tree_hash_root();
    let program_input_hash = program_input.new_lido_validator_state_hash;
    if computed_new_state.tree_hash_root() != program_input.new_lido_validator_state_hash {
        tracing::error!("New validator state hash mismatch: computed {computed_hash} != passed {program_input_hash}",);
        return Err(Error::ValidatorStateHashMismatch {
            computed: computed_hash,
            program_input: program_input_hash,
        });
    }
    tracing::debug!("New state verified");
    Ok(())
}

pub fn verify_public_values(public_values: &SP1PublicValues, expected_public_values: &PublicValuesRust) -> Result<()> {
    let public_values_solidity: PublicValuesSolidity =
        PublicValuesSolidity::abi_decode_validate(public_values.as_slice())?;
    let public_values_rust: PublicValuesRust = public_values_solidity.try_into()?;

    tracing::debug!(
        "Expected hash: {}",
        hex::encode(expected_public_values.metadata.beacon_block_hash)
    );
    tracing::debug!(
        "Computed hash: {}",
        hex::encode(public_values_rust.metadata.beacon_block_hash)
    );
    if public_values_rust != *expected_public_values {
        let msg = format!("Public values mismatch: expected {expected_public_values:?}, got {public_values_rust:?}");
        tracing::error!(msg);
        return Err(anyhow::anyhow!(msg));
    }
    tracing::info!("Public values match!");

    Ok(())
}
