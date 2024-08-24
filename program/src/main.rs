//! A simple program that takes a number `n` as input, and writes the `n-1`th and `n`th fibonacci
//! number as an output.

// These two lines are necessary for the program to properly compile.
//
// Under the hood, we wrap your main function with some extra code so that it behaves properly
// inside the zkVM.
#![no_main]
sp1_zkvm::entrypoint!(main);

use alloy_sol_types::SolType;
use hex;
use sp1_derive;
use sp1_lido_accounting_zk_shared::eth_consensus_layer::{Hash256, Validator};
use sp1_lido_accounting_zk_shared::eth_spec::Unsigned;
use sp1_lido_accounting_zk_shared::lido::LidoValidatorState;
use sp1_lido_accounting_zk_shared::report::ReportData;
use sp1_lido_accounting_zk_shared::util::{u64_to_usize, usize_to_u64};
use sp1_lido_accounting_zk_shared::{consts, eth_spec, hashing};
use tree_hash::TreeHash;

use sp1_lido_accounting_zk_shared::hashing::{HashHelper, HashHelperImpl};
use sp1_lido_accounting_zk_shared::io::eth_io::{
    LidoValidatorStateSolidity, PublicValuesSolidity, ReportMetadataSolidity, ReportSolidity,
};
use sp1_lido_accounting_zk_shared::io::program_io::{ProgramInput, ValsAndBals};
use sp1_lido_accounting_zk_shared::verification::{
    build_root_from_proof, serde as verification_serde, verify_hashes, FieldProof, MerkleTreeFieldLeaves, RsMerkleHash,
};

#[sp1_derive::cycle_tracker]
fn read_input() -> ProgramInput {
    sp1_zkvm::io::read::<ProgramInput>()
}

#[sp1_derive::cycle_tracker]
fn h256_to_alloy_type(value: Hash256) -> alloy_primitives::FixedBytes<32> {
    let bytes: [u8; 32] = value.into();
    bytes.into()
}

#[sp1_derive::cycle_tracker]
fn commit_public_values(
    report: &ReportData,
    beacon_block_hash: &[u8; 32],
    old_state: LidoValidatorStateSolidity,
    new_state: LidoValidatorStateSolidity,
) {
    let public_values_solidity: PublicValuesSolidity = PublicValuesSolidity {
        report: ReportSolidity {
            slot: report.slot,
            deposited_lido_validators: report.deposited_lido_validators,
            exited_lido_validators: report.exited_lido_validators,
            lido_cl_valance: report.lido_cl_balance,
        },
        metadata: ReportMetadataSolidity {
            slot: report.slot,
            epoch: report.epoch,
            lido_withdrawal_credentials: h256_to_alloy_type(report.lido_withdrawal_credentials),
            beacon_block_hash: beacon_block_hash.into(),
            state_for_previous_report: old_state,
            new_state: new_state,
        },
    };

    let bytes = PublicValuesSolidity::abi_encode(&public_values_solidity);

    // Commit to the public values of the program.
    sp1_zkvm::io::commit_slice(&bytes);
}

#[sp1_derive::cycle_tracker]
fn prove_validators_delta(
    validators_hash: &Hash256,
    validators_delta: &Vec<Validator>,
    validators_delta_proof: &[u8],
    old_state: &LidoValidatorState,
) {
    let new_validators = validators_delta;

    println!("cycle-tracker-start: prove_data_correctness.vals_and_bals.validators_delta.validator_roots");
    let validator_hashes: Vec<RsMerkleHash> = new_validators
        .iter()
        .map(|validator| validator.tree_hash_root().to_fixed_bytes())
        .collect();
    println!("cycle-tracker-end: prove_data_correctness.vals_and_bals.validators_delta.validator_roots");

    let validator_indexes = Vec::from_iter(old_state.indices_for_adjacent_delta(new_validators.len()));
    let validators_count: u64 = old_state.max_validator_index + usize_to_u64(new_validators.len()) + 1;
    let tree_depth = hashing::target_tree_depth::<Validator, eth_spec::ValidatorRegistryLimit>();

    println!("cycle-tracker-start: prove_data_correctness.vals_and_bals.validators_delta.deserialize_proof");
    let proof = verification_serde::deserialize_proof(validators_delta_proof)
        .expect("Failed to deserialize validators delta proof");
    println!("cycle-tracker-end: prove_data_correctness.vals_and_bals.validators_delta.deserialize_proof");

    println!("cycle-tracker-start: prove_data_correctness.vals_and_bals.validators_delta.reconstruct_root_from_proof");
    let validators_delta_root = build_root_from_proof(
        &proof,
        validators_count.next_power_of_two(),
        validator_indexes.as_slice(),
        validator_hashes.as_slice(),
        Some(tree_depth),
        Some(validators_count),
    )
    .expect("Failed to construct validators merkle root from delta multiproof");
    println!("cycle-tracker-end: prove_data_correctness.vals_and_bals.validators_delta.reconstruct_root_from_proof");

    verify_hashes(validators_hash, &validators_delta_root).expect("Failed to verify validators delta multiproof");
}

