//! A simple program that takes a number `n` as input, and writes the `n-1`th and `n`th fibonacci
//! number as an output.

// These two lines are necessary for the program to properly compile.
//
// Under the hood, we wrap your main function with some extra code so that it behaves properly
// inside the zkVM.
#![no_main]
sp1_zkvm::entrypoint!(main);
use std::thread::current;

use alloy_sol_types::SolType;
use ethereum_hashing::{hash32_concat, ZERO_HASHES};
use hex;
use sp1_derive;
use sp1_lido_accounting_zk_shared::eth_consensus_layer::{Balances, Hash256, Validators};
use tree_hash::{MerkleHasher, TreeHash};

use sp1_lido_accounting_zk_shared::program_io::{ProgramInput, PublicValuesRust, PublicValuesSolidity, ValsAndBals};

use sp1_lido_accounting_zk_shared::verification::{FieldProof, MerkleTreeFieldLeaves};

trait ValidatorsAndBalancesHash {
    fn balances_hash(&self) -> Hash256;
    // fn validators_hash(&self) -> Hash256;
}

// #[cfg(not(target_arch = "riscv32"))]
// impl ValidatorsAndBalancesHash for ValsAndBals {
//     fn balances_hash(&self) -> Hash256 {
//         self.balances.tree_hash_root()
//     }
//     // fn validators_hash(&self) -> Hash256 {
//     //     self.validators.tree_hash_root()
//     // }
// }

// #[cfg(target_arch = "riscv32")]
struct HashHelper {}
impl HashHelper {
    const MAX_DEPTH: usize = 29;

    fn pad_to_depth(hash: &Hash256, current_depth: usize, target_depth: usize) -> Hash256 {
        let mut curhash: [u8; 32] = hash.to_fixed_bytes();
        for depth in current_depth..target_depth {
            curhash = hash32_concat(&curhash, ZERO_HASHES[depth].as_slice());
        }
        return curhash.into();
    }
}

// #[cfg(target_arch = "riscv32")]
impl ValidatorsAndBalancesHash for ValsAndBals {
    fn balances_hash(&self) -> Hash256 {
        assert!((self.balances.len() as u64) < (u32::MAX as u64));

        let main_tree_depth: usize = HashHelper::MAX_DEPTH;
        let main_tree_elems: usize = (2_usize).pow(main_tree_depth as u32);

        // trailing zeroes is essentially a log2
        let packing_factor = (u64::tree_hash_packing_factor()).trailing_zeros() as usize;
        let target_tree_depth = 40 - packing_factor;

        let mut hasher = MerkleHasher::with_leaves(main_tree_elems);

        // for item in &self.balances {
        for item in &self.balances {
            hasher
                .write(&item.tree_hash_packed_encoding())
                .expect("ssz_types variable vec should not contain more elements than max");
        }

        let actual_elements_root = hasher
            .finish()
            .expect("ssz_types variable vec should not have a remaining buffer");
        let expanded_tree_root = HashHelper::pad_to_depth(&actual_elements_root, main_tree_depth, target_tree_depth);

        tree_hash::mix_in_length(&expanded_tree_root, self.balances.len())
    }
    // fn validators_hash(&self) -> Hash256 {
    //     self.validators.tree_hash_root()
    // }
}

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
    // Step 1: confirm multiproof
    println!("cycle-tracker-start: prove_data_correctness.vals_and_bals.multiproof");
    let bs_indices = beacon_state
        .get_leafs_indices(["validators", "balances"])
        .expect("Failed to get leaf indices");

    beacon_state
        .verify_serialized(&input.validators_and_balances_proof, &bs_indices)
        .expect("Failed to verify validators and balances inclusion");
    println!("cycle-tracker-end: prove_data_correctness.vals_and_bals.multiproof");

    // Step 2: confirm passed validators and balances hashes match the ones in BeaconState
    // println!("cycle-tracker-start: prove_data_correctness.vals_and_bals.validators_root");
    // let validators_hash = input.validators_and_balances.validators.tree_hash_root();
    // assert!(
    //     validators_hash == beacon_state.validators,
    //     "Validators hash mismatch, got {}, expected {}",
    //     hex::encode(validators_hash),
    //     hex::encode(beacon_state.validators),
    // );
    // println!("cycle-tracker-end: prove_data_correctness.vals_and_bals.validators_root");

    println!("cycle-tracker-start: prove_data_correctness.vals_and_bals.balances_root");
    let balances_hash = input.validators_and_balances.balances_hash();
    assert!(
        balances_hash == beacon_state.balances,
        "Balances hash mismatch, got {}, expected {}",
        hex::encode(balances_hash),
        hex::encode(beacon_state.balances),
    );
    println!("cycle-tracker-end: prove_data_correctness.vals_and_bals.balances_root");
    println!("cycle-tracker-end: prove_data_correctness.vals_and_bals");
}

