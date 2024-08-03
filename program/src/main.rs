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

#[sp1_derive::cycle_tracker]
fn verify_inputs(input: &ProgramInput) {
    let beacon_state = &input.beacon_state;

    let indices = beacon_state
        .get_leafs_indices(["validators", "balances"])
        .expect("Failed to get leaf indices");

    beacon_state
        .verify_serialized(&input.validators_and_balances_proof, &indices)
        .expect("Failed to verify validators and balances inclusion");

    // TODO: this should ladder up to beacon block, but for now
    // we're passing beacon state hash as beacon block hash
    assert!(
        beacon_state.tree_hash_root() == input.beacon_block_hash.into(),
        "Failed to verify Beacon State hash, expected {}, got {}",
        hex::encode(input.beacon_block_hash),
        hex::encode(beacon_state.tree_hash_root())
    );
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