/**
 * Proves that the data passed into program is well-formed and correct
 *
 * Going top-down:
 * * Beacon Block root == merkle_tree_root(BeaconBlockHeader)
 * * merkle_tree_root(BeaconState) is included into BeaconBlockHeader
 * * Validators and balances are included into BeaconState (merkle multiproof)
 */
#[sp1_derive::cycle_tracker]
fn prove_data_correctness(input: &ProgramInput) {
    let beacon_block_header = &input.beacon_block_header;
    let beacon_state = &input.beacon_state;

    // Beacon Block root == merkle_tree_root(BeaconBlockHeader)
    println!("cycle-tracker-start: prove_data_correctness.beacon_block_header.root");
    let bh_root = beacon_block_header.tree_hash_root();
    assert!(
        bh_root == input.beacon_block_hash.into(),
        "Failed to verify Beacon Block Header hash, got {}, expected {}",
        hex::encode(bh_root),
        hex::encode(input.beacon_block_hash),
    );
    println!("cycle-tracker-end: prove_data_correctness.beacon_block_header.root");

    // merkle_tree_root(BeaconState) is included into BeaconBlockHeader
    println!("cycle-tracker-start: prove_data_correctness.beacon_state.root");
    let bs_root = beacon_state.tree_hash_root();
    assert!(
        bs_root == beacon_block_header.state_root,
        "Beacon State hash mismatch, got {}, expected {}",
        hex::encode(bs_root),
        hex::encode(beacon_block_header.state_root),
    );
    println!("cycle-tracker-end: prove_data_correctness.beacon_state.root");

    // Validators and balances are included into BeaconState (merkle multiproof)
    println!("cycle-tracker-start: prove_data_correctness.vals_and_bals");
    // Step 1: confirm validators delta
    println!("cycle-tracker-start: prove_data_correctness.vals_and_bals.validators");
    prove_validators_delta(
        &beacon_state.validators,
        &input.validators_and_balances.validators_delta,
        &input.validators_and_balances.validators_delta_proof,
        &input.old_lido_validator_state,
    );
    println!("cycle-tracker-end: prove_data_correctness.vals_and_bals.validators");

    // Step 2: confirm passed balances hashes match the ones in BeaconState
    println!("cycle-tracker-start: prove_data_correctness.vals_and_bals.balances_root");
    let balances_hash = HashHelperImpl::hash_list(&input.validators_and_balances.balances);
    assert!(
        balances_hash == beacon_state.balances,
        "Balances hash mismatch, got {}, expected {}",
        hex::encode(balances_hash),
        hex::encode(beacon_state.balances),
    );
    println!("cycle-tracker-end: prove_data_correctness.vals_and_bals.balances_root");

    // Step 2: confirm validators and balances hashes are included into beacon_state
    println!("cycle-tracker-start: prove_data_correctness.vals_and_bals.multiproof");
    let bs_indices = beacon_state
        .get_leafs_indices(["validators", "balances"])
        .expect("Failed to get leaf indices");

    let vals_and_bals_multiproof_leaves = [
        beacon_state.validators.to_fixed_bytes(),
        beacon_state.balances.to_fixed_bytes(),
    ];
    beacon_state
        .verify_serialized(
            &input.validators_and_balances.validators_and_balances_proof,
            &bs_indices,
            &vals_and_bals_multiproof_leaves,
        )
        .expect("Failed to verify validators and balances inclusion");
    println!("cycle-tracker-end: prove_data_correctness.vals_and_bals.multiproof");
    println!("cycle-tracker-end: prove_data_correctness.vals_and_bals");
}

#[sp1_derive::cycle_tracker]
fn compute_report(input: &ProgramInput) -> ReportData {
    let epoch = input.slot.checked_div(eth_spec::SlotsPerEpoch::to_u64()).unwrap();

    let new_report = input.old_lido_validator_state.clone();

    ReportData::compute(
        input.slot,
        epoch,
        &input.validators_and_balances.validators,
        &input.validators_and_balances.balances,
        &consts::LIDO_WITHDRAWAL_CREDENTIALS.into(),
    )
}

#[sp1_derive::cycle_tracker]
fn verify_inputs(input: &ProgramInput) {
    prove_data_correctness(input);
}

pub fn main() {
    println!("cycle-tracker-start: main.read_args");
    let input: ProgramInput = read_input();
    println!("cycle-tracker-end: main.read_args");

    println!("cycle-tracker-start: main.verify_inputs");
    verify_inputs(&input);
    println!("cycle-tracker-start: main.verify_inputs");

    println!("cycle-tracker-start: main.compute_report");
    let report = compute_report(&input);
    println!("cycle-tracker-end: main.compute_report");

    println!("cycle-tracker-start: main.commit_public_values");
    commit_public_values(&report, &input.beacon_block_hash);
    println!("cycle-tracker-end: main.commit_public_values");
}
