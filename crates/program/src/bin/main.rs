#![no_main]
sp1_zkvm::entrypoint!(main);

use alloy_sol_types::SolType;
use sp1_lido_accounting_zk_shared::circuit_logic::input_verification::{CycleTracker, InputVerifier};
use sp1_lido_accounting_zk_shared::circuit_logic::io::create_public_values;
use sp1_lido_accounting_zk_shared::circuit_logic::report::ReportData;
use sp1_lido_accounting_zk_shared::lido::LidoValidatorState;
use tree_hash::TreeHash;

use sp1_lido_accounting_zk_shared::io::eth_io::{LidoWithdrawalVaultDataRust, PublicValuesSolidity};
use sp1_lido_accounting_zk_shared::io::program_io::ProgramInput;

struct Sp1CycleTracker {}
impl CycleTracker for Sp1CycleTracker {
    fn start_span(&self, label: &str) {
        println!("cycle-tracker-start: {label}");
    }

    fn end_span(&self, label: &str) {
        println!("cycle-tracker-end: {label}");
    }
}

pub fn main() {
    let cycle_tracker = Sp1CycleTracker {};

    cycle_tracker.start_span("main.read_args");
    let input: ProgramInput = sp1_zkvm::io::read::<ProgramInput>();
    cycle_tracker.end_span("main.read_args");

    cycle_tracker.start_span("main.verify_inputs");
    let input_verifier = InputVerifier::new(&cycle_tracker);
    input_verifier.prove_input(&input).expect("Failed to verify input");
    cycle_tracker.end_span("main.verify_inputs");

    cycle_tracker.start_span("main.compute_new_state");
    let new_state: LidoValidatorState = input
        .compute_new_state()
        .expect("Failed to compute new state from input");

    cycle_tracker.start_span("main.compute_new_state.check_invariants");
    new_state
        .check_invariants()
        .expect("New lido validator state violated invariant check");
    cycle_tracker.end_span("main.compute_new_state.check_invariants");
    cycle_tracker.end_span("main.compute_new_state");

    cycle_tracker.start_span("main.compute_new_state.hash_root");
    let new_state_hash_root = new_state.tree_hash_root();
    assert_eq!(new_state_hash_root, input.new_lido_validator_state_hash);
    cycle_tracker.end_span("main.compute_new_state.hash_root");

    cycle_tracker.start_span("main.compute_old_state.hash_root");
    let old_state_hash_root = input.old_lido_validator_state.tree_hash_root();
    cycle_tracker.end_span("main.compute_old_state.hash_root");

    cycle_tracker.start_span("main.compute_report");
    let report = ReportData::compute_from_state(
        input.reference_slot,
        &new_state,
        &input.validators_and_balances.balances,
        &input.validators_and_balances.lido_withdrawal_credentials,
    );
    cycle_tracker.end_span("main.compute_report");

    cycle_tracker.start_span("main.commit_public_values");
    let withdrawal_vault_data: LidoWithdrawalVaultDataRust = input.withdrawal_vault_data.into();
    let public_values = create_public_values(
        &report,
        input.bc_slot,
        &input.beacon_block_hash,
        withdrawal_vault_data,
        input.old_lido_validator_state.slot,
        &old_state_hash_root,
        new_state.slot,
        &new_state_hash_root,
    )
    .expect("Failed to create public values");
    let bytes = PublicValuesSolidity::abi_encode(&public_values);
    sp1_zkvm::io::commit_slice(&bytes);
    cycle_tracker.end_span("main.commit_public_values");
}
