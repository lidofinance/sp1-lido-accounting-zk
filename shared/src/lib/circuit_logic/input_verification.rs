use std::{collections::HashSet, num::TryFromIntError, sync::Arc};

use alloy_primitives::keccak256;
use alloy_rlp::Decodable;
use eth_trie::{EthTrie, MemoryDB, Trie};
use tree_hash::TreeHash;

use crate::{
    eth_consensus_layer::{
        BeaconStateFields, ExecutionPayloadHeader, ExecutionPayloadHeaderFields, Hash256, Validator,
    },
    eth_execution_layer::EthAccountRlpValue,
    eth_spec,
    hashing::{self, HashHelper, HashHelperImpl},
    io::{
        eth_io::HaveEpoch,
        program_io::{ProgramInput, WithdrawalVaultData},
    },
    lido::{LidoValidatorState, ValidatorDelta, ValidatorWithIndex},
    merkle_proof::{self, FieldProof, MerkleProofWrapper, StaticFieldProof},
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
        tracing::debug!("Start {label}")
    }
    fn end_span(&self, label: &str) {
        tracing::debug!("End {label}")
    }
}

#[derive(derive_more::Debug, thiserror::Error)]
pub enum ConditionCheckFailure {
    #[error("Failed to verify Beacon Block Header hash, got {actual:?}, expected {expected:?}")]
    BeaconBlockHashMismatch {
        #[debug("0x{:?}", hex::encode(actual))]
        actual: Hash256,
        #[debug("0x{:?}", hex::encode(expected))]
        expected: Hash256,
    },
    #[error("Beacon State hash mismatch, got {actual:?}, expected {expected:?}")]
    BeaconStateHashMismatch {
        #[debug("0x{:?}", hex::encode(actual))]
        actual: Hash256,
        #[debug("0x{:?}", hex::encode(expected))]
        expected: Hash256,
    },
    #[error("Wrong old state epoch: passed {epoch}, expected from slot {epoch_from_slot}")]
    OldStateEpochMismatch { epoch: u64, epoch_from_slot: u64 },
    #[error("Deposited validators should be sorted and unique")]
    DepositedValidatorsNotSorted,
    #[error("Pending deposit validators should be sorted and unique")]
    PendingDepositValidatorsNotSorted,
    #[error("Total validators count {total_validators} != balances count {balances_count}")]
    TotalValidatorsCountMismatch { total_validators: u64, balances_count: u64 },
    #[error("Balances hash mismatch, got {actual:?}, expected {expected:?}")]
    BalancesHashMismatch {
        #[debug("0x{:?}", hex::encode(actual))]
        actual: Hash256,
        #[debug("0x{:?}", hex::encode(expected))]
        expected: Hash256,
    },
    #[error("All added should be sorted by index and have no duplicates")]
    AllAddedNotSorted,
    #[error("Old validator count {old_validator_count} != new {new_validator_count}")]
    ValidatorCountMismatchWhenAllAddedEmpty {
        old_validator_count: u64,
        new_validator_count: u64,
    },
    #[error("Lido changed should be sorted by index and have no duplicates")]
    LidoChangedNotSorted,
    #[error("Pending deposits was not empty in the last run")]
    PendingDepositsNotEmpty,
    #[error("Not all new validators were passed - expected {expected}, got {actual}")]
    NotAllNewValidatorsPassed { expected: u64, actual: u64 },
    #[error("Required validators missing. Missed indices: {missed:?}")]
    RequiredValidatorsMissing { missed: Vec<u64> },
    #[error("Failed to construct validators merkle root from delta multiproof")]
    ValidatorMerkleRootConstructionFailed(merkle_proof::Error),
    #[error("Failed to verify validators delta multiproof")]
    ValidatorDeltaMultiproofVerificationFailed(merkle_proof::Error),
    #[error("Withdrawal vault balance mismatch, got {actual:?}, expected {expected:?}")]
    WithdrawalVaultBalanceMismatch {
        actual: alloy_primitives::U256,
        expected: alloy_primitives::U256,
    },
}

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("Could not decode Account RLP encoding from tree: {0:?}")]
    AlloyRLPError(#[from] alloy_rlp::Error),

    #[error("Key {0} not found in the account patricia tree")]
    EthTrieKeyNotFoundError(String),

    #[error("FAiled constructing eth trie {0:?}")]
    EthTrieError(#[from] eth_trie::TrieError),

    #[error("Failed to verify {operation}: {error:?}")]
    MerkleProofError {
        operation: &'static str,
        error: merkle_proof::Error,
    },

    #[error("Failed condition check: {0:?}")]
    ConditionCheck(#[from] ConditionCheckFailure),

    #[error("Failed condition check: {0:?}")]
    U64ToUsizeConversionError(#[from] TryFromIntError),
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
        proof: MerkleProofWrapper,
    ) -> Result<(), Error> {
        let tree_depth = hashing::target_tree_depth::<Validator, eth_spec::ValidatorRegistryLimit>();

        let validators_count = validators_with_indices.len();
        let mut indexes: Vec<usize> = Vec::with_capacity(validators_count);
        let mut hashes: Vec<Hash256> = Vec::with_capacity(validators_count);

        self.cycle_tracker
            .start_span(&format!("{tracker_prefix}.validator_roots"));
        for validator_with_index in validators_with_indices {
            indexes.push(validator_with_index.index.try_into()?);
            hashes.push(validator_with_index.validator.tree_hash_root());
        }
        self.cycle_tracker
            .end_span(&format!("{tracker_prefix}.validator_roots"));

        self.cycle_tracker
            .start_span(&format!("{tracker_prefix}.deserialize_proof"));

        self.cycle_tracker
            .end_span(&format!("{tracker_prefix}.deserialize_proof"));

        self.cycle_tracker
            .start_span(&format!("{tracker_prefix}.reconstruct_root_from_proof"));
        let validators_delta_root = proof
            .build_root_from_proof(
                total_validator_count.next_power_of_two().try_into()?,
                indexes.as_slice(),
                hashes.as_slice(),
                Some(tree_depth),
                Some(total_validator_count.try_into()?),
            )
            .map_err(|e| Error::ConditionCheck(ConditionCheckFailure::ValidatorMerkleRootConstructionFailed(e)))?;
        self.cycle_tracker
            .end_span(&format!("{tracker_prefix}.reconstruct_root_from_proof"));

        self.cycle_tracker.start_span(&format!("{tracker_prefix}.verify_hash"));
        merkle_proof::verify_hashes(validators_hash, &validators_delta_root)
            .map_err(|e| Error::ConditionCheck(ConditionCheckFailure::ValidatorDeltaMultiproofVerificationFailed(e)))?;
        self.cycle_tracker.end_span(&format!("{tracker_prefix}.verify_hash"));
        Ok(())
    }

    fn verify_delta(
        &self,
        delta: &ValidatorDelta,
        old_state: &LidoValidatorState,
        actual_validator_count: u64,
    ) -> Result<(), Error> {
        let all_added_count: u64 = delta.all_added.len().try_into()?;
        let validator_from_delta = old_state.total_validators() + all_added_count;
        if validator_from_delta != actual_validator_count {
            return Err(Error::ConditionCheck(
                ConditionCheckFailure::NotAllNewValidatorsPassed {
                    expected: validator_from_delta,
                    actual: actual_validator_count,
                },
            ));
        }

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
        if !missed_update.is_empty() {
            return Err(Error::ConditionCheck(
                ConditionCheckFailure::RequiredValidatorsMissing {
                    missed: missed_update.into_iter().copied().collect(),
                },
            ));
        }
        Ok(())
    }

    // NOTE: mutable iterator - data is still immutable
    fn is_sorted_and_unique<Elem: PartialOrd>(input: &mut impl Iterator<Item = Elem>) -> bool {
        let next = input.next();
        match next {
            None => true, // empty iterator is sorted
            Some(value) => {
                let mut current = value;
                for new_val in input.by_ref() {
                    if current >= new_val {
                        return false;
                    }
                    current = new_val;
                }
                true
            }
        }
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
    pub fn prove_input(&self, input: &ProgramInput) -> Result<(), Error> {
        let beacon_block_header = &input.beacon_block_header;
        let beacon_state = &input.beacon_state;

        // Beacon Block root == merkle_tree_root(BeaconBlockHeader)
        self.cycle_tracker.start_span("prove_input.beacon_block_header");
        let bh_root = beacon_block_header.tree_hash_root();
        if bh_root != input.beacon_block_hash {
            return Err(Error::ConditionCheck(ConditionCheckFailure::BeaconBlockHashMismatch {
                actual: bh_root,
                expected: input.beacon_block_hash,
            }));
        }
        self.cycle_tracker.end_span("prove_input.beacon_block_header");

        // merkle_tree_root(BeaconState) is included into BeaconBlockHeader
        self.cycle_tracker.start_span("prove_input.beacon_state");
        let bs_root = beacon_state.tree_hash_root();
        if bs_root != beacon_block_header.state_root {
            return Err(Error::ConditionCheck(ConditionCheckFailure::BeaconStateHashMismatch {
                actual: bs_root,
                expected: beacon_block_header.state_root,
            }));
        }
        self.cycle_tracker.end_span("prove_input.beacon_state");

        self.cycle_tracker.start_span("prove_input.old_state");
        let epoch_from_slot = input.old_lido_validator_state.slot.epoch();
        let actual_epoch = input.old_lido_validator_state.epoch;
        if actual_epoch != epoch_from_slot {
            return Err(Error::ConditionCheck(ConditionCheckFailure::OldStateEpochMismatch {
                epoch: actual_epoch,
                epoch_from_slot,
            }));
        }
        self.cycle_tracker.start_span("prove_input.old_state.deposited.sorted");
        if !Self::is_sorted_and_unique(&mut input.old_lido_validator_state.deposited_lido_validator_indices.iter()) {
            return Err(Error::ConditionCheck(
                ConditionCheckFailure::DepositedValidatorsNotSorted,
            ));
        }
        self.cycle_tracker.end_span("prove_input.old_state.deposited.sorted");

        self.cycle_tracker.start_span("prove_input.old_state.pending.sorted");
        if !Self::is_sorted_and_unique(
            &mut input
                .old_lido_validator_state
                .pending_deposit_lido_validator_indices
                .iter(),
        ) {
            return Err(Error::ConditionCheck(
                ConditionCheckFailure::PendingDepositValidatorsNotSorted,
            ));
        }
        self.cycle_tracker.end_span("prove_input.old_state.pending.sorted");
        self.cycle_tracker.end_span("prove_input.old_state");

        // Validators and balances are included into BeaconState (merkle multiproof)
        let vals_and_bals_prefix = "prove_input.vals_and_bals";
        self.cycle_tracker.start_span(vals_and_bals_prefix);

        self.cycle_tracker
            .start_span(&format!("{vals_and_bals_prefix}.total_validators"));
        let total_validators = input.validators_and_balances.total_validators;
        let balances_count = input.validators_and_balances.balances.len().try_into()?;
        if total_validators != balances_count {
            return Err(Error::ConditionCheck(
                ConditionCheckFailure::TotalValidatorsCountMismatch {
                    total_validators,
                    balances_count,
                },
            ));
        }
        self.cycle_tracker
            .end_span(&format!("{vals_and_bals_prefix}.total_validators"));

        self.cycle_tracker
            .start_span(&format!("{vals_and_bals_prefix}.validator_delta"));
        self.verify_delta(
            &input.validators_and_balances.validators_delta,
            &input.old_lido_validator_state,
            total_validators,
        )?;
        self.cycle_tracker
            .end_span(&format!("{vals_and_bals_prefix}.validator_delta"));

        // Step 1: confirm validators and balances hashes are included into beacon_state
        self.cycle_tracker
            .start_span(&format!("{vals_and_bals_prefix}.inclusion_proof"));

        let vals_and_bals_multiproof_leaves = [beacon_state.validators, beacon_state.balances];
        beacon_state
            .verify_serialized(
                &input.validators_and_balances.validators_and_balances_proof,
                &[BeaconStateFields::validators, BeaconStateFields::balances],
                &vals_and_bals_multiproof_leaves,
            )
            .map_err(|e| Error::MerkleProofError {
                operation: "Verify Inclusion Proof for validators and balances",
                error: e,
            })?;
        self.cycle_tracker
            .end_span(&format!("{vals_and_bals_prefix}.inclusion_proof"));

        // Step 2: confirm passed balances match the ones in BeaconState
        self.cycle_tracker
            .start_span(&format!("{vals_and_bals_prefix}.balances"));
        let balances_hash = HashHelperImpl::hash_list(&input.validators_and_balances.balances);
        if balances_hash != beacon_state.balances {
            return Err(Error::ConditionCheck(ConditionCheckFailure::BalancesHashMismatch {
                actual: balances_hash,
                expected: beacon_state.balances,
            }));
        }
        self.cycle_tracker.end_span(&format!("{vals_and_bals_prefix}.balances"));

        self.cycle_tracker.end_span(vals_and_bals_prefix);

        // Step 3: confirm validators delta
        self.cycle_tracker
            .start_span(&format!("{vals_and_bals_prefix}.validator_inclusion_proofs"));

        if !input.validators_and_balances.validators_delta.all_added.is_empty() {
            if !Self::is_sorted_and_unique(&mut input.validators_and_balances.validators_delta.added_indices()) {
                return Err(Error::ConditionCheck(ConditionCheckFailure::AllAddedNotSorted));
            }
            let proof =
                merkle_proof::serde::deserialize_proof(&input.validators_and_balances.added_validators_inclusion_proof)
                    .map_err(|err| Error::MerkleProofError {
                        operation: "Deserialize all_added Inclusion Proof",
                        error: err,
                    })?;
            self.verify_validator_inclusion_proof(
                &format!("{vals_and_bals_prefix}.validator_inclusion_proofs.all_added"),
                total_validators,
                &beacon_state.validators,
                &input.validators_and_balances.validators_delta.all_added,
                proof,
            )?;
        } else {
            // If all added is empty, no validators were added since old report (e.g. rerunning on same slot)
            // in such case, old_report.total_validators should be same as beacon_state.validators.len()
            // We're not passing the validators as a whole, but we do pass all balances - so we can
            // use that instead. We can trust all balances are passed since we have verified in in
            // Step 2
            tracing::info!("ValidatorsDelta.all_added was empty - checking total validator count have not changed");
            self.cycle_tracker
                .start_span(&format!("{vals_and_bals_prefix}.all_added.empty"));
            let old_validator_count = input.old_lido_validator_state.total_validators();
            let new_validator_count = input.validators_and_balances.balances.len().try_into()?;
            if old_validator_count != new_validator_count {
                return Err(Error::ConditionCheck(
                    ConditionCheckFailure::ValidatorCountMismatchWhenAllAddedEmpty {
                        old_validator_count,
                        new_validator_count,
                    },
                ));
            }
            self.cycle_tracker
                .end_span(&format!("{vals_and_bals_prefix}.all_added.empty"));
            tracing::info!("Validator count have not changed since last run");
        }

        if !input.validators_and_balances.validators_delta.lido_changed.is_empty() {
            if !Self::is_sorted_and_unique(&mut input.validators_and_balances.validators_delta.lido_changed_indices()) {
                return Err(Error::ConditionCheck(ConditionCheckFailure::LidoChangedNotSorted));
            }
            let proof = merkle_proof::serde::deserialize_proof(
                &input.validators_and_balances.changed_validators_inclusion_proof,
            )
            .map_err(|err| Error::MerkleProofError {
                operation: "Deserialize lido_changed Inclusion Proof",
                error: err,
            })?;
            self.verify_validator_inclusion_proof(
                &format!("{vals_and_bals_prefix}.validator_inclusion_proofs.lido_changed"),
                total_validators,
                &beacon_state.validators,
                &input.validators_and_balances.validators_delta.lido_changed,
                proof,
            )?;
        } else {
            tracing::info!(
                "ValidatorsDelta.lido_changed was empty - checking pending deposits was empty in previous state"
            );
            self.cycle_tracker
                .start_span(&format!("{vals_and_bals_prefix}.lido_changed.empty"));
            if !input
                .old_lido_validator_state
                .pending_deposit_lido_validator_indices
                .is_empty()
            {
                return Err(Error::ConditionCheck(ConditionCheckFailure::PendingDepositsNotEmpty));
            }
            self.cycle_tracker
                .end_span(&format!("{vals_and_bals_prefix}.lido_changed.empty"));

            tracing::info!("Pending deposits was empty in the last run");
        }
        self.cycle_tracker
            .end_span(&format!("{vals_and_bals_prefix}.validator_inclusion_proofs"));

        // Step 4: Verify withdrawal vault input
        self.cycle_tracker.start_span("prove_input.widthrawal_vault");
        // Step 4.1: Verify execution payload header
        self.verify_execution_payload_header(input, beacon_state)?;

        self.cycle_tracker
            .start_span("prove_input.widthrawal_vault.balance_proof");
        self.verify_account_balance_proof(
            input.latest_execution_header_data.state_root,
            &input.withdrawal_vault_data,
        )?;
        self.cycle_tracker
            .end_span("prove_input.widthrawal_vault.balance_proof");
        self.cycle_tracker.end_span("prove_input.widthrawal_vault");
        Ok(())
    }

    fn verify_execution_payload_header(
        &self,
        input: &ProgramInput,
        beacon_state: &crate::eth_consensus_layer::BeaconStatePrecomputedHashes,
    ) -> Result<(), Error> {
        self.cycle_tracker
            .start_span("prove_input.widthrawal_vault.latest_execution_header");

        let indices = [ExecutionPayloadHeaderFields::state_root];
        let proof =
            merkle_proof::serde::deserialize_proof(&input.latest_execution_header_data.state_root_inclusion_proof)
                .map_err(|err| Error::MerkleProofError {
                    operation: "Deserialize ExecutionPayloadHeader",
                    error: err,
                })?;
        let hashes: Vec<Hash256> = vec![input.latest_execution_header_data.state_root];
        ExecutionPayloadHeader::verify(&proof, &indices, &hashes, &beacon_state.latest_execution_payload_header)
            .map_err(|err| Error::MerkleProofError {
                operation: "Verify ExecutionPayloadHeader",
                error: err,
            })?;
        self.cycle_tracker
            .end_span("prove_input.widthrawal_vault.latest_execution_header");
        Ok(())
    }

    fn verify_account_balance_proof(
        &self,
        expected_root: Hash256,
        withdrawal_vault_data: &WithdrawalVaultData,
    ) -> Result<(), Error> {
        let key = keccak256(withdrawal_vault_data.vault_address);
        let trie = EthTrie::new(Arc::new(MemoryDB::new(true)));
        let proof: Vec<Vec<u8>> = withdrawal_vault_data.account_proof.clone();
        let found = trie.verify_proof(expected_root.0.into(), key.as_slice(), proof)?;
        let value = match found {
            Some(v) => Ok(v),
            None => Err(Error::EthTrieKeyNotFoundError(hex::encode(key))),
        }?;
        let decoded = EthAccountRlpValue::decode(&mut value.as_slice())?;

        if withdrawal_vault_data.balance != decoded.balance {
            return Err(Error::ConditionCheck(
                ConditionCheckFailure::WithdrawalVaultBalanceMismatch {
                    actual: withdrawal_vault_data.balance,
                    expected: decoded.balance,
                },
            ));
        }
        Ok(())
    }
}

#[cfg(test)]
mod test {
    use std::collections::HashSet;

    use crate::eth_consensus_layer::test_utils::proptest_utils as eth_proptest;
    use crate::eth_consensus_layer::{Epoch, Validators};
    use proptest as prop;
    use proptest::prelude::*;

    use super::{FieldProof, Hash256, InputVerifier, NoopCycleTracker, TreeHash, Validator, ValidatorWithIndex};

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

    const TEST_EPOCH: Epoch = 123456; // any would do

    // larger values still pass, but take a lot of time
    const MAX_VALIDATORS: usize = 256;
    const MAX_VALIDATORS_FOR_PROOF: usize = 16;

    proptest! {
        #[test]
        fn test_validator_sparse_proof(
            validators in prop::collection::vec(eth_proptest::gen_validator(TEST_EPOCH), 1..MAX_VALIDATORS),
            indices in prop::collection::vec(any::<prop::sample::Index>(), 1..MAX_VALIDATORS_FOR_PROOF)
        ) {
            let vals_size = validators.len();
            let target_indices: Vec<usize> = indices
                .into_iter()
                .map(|idx| idx.index(vals_size))
                .collect::<HashSet<_>>()
                .iter().cloned().collect::<Vec<_>>();

            test_validator_multiproof(validators, target_indices);
        }
    }

    fn test_validator_multiproof(validators: Vec<Validator>, target_indices: Vec<usize>) {
        let validator_variable_list: Validators = validators.clone().into();

        let validators_with_indices: Vec<ValidatorWithIndex> = target_indices
            .iter()
            .map(|idx| ValidatorWithIndex {
                index: (*idx)
                    .try_into()
                    .expect("Test: Failed to convert index into validator index"),
                validator: validators[*idx].clone(),
            })
            .collect();

        let cycle_tracker = NoopCycleTracker {};
        let input_verifier = InputVerifier::new(&cycle_tracker);

        let proof = validator_variable_list.get_members_multiproof(target_indices.as_slice());

        input_verifier
            .verify_validator_inclusion_proof(
                "",
                validators
                    .len()
                    .try_into()
                    .expect("Test: Failed to convert number of validators to u64"),
                &validator_variable_list.tree_hash_root(),
                &validators_with_indices,
                proof,
            )
            .expect("Test: Failed to verify multiproof");
    }

    // Some special cases - to ensure they pass too
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