// #[cfg(target_arch = "riscv32")]
// fn balances_hash(balances: &Balances) -> Hash256 {
//     assert!((balances.len() as u64) < (u32::MAX as u64));

//     let mut hasher = MerkleHasher::with_leaves((u32::MAX / 8) as usize);
//     for item in &balances.to_vec() {
//         hasher
//             .write(&item.tree_hash_packed_encoding())
//             .expect("ssz_types variable vec should not contain more elements than max");
//     }

//     let root = hasher
//         .finish()
//         .expect("ssz_types variable vec should not have a remaining buffer");

//     tree_hash::mix_in_length(&root, balances.len())
// }

// #[cfg(not(target_arch = "riscv32"))]
// fn balances_hash(balances: &Balances) -> Hash256 {
//     balances.tree_hash_root()
// }

fn debug(input: &ProgramInput) {
    let bals = &input.validators_and_balances.balances;
    println!("Balances size {}", bals.len());

    // sp1 uses riscv32 that has usize = 32, and ssz_types + tree_hash extensively
    // use usize. However, ValidatorRegistryLimit is 2**40 - which causes overflow and
    // everything crashes and burns at multiple places. For example:
    // * eth_spec::ValidatorRegistryLimit::to_usize() => 0 (because overflow)
    // ** ... and then ssz_types/src/tree_hash.rs vec_tree_hash_root<T, N> => MerkleHasher::with_leaves(0) -> tree of depth 1
    // * tree_hash/src/merkle_hasher.rs MerkleHasher::process_leaf() - for trees higher than 32 max_elements overflows
    // ... probably more
    assert!((bals.len() as u64) < (u32::MAX as u64));

    let mut hasher = MerkleHasher::with_leaves((u32::MAX / 8) as usize);

    // println!(
    //     "ValidatorRegistryLimit: {}",
    //     eth_spec::ValidatorRegistryLimit::to_usize()
    // );
    // // println!("Type: {}", std::any::type_name::<eth_spec::ValidatorRegistryLimit>());
    // println!("Test  : {}", eth_spec::Test::to_usize());
    // println!("Test2 : {}", eth_spec::Test2::to_usize());
    // println!("Test3 : {}", eth_spec::Test3::to_usize());
    // println!("Test4 : {}", eth_spec::Test4::to_usize());
    // println!("Test5 : {}", eth_spec::Test5::to_usize());
    // println!("Test6 : {}", eth_spec::Test6::to_usize());
    // println!("Max usize: {}", usize::MAX);

    // println!(
    //     "Size: {}, Packing: {}, leaves: {}",
    //     eth_spec::ValidatorRegistryLimit::to_usize(),
    //     u64::tree_hash_packing_factor(),
    //     leaves
    // );

    for item in &input.validators_and_balances.balances.to_vec() {
        hasher
            .write(&item.tree_hash_packed_encoding())
            .expect("ssz_types variable vec should not contain more elements than max");
    }

    let hash = hasher
        .finish()
        .expect("ssz_types variable vec should not have a remaining buffer");
    // let hash = bals.tree_hash_root();

    println!("Balances hash: {}", hex::encode(hash));
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

    println!("cycle-tracker-start: main.commit_public_values");
    let public_values = PublicValuesRust {
        slot: input.slot,
        beacon_block_hash: input.beacon_block_hash,
    };

    commit_public_values(public_values);
    println!("cycle-tracker-end: main.commit_public_values");
}
