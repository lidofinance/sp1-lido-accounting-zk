//! A simple program that takes a number `n` as input, and writes the `n-1`th and `n`th fibonacci
//! number as an output.

// These two lines are necessary for the program to properly compile.
//
// Under the hood, we wrap your main function with some extra code so that it behaves properly
// inside the zkVM.
#![no_main]
sp1_zkvm::entrypoint!(main);
use sp1_derive;

use alloy_sol_types::SolType;

use sp1_lido_accounting_zk_shared::program_io::{ProgramInput, PublicValuesRust, PublicValuesSolidity};

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

pub fn main() {
    let input: ProgramInput = read_input();

    let public_values = PublicValuesRust {
        slot: input.slot,
        beacon_block_hash: input.beacon_block_hash,
    };

    commit_public_values(public_values);
}
