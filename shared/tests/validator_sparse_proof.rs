use sp1_lido_accounting_zk_shared::{
    circuit_logic::input_verification::{InputVerifier, NoopCycleTracker},
    eth_consensus_layer::{Hash256, Validator, Validators},
    lido::ValidatorWithIndex,
    merkle_proof::FieldProof,
    util::usize_to_u64,
};
use tree_hash::TreeHash;

// TODO: use Arbitrary?
fn create_validator(index: u8) -> Validator {
    let mut pubkey: [u8; 48] = [0; 48];
    pubkey[0] = index;
    Validator {
        pubkey: pubkey.to_vec().into(),
        withdrawal_credentials: Hash256::random(),
        effective_balance: 32_u64 * 10_u64.pow(9),
        slashed: false,
        activation_eligibility_epoch: 10,
        activation_epoch: 12,
        exit_epoch: u64::MAX,
        withdrawable_epoch: 50,
    }
}

fn test_validator_multiproof(validators: Vec<Validator>, target_indices: Vec<usize>) {
    let validator_variable_list: Validators = validators.clone().into();

    let validators_with_indices: Vec<ValidatorWithIndex> = target_indices
        .iter()
        .map(|idx| ValidatorWithIndex {
            index: usize_to_u64(*idx),
            validator: validators[*idx].clone(),
        })
        .collect();

    let cycle_tracker = NoopCycleTracker {};
    let input_verifier = InputVerifier::new(&cycle_tracker);

    let proof = validator_variable_list.get_field_multiproof(target_indices.as_slice());

    input_verifier.verify_validator_inclusion_proof(
        "",
        usize_to_u64(validators.len()),
        &validator_variable_list.tree_hash_root(),
        &validators_with_indices,
        proof,
    );
}

#[test]
fn test_validator_sparse_proof_sequential_indices() {
    let validators: Vec<Validator> = (0u8..20).map(create_validator).collect();
    let prove_indices = vec![1, 7, 12, 19];

    test_validator_multiproof(validators, prove_indices);
}

#[test]
fn test_validator_sparse_proof_sequential_decreasing() {
    let validators: Vec<Validator> = (0u8..20).map(create_validator).collect();
    let prove_indices = vec![12, 8, 6, 3];

    test_validator_multiproof(validators, prove_indices);
}

#[test]
fn test_validator_sparse_proof_out_of_order_indices() {
    let validators: Vec<Validator> = (0u8..20).map(create_validator).collect();
    let prove_indices = vec![7, 12, 19, 1];

    test_validator_multiproof(validators, prove_indices);
}
