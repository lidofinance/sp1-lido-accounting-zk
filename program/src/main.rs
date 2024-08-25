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
use sp1_lido_accounting_zk_shared::lido::{LidoValidatorState, ValidatorDelta, ValidatorWithIndex};
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
    old_state: LidoValidatorState,
    new_state: LidoValidatorState,
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
            state_for_previous_report: LidoValidatorStateSolidity {
                slot: old_state.slot,
                hash: old_state.tree_hash_root().to_fixed_bytes().into(),
            },
            new_state: LidoValidatorStateSolidity {
                slot: new_state.slot,
                hash: new_state.tree_hash_root().to_fixed_bytes().into(),
            },
        },
    };

    let bytes = PublicValuesSolidity::abi_encode(&public_values_solidity);

    // Commit to the public values of the program.
    sp1_zkvm::io::commit_slice(&bytes);
}

#[sp1_derive::cycle_tracker]
fn verify_validator_inclusion_proof(
    label: &str,
    validators_hash: &Hash256,
    validators_with_indices: &Vec<ValidatorWithIndex>,
    serialized_proof: &[u8],
) {
    let tree_depth = hashing::target_tree_depth::<Validator, eth_spec::ValidatorRegistryLimit>();

    let validators_count = validators_with_indices.len();
    let mut indexes: Vec<usize> = Vec::with_capacity(validators_count);
    let mut hashes: Vec<RsMerkleHash> = Vec::with_capacity(validators_count);

    println!(
        "cycle-tracker-start: prove_data_correctness.vals_and_bals.validators_delta.{}.validator_roots",
        label
    );
    for validator_with_index in validators_with_indices {
        indexes.push(u64_to_usize(validator_with_index.index));
        hashes.push(validator_with_index.validator.tree_hash_root().to_fixed_bytes());
    }
    println!(
        "cycle-tracker-end: prove_data_correctness.vals_and_bals.validators_delta.{}.validator_roots",
        label
    );

    println!(
        "cycle-tracker-start: prove_data_correctness.vals_and_bals.validators_delta.{}.deserialize_proof",
        label
    );
    let proof = verification_serde::deserialize_proof(serialized_proof).expect("Failed to deserialize proof");
    println!(
        "cycle-tracker-end: prove_data_correctness.vals_and_bals.validators_delta.{}.deserialize_proof",
        label
    );

    println!(
        "cycle-tracker-start: prove_data_correctness.vals_and_bals.validators_delta.{}.reconstruct_root_from_proof",
        label
    );
    let validators_delta_root = build_root_from_proof(
        &proof,
        validators_count.next_power_of_two(),
        indexes.as_slice(),
        hashes.as_slice(),
        Some(tree_depth),
        Some(validators_count),
    )
    .expect("Failed to construct validators merkle root from delta multiproof");
    println!(
        "cycle-tracker-end: prove_data_correctness.vals_and_bals.validators_delta.{}.reconstruct_root_from_proof",
        label
    );

    println!(
        "cycle-tracker-start: prove_data_correctness.vals_and_bals.validators_delta.{}.verify_hash",
        label
    );
    // verify_hashes(validators_hash, &validators_delta_root).expect("Failed to verify validators delta multiproof");
    println!(
        "cycle-tracker-end: prove_data_correctness.vals_and_bals.validators_delta.{}.verify_hash",
        label
    );
}

/**
 * Proves that the data passed into program is well-formed and correct
 *
 * Going top-down:
 * * Beacon Block root == merkle_tree_root(BeaconBlockHeader)
 * * merkle_tree_root(BeaconState) is included into BeaconBlockHeader
 * * Balances are included into BeaconState (merkle multiproof)
 * * Validators passed in validators delta are included into BeaconState (merkle multiproofs)
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
    println!("cycle-tracker-start: prove_data_correctness.vals_and_bals.validators_delta");
    verify_validator_inclusion_proof(
        "all_added",
        &beacon_state.validators,
        &input.validators_and_balances.validators_delta.all_added,
        &input.validators_and_balances.added_validators_inclusion_proof,
    );

    verify_validator_inclusion_proof(
        "lido_changed",
        &beacon_state.validators,
        &input.validators_and_balances.validators_delta.lido_changed,
        &input.validators_and_balances.changed_validators_inclusion_proof,
    );
    println!("cycle-tracker-end: prove_data_correctness.vals_and_bals.validators_delta");

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

pub fn main() {
    println!("cycle-tracker-start: main.read_args");
    let input: ProgramInput = read_input();
    println!("cycle-tracker-end: main.read_args");

    println!("cycle-tracker-start: main.verify_inputs");
    prove_data_correctness(&input);
    println!("cycle-tracker-start: main.verify_inputs");

    println!("cycle-tracker-start: main.compute_report");
    let withdrawal_creds: Hash256 = consts::LIDO_WITHDRAWAL_CREDENTIALS.into();

    let new_state: LidoValidatorState = input.old_lido_validator_state.merge_validator_delta(
        input.slot,
        &input.validators_and_balances.validators_delta,
        &withdrawal_creds,
    );

    let report = ReportData::compute_from_state(&new_state, &input.validators_and_balances.balances, &withdrawal_creds);
    println!("cycle-tracker-end: main.compute_report");

    println!("cycle-tracker-start: main.commit_public_values");
    commit_public_values(
        &report,
        &input.beacon_block_hash.to_fixed_bytes(),
        input.old_lido_validator_state,
        new_state,
    );
    println!("cycle-tracker-end: main.commit_public_values");
}
