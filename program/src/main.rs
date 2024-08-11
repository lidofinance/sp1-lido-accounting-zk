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
use sp1_lido_accounting_zk_shared::eth_consensus_layer::Hash256;
use sp1_lido_accounting_zk_shared::eth_spec::Unsigned;
use sp1_lido_accounting_zk_shared::report::ReportData;
use sp1_lido_accounting_zk_shared::{consts, eth_spec};
use tree_hash::TreeHash;

#[cfg(target_arch = "riscv32")]
use ethereum_hashing::{hash32_concat, ZERO_HASHES};
#[cfg(target_arch = "riscv32")]
use tree_hash::MerkleHasher;

use sp1_lido_accounting_zk_shared::program_io::{
    ProgramInput, PublicValuesRust, PublicValuesSolidity, ReportSolidity, ValsAndBals,
};
use sp1_lido_accounting_zk_shared::verification::{FieldProof, MerkleTreeFieldLeaves};

trait ValidatorsAndBalancesHash {
    fn balances_hash(&self) -> Hash256;
    fn validators_hash(&self) -> Hash256;
}

#[cfg(not(target_arch = "riscv32"))]
impl ValidatorsAndBalancesHash for ValsAndBals {
    fn balances_hash(&self) -> Hash256 {
        self.balances.tree_hash_root()
    }
    fn validators_hash(&self) -> Hash256 {
        self.validators.tree_hash_root()
    }
}

#[cfg(target_arch = "riscv32")]
struct HashHelper {}
#[cfg(target_arch = "riscv32")]
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

#[cfg(target_arch = "riscv32")]
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
                .expect("Failed to write item into hasher");
        }

        let actual_elements_root = hasher.finish().expect("Failed to finalize the hasher");
        let expanded_tree_root = HashHelper::pad_to_depth(&actual_elements_root, main_tree_depth, target_tree_depth);

        tree_hash::mix_in_length(&expanded_tree_root, self.balances.len())
    }

    fn validators_hash(&self) -> Hash256 {
        assert!((self.validators.len() as u64) < (u32::MAX as u64));

        let main_tree_depth: usize = HashHelper::MAX_DEPTH;
        let main_tree_elems: usize = (2_usize).pow(main_tree_depth as u32);

        // trailing zeroes is essentially a log2
        let target_tree_depth = 40;

        let mut hasher = MerkleHasher::with_leaves(main_tree_elems);

        // for item in &self.balances {
        for item in &self.validators {
            hasher
                .write(item.tree_hash_root().as_fixed_bytes())
                .expect("Failed to write item into hasher");
        }

        let actual_elements_root = hasher.finish().expect("Failed to finalize the hasher");
        let expanded_tree_root = HashHelper::pad_to_depth(&actual_elements_root, main_tree_depth, target_tree_depth);

        tree_hash::mix_in_length(&expanded_tree_root, self.validators.len())
    }
}

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
fn commit_public_values(public_values: PublicValuesRust) {
    let report_solidity = ReportSolidity {
        slot: public_values.report.slot,
        epoch: public_values.report.epoch,
        lido_withdrawal_credentials: h256_to_alloy_type(public_values.report.lido_withdrawal_credentials),
        all_lido_validators: public_values.report.all_lido_validators,
        exited_lido_validators: public_values.report.exited_lido_validators,
        lido_cl_valance: public_values.report.lido_cl_valance,
    };
    // public_values.report
    let public_values_solidity: PublicValuesSolidity = PublicValuesSolidity {
        slot: public_values.slot,
        beacon_block_hash: public_values.beacon_block_hash.into(),
        report: report_solidity,
    };
    let bytes = PublicValuesSolidity::abi_encode(&public_values_solidity);

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
    println!("cycle-tracker-start: prove_data_correctness.vals_and_bals.validators_root");
    let validators_hash = input.validators_and_balances.validators_hash();
    assert!(
        validators_hash == beacon_state.validators,
        "Validators hash mismatch, got {}, expected {}",
        hex::encode(validators_hash),
        hex::encode(beacon_state.validators),
    );
    println!("cycle-tracker-end: prove_data_correctness.vals_and_bals.validators_root");

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

#[sp1_derive::cycle_tracker]
fn compute_report(input: &ProgramInput) -> ReportData {
    let epoch = input.slot.checked_div(eth_spec::SlotsPerEpoch::to_u64()).unwrap();

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
    let public_values = PublicValuesRust {
        slot: input.slot,
        beacon_block_hash: input.beacon_block_hash,
        report,
    };
    commit_public_values(public_values);
    println!("cycle-tracker-end: main.commit_public_values");
}
