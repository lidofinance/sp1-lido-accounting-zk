use std::collections::HashSet;

use rs_merkle::{algorithms::Sha256, MerkleProof};
use tree_hash::TreeHash;

use crate::{
    eth_consensus_layer::{Hash256, Validator},
    eth_spec,
    hashing::{self, HashHelper, HashHelperImpl},
    io::program_io::ProgramInput,
    lido::{LidoValidatorState, ValidatorDelta, ValidatorWithIndex},
    merkle_proof::{self, FieldProof, MerkleTreeFieldLeaves},
    util::{u64_to_usize, usize_to_u64},
};

pub trait CycleTracker {
    fn start_span(&self, label: &str);
    fn end_span(&self, label: &str);
}

pub struct NoopCycleTracker {}

impl CycleTracker for NoopCycleTracker {
    fn start_span(&self, _label: &str) {}
    fn end_span(&self, _label: &str) {}
}

pub struct LogCycleTracker {}
impl CycleTracker for LogCycleTracker {
    fn start_span(&self, label: &str) {
        log::debug!("Start {label}")
    }
    fn end_span(&self, label: &str) {
        log::debug!("End {label}")
    }
}

pub struct InputVerifier<'a, Tracker: CycleTracker> {
    cycle_tracker: &'a Tracker,
}

impl<'a, Tracker: CycleTracker> InputVerifier<'a, Tracker> {
    pub fn new(cycle_tracker: &'a Tracker) -> Self {
        Self { cycle_tracker }
    }

    pub fn verify_validator_inclusion_proof(
        &self,
        tracker_prefix: &str,
        total_validator_count: u64,
        validators_hash: &Hash256,
        validators_with_indices: &Vec<ValidatorWithIndex>,
        proof: MerkleProof<Sha256>,
    ) {
        let tree_depth = hashing::target_tree_depth::<Validator, eth_spec::ValidatorRegistryLimit>();

        let validators_count = validators_with_indices.len();
        let mut indexes: Vec<usize> = Vec::with_capacity(validators_count);
        let mut hashes: Vec<merkle_proof::RsMerkleHash> = Vec::with_capacity(validators_count);

        self.cycle_tracker
            .start_span(&format!("{tracker_prefix}.validator_roots"));
        for validator_with_index in validators_with_indices {
            indexes.push(u64_to_usize(validator_with_index.index));
            hashes.push(validator_with_index.validator.tree_hash_root().to_fixed_bytes());
        }
        self.cycle_tracker
            .end_span(&format!("{tracker_prefix}.validator_roots"));

        self.cycle_tracker
            .start_span(&format!("{tracker_prefix}.deserialize_proof"));

        self.cycle_tracker
            .end_span(&format!("{tracker_prefix}.deserialize_proof"));

        self.cycle_tracker
            .start_span(&format!("{tracker_prefix}.reconstruct_root_from_proof"));
        let validators_delta_root = merkle_proof::build_root_from_proof(
            &proof,
            u64_to_usize(total_validator_count.next_power_of_two()),
            indexes.as_slice(),
            hashes.as_slice(),
            Some(tree_depth),
            Some(u64_to_usize(total_validator_count)),
        )
        .expect("Failed to construct validators merkle root from delta multiproof");
        self.cycle_tracker
            .end_span(&format!("{tracker_prefix}.reconstruct_root_from_proof"));

        self.cycle_tracker.start_span(&format!("{tracker_prefix}.verify_hash"));
        merkle_proof::verify_hashes(validators_hash, &validators_delta_root)
            .expect("Failed to verify validators delta multiproof");
        self.cycle_tracker.end_span(&format!("{tracker_prefix}.verify_hash"));
    }

