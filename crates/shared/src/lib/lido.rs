use std::collections::HashSet;
use std::num::TryFromIntError;
use std::ops::Range;

use itertools::Itertools;
use serde::{Deserialize, Serialize};
use ssz_types::VariableList;
use tree_hash_derive::TreeHash;

use crate::eth_consensus_layer::{BeaconState, Epoch, Hash256, Validator, ValidatorIndex, Validators};
use crate::io::eth_io::{BeaconChainSlot, HaveEpoch, HaveSlotWithBlock};
use crate::util::{erroring_add, usize_to_u64, IntegerError};
use crate::{eth_spec, util};

pub type ValidatorIndexList = VariableList<ValidatorIndex, eth_spec::ReducedValidatorRegistryLimit>;

#[derive(derive_more::Debug, thiserror::Error)]
pub enum Error {
    #[error("Invalid validator transition: {validator_index:?}, {old_status:?} => {new_status:?}")]
    InvalidValidatorTransition {
        validator_index: ValidatorIndex,
        old_status: ValidatorStatus,
        new_status: ValidatorStatus,
    },
    #[error("All added list is malformed first index expected to be {expected:?}, got {actual:?}")]
    MalformedAllAddedList {
        actual: ValidatorIndex,
        expected: ValidatorIndex,
    },
    #[error("Lido changed list contained index {index} that is higher than old state max index {max_allowed}")]
    DisallowedIndexInLidoChanged {
        index: ValidatorIndex,
        max_allowed: ValidatorIndex,
    },
    #[error("Passed non-Lido validator in delta")]
    NonLidoValidatorInDelta {
        index: ValidatorIndex,
        #[debug("{:#?}", withdrawal_credentials)]
        withdrawal_credentials: Hash256,
    },
    #[error("Passed non-Lido validator in delta")]
    U64ToUizeConversionError(#[from] TryFromIntError),

    #[error(transparent)]
    IntegerError(#[from] IntegerError),
}

#[derive(derive_more::Debug, thiserror::Error)]
pub enum InvariantError {
    #[error("Lido Validator State invariant violation {0:?}")]
    InvariantViolation(InvariantViolation),
}

#[derive(Debug, Clone)]
pub enum InvariantViolation {
    SlotEpochNotEqualLidoStateEpoch,
    DepositedValidatorsNotSorted,
    PendingValidatorsNotSorted,
    DepositedIndexGreaterThanMaxValidatorIndex,
    PendingIndexGreaterThanMaxValidatorIndex,
}

#[derive(PartialEq, Clone, Debug, Serialize, Deserialize, TreeHash)]
pub struct LidoValidatorState {
    pub slot: BeaconChainSlot,
    pub epoch: Epoch,
    pub max_validator_index: ValidatorIndex,
    pub deposited_lido_validator_indices: ValidatorIndexList,
    pub pending_deposit_lido_validator_indices: ValidatorIndexList,
    // TODO: attackers can manipulate exited by not providing validators that have existed in the update.
    // The only way to close this loophole is to include all the lido validators in each update, but
    // it generally defeats the purpose of state caching, since lido operates ~30% validators.
    //
    // This field is skipped from hashing to prevent a denial of service attack - by manipulating
    // the exited validators, an attacker can "corrupt" the validator state hash and cause future updates
    // from legitimate oracles to fail.
    //
    // Moreover, the harm done this way is temporary - future report from correct, non-compromised oracle
    // will correctly count the previously omitted validators as exited.
    #[tree_hash(skip_hashing)]
    pub exited_lido_validator_indices: ValidatorIndexList,
}

impl HaveSlotWithBlock for LidoValidatorState {
    fn bc_slot(&self) -> BeaconChainSlot {
        self.slot
    }
}

impl LidoValidatorState {
    pub fn check_invariants(&self) -> Result<&Self, InvariantError> {
        if self.slot.epoch() != self.epoch {
            return Err(InvariantError::InvariantViolation(
                InvariantViolation::SlotEpochNotEqualLidoStateEpoch,
            ));
        }
        if !util::is_sorted_ascending_and_unique(&mut self.deposited_lido_validator_indices.iter()) {
            return Err(InvariantError::InvariantViolation(
                InvariantViolation::DepositedValidatorsNotSorted,
            ));
        }
        if !util::is_sorted_ascending_and_unique(&mut self.pending_deposit_lido_validator_indices.iter()) {
            return Err(InvariantError::InvariantViolation(
                InvariantViolation::PendingValidatorsNotSorted,
            ));
        }

        // We know the deposited_lido_validator_indices is sorted ascending, so it's enough to check the last element
        if let Some(&value) = self.deposited_lido_validator_indices.last() {
            if value > self.max_validator_index {
                return Err(InvariantError::InvariantViolation(
                    InvariantViolation::DepositedIndexGreaterThanMaxValidatorIndex,
                ));
            }
        }

        // We know the pending_deposit_lido_validator_indices is sorted ascending, so it's enough to check the last element
        if let Some(&value) = self.pending_deposit_lido_validator_indices.last() {
            if value > self.max_validator_index {
                return Err(InvariantError::InvariantViolation(
                    InvariantViolation::PendingIndexGreaterThanMaxValidatorIndex,
                ));
            }
        }
        Ok(self)
    }

    pub fn total_validators(&self) -> Result<ValidatorIndex, IntegerError> {
        erroring_add(self.max_validator_index, 1)
    }

    pub fn deposited_indices_set(&self) -> HashSet<u64> {
        self.deposited_lido_validator_indices.iter().cloned().collect()
    }

    pub fn exited_indices_set(&self) -> HashSet<u64> {
        self.exited_lido_validator_indices.iter().cloned().collect()
    }

    pub fn compute(slot: BeaconChainSlot, validators: &Validators, lido_withdrawal_credentials: &Hash256) -> Self {
        let mut deposited: Vec<ValidatorIndex> = vec![];
        let mut pending_deposit: Vec<ValidatorIndex> = vec![];
        let mut exited: Vec<ValidatorIndex> = vec![];

        let epoch = slot.epoch();
        let validator_count: u64 = usize_to_u64(validators.len());

        for (idx, validator) in validators.iter().enumerate() {
            if !validator.is_lido(lido_withdrawal_credentials) {
                continue;
            }

            match validator.status(epoch) {
                ValidatorStatus::Deposited => deposited.push(usize_to_u64(idx)),
                ValidatorStatus::FutureDeposit => pending_deposit.push(usize_to_u64(idx)),
                ValidatorStatus::Exited => {
                    deposited.push(usize_to_u64(idx));
                    exited.push(usize_to_u64(idx));
                }
            }
        }
        Self {
            slot,
            epoch,
            max_validator_index: validator_count - 1,
            deposited_lido_validator_indices: deposited.into(),
            pending_deposit_lido_validator_indices: pending_deposit.into(),
            exited_lido_validator_indices: exited.into(),
        }
    }

    pub fn all_lido_validators_indices(&self) -> impl Iterator<Item = &u64> {
        self.deposited_lido_validator_indices
            .iter()
            .chain(self.pending_deposit_lido_validator_indices.iter())
    }

    fn index_of_first_new_validator(&self) -> Result<ValidatorIndex, IntegerError> {
        erroring_add(self.max_validator_index, 1)
    }

    pub fn indices_for_adjacent_delta(&self, added: u64) -> Result<Range<ValidatorIndex>, IntegerError> {
        let first = self.index_of_first_new_validator()?;
        let last = erroring_add(first, added)?;
        Ok(first..last)
    }

    pub fn compute_from_beacon_state(bs: &BeaconState, lido_withdrawal_credentials: &Hash256) -> Self {
        Self::compute(BeaconChainSlot(bs.slot), &bs.validators, lido_withdrawal_credentials)
    }

    pub fn merge_validator_delta(
        &self,
        slot: BeaconChainSlot,
        validator_delta: &ValidatorDelta,
        lido_withdrawal_credentials: &Hash256,
    ) -> Result<Self, Error> {
        let mut new_deposited = self.deposited_lido_validator_indices.to_vec().clone();
        // pending deposit is a bit special - we want to conveniently add and remove to it
        // and convert to sorted at the end. This list will generally be small (<10**3, roughly)
        // so additional overhead of list -> set -> list -> sort should be small/negligible
        let mut new_pending_deposit: HashSet<u64> =
            self.pending_deposit_lido_validator_indices.iter().copied().collect();
        let mut new_exited = self.exited_lido_validator_indices.to_vec().clone();

        let epoch = slot.epoch();
        let expected_first_new = self.index_of_first_new_validator()?;

        if !validator_delta.all_added.is_empty() && validator_delta.all_added[0].index != expected_first_new {
            return Err(Error::MalformedAllAddedList {
                actual: validator_delta.all_added[0].index,
                expected: expected_first_new,
            });
        }
        for validator_with_index in &validator_delta.all_added {
            let validator = &validator_with_index.validator;
            if !validator.is_lido(lido_withdrawal_credentials) {
                continue;
            }
            match validator.status(epoch) {
                ValidatorStatus::Deposited => new_deposited.push(validator_with_index.index),
                ValidatorStatus::FutureDeposit => {
                    new_pending_deposit.insert(validator_with_index.index);
                }
                ValidatorStatus::Exited => {
                    new_deposited.push(validator_with_index.index);
                    new_exited.push(validator_with_index.index);
                }
            }
        }

        for validator_with_index in &validator_delta.lido_changed {
            let validator = &validator_with_index.validator;
            // This check protects from malicious caller passing non-lido validators through lido_changed
            if !validator.is_lido(lido_withdrawal_credentials) {
                return Err(Error::NonLidoValidatorInDelta {
                    index: validator_with_index.index,
                    withdrawal_credentials: validator.withdrawal_credentials,
                });
            }

            if validator_with_index.index > self.max_validator_index {
                return Err(Error::DisallowedIndexInLidoChanged {
                    index: validator_with_index.index,
                    max_allowed: self.max_validator_index,
                });
            }

            let old_status = validator.status(self.epoch);
            let new_status = validator.status(epoch);

            match (&old_status, &new_status) {
                (ValidatorStatus::FutureDeposit, ValidatorStatus::Deposited) => {
                    new_deposited.push(validator_with_index.index);
                    new_pending_deposit.remove(&validator_with_index.index);
                }
                (ValidatorStatus::FutureDeposit, ValidatorStatus::Exited) => {
                    new_deposited.push(validator_with_index.index);
                    new_pending_deposit.remove(&validator_with_index.index);
                    new_exited.push(validator_with_index.index);
                }
                (ValidatorStatus::Deposited, ValidatorStatus::Exited) => {
                    new_exited.push(validator_with_index.index);
                }
                // No change - noop
                (ValidatorStatus::Deposited, ValidatorStatus::Deposited)
                | (ValidatorStatus::FutureDeposit, ValidatorStatus::FutureDeposit)
                | (ValidatorStatus::Exited, ValidatorStatus::Exited) => {}
                // Invalid transitions - violently crash
                (ValidatorStatus::Exited, ValidatorStatus::Deposited)
                | (ValidatorStatus::Exited, ValidatorStatus::FutureDeposit)
                | (ValidatorStatus::Deposited, ValidatorStatus::FutureDeposit) => {
                    return Err(Error::InvalidValidatorTransition {
                        validator_index: validator_with_index.index,
                        old_status,
                        new_status,
                    });
                }
            }
        }

        // Sorts are important to ensure the validator state merkle hash is stable, regardless of the
        // order of validators in delta
        let deposited_list: ValidatorIndexList = new_deposited.into_iter().sorted().collect_vec().into();
        let pending_deposit_list: ValidatorIndexList = new_pending_deposit.into_iter().sorted().collect_vec().into();
        let exited_list: ValidatorIndexList = new_exited.into_iter().sorted().collect_vec().into();
        let added_validator_count: u64 = validator_delta.all_added.len().try_into()?;
        let max_validator_index = erroring_add(self.max_validator_index, added_validator_count)?;

        let result = Self {
            slot,
            epoch,
            max_validator_index,
            deposited_lido_validator_indices: deposited_list,
            pending_deposit_lido_validator_indices: pending_deposit_list,
            exited_lido_validator_indices: exited_list,
        };
        Ok(result)
    }
}

#[derive(PartialEq, Debug)]
pub enum ValidatorStatus {
    Deposited,
    FutureDeposit,
    Exited,
}

pub trait ValidatorOps {
    fn status(&self, epoch: Epoch) -> ValidatorStatus;
    fn is_lido(&self, withdrawal_credentials: &Hash256) -> bool;
}

impl ValidatorOps for Validator {
    fn status(&self, epoch: Epoch) -> ValidatorStatus {
        if epoch >= self.exit_epoch {
            ValidatorStatus::Exited
        } else if epoch >= self.activation_eligibility_epoch {
            ValidatorStatus::Deposited
        } else {
            ValidatorStatus::FutureDeposit
        }
    }

    fn is_lido(&self, withdrawal_credentials: &Hash256) -> bool {
        self.withdrawal_credentials == *withdrawal_credentials
    }
}

#[derive(PartialEq, Debug, Clone, Serialize, Deserialize)]
pub struct ValidatorWithIndex {
    pub index: ValidatorIndex,
    pub validator: Validator,
}

#[derive(PartialEq, Clone, Debug, Serialize, Deserialize)]
pub struct ValidatorDelta {
    pub all_added: Vec<ValidatorWithIndex>,
    pub lido_changed: Vec<ValidatorWithIndex>,
}

impl ValidatorDelta {
    pub fn added_indices(&self) -> impl Iterator<Item = &'_ ValidatorIndex> {
        self.all_added.iter().map(|v: &ValidatorWithIndex| &v.index)
    }

    pub fn lido_changed_indices(&self) -> impl Iterator<Item = &'_ ValidatorIndex> {
        self.lido_changed.iter().map(|v: &ValidatorWithIndex| &v.index)
    }
}

