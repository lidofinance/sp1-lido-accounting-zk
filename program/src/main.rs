//! A simple program that takes a number `n` as input, and writes the `n-1`th and `n`th fibonacci
//! number as an output.

// These two lines are necessary for the program to properly compile.
//
// Under the hood, we wrap your main function with some extra code so that it behaves properly
// inside the zkVM.
#![no_main]
sp1_zkvm::entrypoint!(main);

use alloy_sol_types::{sol, SolType};

/// The public values encoded as a tuple that can be easily deserialized inside Solidity.
type PublicValuesTuple = sol! {
    tuple(uint64, bytes32)
};

type HashElement = [u8; 32];

pub fn main() {
    let slot: u64 = sp1_zkvm::io::read::<u64>();
    let hash: HashElement = sp1_zkvm::io::read::<HashElement>();

    // Encocde the public values of the program.
    let bytes = PublicValuesTuple::abi_encode(&(slot, hash));

    // Commit to the public values of the program.
    sp1_zkvm::io::commit_slice(&bytes);
}