    fn verify_delta(&self, delta: &ValidatorDelta, old_state: &LidoValidatorState, actual_valdiator_count: u64) {
        let validator_from_delta = old_state.total_validators() + usize_to_u64(delta.all_added.len());
        assert!(
            validator_from_delta == actual_valdiator_count,
            "Not all new validators were passed - expected {validator_from_delta}, got {actual_valdiator_count}"
        );

        let lido_changed_indices: HashSet<u64> = delta.lido_changed_indices().copied().collect();
        let pending_deposit_from_old_state: HashSet<u64> = old_state
            .pending_deposit_lido_validator_indices
            .iter()
            .copied()
            .collect();

        // all validators with pending deposits from old state are required - to make sure they are not omitted
        let missed_update: HashSet<&u64> = pending_deposit_from_old_state
            .difference(&lido_changed_indices)
            .collect();
        assert!(
            missed_update.is_empty(),
            "Required validators missing. Missed indices: {:?}",
            missed_update
        )
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
    pub fn prove_input(&self, input: &ProgramInput) {
        let beacon_block_header = &input.beacon_block_header;
        let beacon_state = &input.beacon_state;

        // Beacon Block root == merkle_tree_root(BeaconBlockHeader)
        self.cycle_tracker.start_span("prove_input.beacon_block_header");
        let bh_root = beacon_block_header.tree_hash_root();
        assert!(
            bh_root == input.beacon_block_hash,
            "Failed to verify Beacon Block Header hash, got {}, expected {}",
            hex::encode(bh_root),
            hex::encode(input.beacon_block_hash),
        );
        self.cycle_tracker.end_span("prove_input.beacon_block_header");

        // merkle_tree_root(BeaconState) is included into BeaconBlockHeader
        self.cycle_tracker.start_span("prove_input.beacon_state");
        let bs_root = beacon_state.tree_hash_root();
        assert!(
            bs_root == beacon_block_header.state_root,
            "Beacon State hash mismatch, got {}, expected {}",
            hex::encode(bs_root),
            hex::encode(beacon_block_header.state_root),
        );
        self.cycle_tracker.end_span("prove_input.beacon_state");

        // Validators and balances are included into BeaconState (merkle multiproof)
        let vals_and_bals_prefix = "prove_input.vals_and_bals";
        self.cycle_tracker.start_span(vals_and_bals_prefix);

        self.cycle_tracker
            .start_span(&format!("{vals_and_bals_prefix}.total_validators"));
        let total_validators = input.validators_and_balances.total_validators;
        assert_eq!(
            total_validators,
            usize_to_u64(input.validators_and_balances.balances.len())
        );
        self.cycle_tracker
            .end_span(&format!("{vals_and_bals_prefix}.total_validators"));

        self.cycle_tracker
            .start_span(&format!("{vals_and_bals_prefix}.validator_delta"));
        self.verify_delta(
            &input.validators_and_balances.validators_delta,
            &input.old_lido_validator_state,
            total_validators,
        );
        self.cycle_tracker
            .end_span(&format!("{vals_and_bals_prefix}.validator_delta"));

        // Step 1: confirm validators and balances hashes are included into beacon_state
        self.cycle_tracker
            .start_span(&format!("{vals_and_bals_prefix}.inclusion_proof"));
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
        self.cycle_tracker
            .end_span(&format!("{vals_and_bals_prefix}.inclusion_proof"));

        // Step 2: confirm passed balances match the ones in BeaconState
        self.cycle_tracker
            .start_span(&format!("{vals_and_bals_prefix}.balances"));
        let balances_hash = HashHelperImpl::hash_list(&input.validators_and_balances.balances);
        assert!(
            balances_hash == beacon_state.balances,
            "Balances hash mismatch, got {}, expected {}",
            hex::encode(balances_hash),
            hex::encode(beacon_state.balances),
        );
        self.cycle_tracker.end_span(&format!("{vals_and_bals_prefix}.balances"));

        self.cycle_tracker.end_span(vals_and_bals_prefix);

        // Step 3: confirm validators delta
        self.cycle_tracker
            .start_span(&format!("{vals_and_bals_prefix}.validator_inclusion_proofs"));

        if !input.validators_and_balances.validators_delta.all_added.is_empty() {
            let proof =
                merkle_proof::serde::deserialize_proof(&input.validators_and_balances.added_validators_inclusion_proof)
                    .expect("Failed to deserialize proof");
            self.verify_validator_inclusion_proof(
                &format!("{vals_and_bals_prefix}.validator_inclusion_proofs.all_added"),
                total_validators,
                &beacon_state.validators,
                &input.validators_and_balances.validators_delta.all_added,
                proof,
            );
        } else {
            // If all added is empty, no validators were added since old report (e.g. rerunning on same slot)
            // in such case, old_report.total_validators should be same as beacon_state.validators.len()
            // We're not passing the validators as a whole, but we do pass all balances - so we can
            // use that instead. We can trust all balances are passed since we have verified in in
            // Step 2
            log::info!("ValidatorsDelta.all_added was empty - checking total validator count have not changed");
            self.cycle_tracker
                .start_span(&format!("{vals_and_bals_prefix}.all_added.empty"));
            assert_eq!(
                input.old_lido_validator_state.total_validators(),
                usize_to_u64(input.validators_and_balances.balances.len())
            );
            self.cycle_tracker
                .end_span(&format!("{vals_and_bals_prefix}.all_added.empty"));
            log::info!("Validator count have not changed since last run");
        }

        if !input.validators_and_balances.validators_delta.lido_changed.is_empty() {
            let proof = merkle_proof::serde::deserialize_proof(
                &input.validators_and_balances.changed_validators_inclusion_proof,
            )
            .expect("Failed to deserialize proof");
            self.verify_validator_inclusion_proof(
                &format!("{vals_and_bals_prefix}.validator_inclusion_proofs.lido_changed"),
                total_validators,
                &beacon_state.validators,
                &input.validators_and_balances.validators_delta.lido_changed,
                proof,
            );
        } else {
            log::info!(
                "ValidatorsDelta.lido_changed was empty - checking pending deposits was empty in previous state"
            );
            self.cycle_tracker
                .start_span(&format!("{vals_and_bals_prefix}.lido_changed.empty"));
            assert!(input
                .old_lido_validator_state
                .pending_deposit_lido_validator_indices
                .is_empty());
            self.cycle_tracker
                .end_span(&format!("{vals_and_bals_prefix}.lido_changed.empty"));

            log::info!("Pending deposits was empty in the last run");
        }

        self.cycle_tracker
            .end_span(&format!("{vals_and_bals_prefix}.validator_inclusion_proofs"));
    }
}

#[cfg(test)]
mod test {
    use crate::eth_consensus_layer::Validators;

    use super::{
        usize_to_u64, FieldProof, Hash256, InputVerifier, NoopCycleTracker, TreeHash, Validator, ValidatorWithIndex,
    };

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
    fn test_validator_sparse_proof_sequential_increasing_indices() {
        let validators: Vec<Validator> = (0u8..20).map(create_validator).collect();
        let prove_indices = vec![4, 5, 6, 7, 8];

        test_validator_multiproof(validators, prove_indices);
    }

    #[test]
    fn test_validator_sparse_proof_increasing_indices() {
        let validators: Vec<Validator> = (0u8..20).map(create_validator).collect();
        let prove_indices = vec![1, 7, 12, 19];

        test_validator_multiproof(validators, prove_indices);
    }

    #[test]
    fn test_validator_sparse_proof_sequential_decreasing() {
        let validators: Vec<Validator> = (0u8..20).map(create_validator).collect();
        let prove_indices = vec![15, 14, 13, 12, 11, 10];

        test_validator_multiproof(validators, prove_indices);
    }

    #[test]
    fn test_validator_sparse_proof_decreasing() {
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
}