#[cfg(test)]
mod tests {
    use super::{Epoch, ValidatorOps, ValidatorStatus};
    use crate::eth_consensus_layer::test_utils::proptest_utils as eth_proptest;
    use proptest::prelude::*;
    use proptest_arbitrary_interop::arb;

    // Helper function to reduce the number of search hits for `assert` in the production files
    fn check_eq<T: PartialEq + std::fmt::Debug>(left: T, right: T) {
        assert_eq!(left, right);
    }

    proptest! {
        #[test]
        fn test_pending_validator_status_future_deposit(
            (epoch, validator) in (arb::<Epoch>()).prop_flat_map(|epoch| (Just(epoch), eth_proptest::pending_validator(epoch)))
        ) {
            check_eq(validator.status(epoch), ValidatorStatus::FutureDeposit)
        }
    }

    proptest! {
        #[test]
        fn test_deposited_validator_status_deposited(
            (epoch, validator) in (arb::<Epoch>()).prop_flat_map(|epoch| (Just(epoch), eth_proptest::deposited_validator(epoch)))
        ) {
            check_eq(validator.status(epoch), ValidatorStatus::Deposited)
        }
    }

    proptest! {
        #[test]
        fn test_activate_validator_status_deposited(
            (epoch, validator) in (arb::<Epoch>()).prop_flat_map(|epoch| (Just(epoch), eth_proptest::activated_validator(epoch)))
        ) {
            check_eq(validator.status(epoch), ValidatorStatus::Deposited)
        }
    }

    proptest! {
        #[test]
        fn test_withdrawable_validator_status_deposited(
            (epoch, validator) in (arb::<Epoch>()).prop_flat_map(|epoch| (Just(epoch), eth_proptest::withdrawable_validator(epoch)))
        ) {
            check_eq(validator.status(epoch), ValidatorStatus::Deposited)
        }
    }

    proptest! {
        #[test]
        fn test_exited_validator_status_exited(
            (epoch, validator) in (arb::<Epoch>()).prop_flat_map(|epoch| (Just(epoch), eth_proptest::exited_validator(epoch)))
        ) {
            check_eq(validator.status(epoch), ValidatorStatus::Exited)
        }
    }
}
