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
use tree_hash::TreeHash;

use sp1_lido_accounting_zk_shared::program_io::{ProgramInput, PublicValuesRust, PublicValuesSolidity};

use sp1_lido_accounting_zk_shared::verification::{FieldProof, MerkleTreeFieldLeaves};

#[sp1_derive::cycle_tracker]
fn read_input() -> ProgramInput {
    sp1_zkvm::io::read::<ProgramInput>()
}

#[sp1_derive::cycle_tracker]
fn commit_public_values(public_values: PublicValuesRust) {
    let bytes = PublicValuesSolidity::abi_encode(&(public_values.slot, public_values.beacon_block_hash));

    // Commit to the public values of the program.
    sp1_zkvm::io::commit_slice(&bytes);
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
        "Failed to verify Beacon Block Header hash, expected {}, got {}",
        hex::encode(input.beacon_block_hash),
        hex::encode(bh_root)
    );
    println!("cycle-tracker-end: prove_data_correctness.beacon_block_header.root");

    // merkle_tree_root(BeaconState) is included into BeaconBlockHeader
    println!("cycle-tracker-start: prove_data_correctness.beacon_state.root");
    let bs_root = beacon_state.tree_hash_root();
    assert!(
        bs_root == beacon_block_header.state_root.into(),
        "Beacon State hash mismatch, expected {}, got {}",
        hex::encode(input.beacon_block_header.state_root),
        hex::encode(bs_root)
    );
    println!("cycle-tracker-end: prove_data_correctness.beacon_state.root");

    // Validators and balances are included into BeaconState (merkle multiproof)
    println!("cycle-tracker-start: prove_data_correctness.vals_and_bals.multiproof");
    let bs_indices = beacon_state
        .get_leafs_indices(["validators", "balances"])
        .expect("Failed to get leaf indices");

    beacon_state
        .verify_serialized(&input.validators_and_balances_proof, &bs_indices)
        .expect("Failed to verify validators and balances inclusion");
    println!("cycle-tracker-end: prove_data_correctness.vals_and_bals.multiproof");
}

#[sp1_derive::cycle_tracker]
fn verify_inputs(input: &ProgramInput) {
    prove_data_correctness(input);
}

pub fn main() {
    let input: ProgramInput = read_input();

    verify_inputs(&input);

    let public_values = PublicValuesRust {
        slot: input.slot,
        beacon_block_hash: input.beacon_block_hash,
    };

    commit_public_values(public_values);
}
