//! A simple program that takes a number `n` as input, and writes the `n-1`th and `n`th fibonacci
//! number as an output.

// These two lines are necessary for the program to properly compile.
//
// Under the hood, we wrap your main function with some extra code so that it behaves properly
// inside the zkVM.
#![no_main]
sp1_zkvm::entrypoint!(main);

use alloy_sol_types::SolType;
use sp1_derive;
use sp1_lido_accounting_zk_shared::circuit_logic::input_verification::{CycleTracker, InputVerifier};
use sp1_lido_accounting_zk_shared::circuit_logic::report::ReportData;
use sp1_lido_accounting_zk_shared::consts;
use sp1_lido_accounting_zk_shared::eth_consensus_layer::Hash256;
use sp1_lido_accounting_zk_shared::lido::LidoValidatorState;
use tree_hash::TreeHash;

use sp1_lido_accounting_zk_shared::io::eth_io::{
    LidoValidatorStateSolidity, PublicValuesSolidity, ReportMetadataSolidity, ReportSolidity,
};
use sp1_lido_accounting_zk_shared::io::program_io::ProgramInput;

#[sp1_derive::cycle_tracker]
fn h256_to_alloy_type(value: Hash256) -> alloy_primitives::FixedBytes<32> {
    value.to_fixed_bytes().into()
}

struct Sp1CycleTracker {}
impl CycleTracker for Sp1CycleTracker {
    fn start_span(&self, label: &str) {
        println!("cycle-tracker-start: {label}");
    }

    fn end_span(&self, label: &str) {
        println!("cycle-tracker-end: {label}");
    }
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
                hash: h256_to_alloy_type(old_state.tree_hash_root()),
            },
            new_state: LidoValidatorStateSolidity {
                slot: new_state.slot,
                hash: h256_to_alloy_type(new_state.tree_hash_root()),
            },
        },
    };

    let bytes = PublicValuesSolidity::abi_encode(&public_values_solidity);

    // Commit to the public values of the program.
    sp1_zkvm::io::commit_slice(&bytes);
}

pub fn main() {
    let cycle_tracker = Sp1CycleTracker {};

    cycle_tracker.start_span("main.read_args");
    let input: ProgramInput = sp1_zkvm::io::read::<ProgramInput>();
    cycle_tracker.end_span("main.read_args");

    cycle_tracker.start_span("main.verify_inputs");
    let input_verifier = InputVerifier::new(&cycle_tracker);
    input_verifier.prove_input(&input);
    cycle_tracker.end_span("main.verify_inputs");

    cycle_tracker.start_span("main.compute_report");
    let withdrawal_creds: Hash256 = consts::LIDO_WITHDRAWAL_CREDENTIALS.into();

    let new_state: LidoValidatorState = input.old_lido_validator_state.merge_validator_delta(
        input.slot,
        &input.validators_and_balances.validators_delta,
        &withdrawal_creds,
    );

    let report = ReportData::compute_from_state(&new_state, &input.validators_and_balances.balances, &withdrawal_creds);
    cycle_tracker.end_span("main.compute_report");

    cycle_tracker.start_span("main.commit_public_values");
    commit_public_values(
        &report,
        &input.beacon_block_hash.to_fixed_bytes(),
        input.old_lido_validator_state,
        new_state,
    );
    cycle_tracker.end_span("main.commit_public_values");
}
