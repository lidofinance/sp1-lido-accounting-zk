use std::collections::HashSet;
use std::ops::Range;

use itertools::Itertools;
use serde::{Deserialize, Serialize};
use ssz_types::VariableList;
use tree_hash_derive::TreeHash;

use crate::eth_consensus_layer::{self, BeaconState, Epoch, Hash256, Slot, Validator, ValidatorIndex, Validators};
use crate::eth_spec;
use crate::util::usize_to_u64;

type ValidatorIndexList = VariableList<ValidatorIndex, eth_spec::ReducedValidatorRegistryLimit>;

#[derive(PartialEq, Clone, Debug, Serialize, Deserialize, TreeHash)]
pub struct LidoValidatorState {
    pub slot: Slot,
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

impl LidoValidatorState {
    pub fn total_validators(&self) -> ValidatorIndex {
        self.max_validator_index + 1
    }

    pub fn compute(slot: Slot, validators: &Validators, lido_withdrawal_credentials: &Hash256) -> Self {
        let mut deposited: Vec<ValidatorIndex> = vec![];
        let mut pending_deposit: Vec<ValidatorIndex> = vec![];
        let mut exited: Vec<ValidatorIndex> = vec![];

        let epoch = eth_consensus_layer::epoch(slot).unwrap();
        let max_validator_index = usize_to_u64(validators.len()) - 1;

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
            max_validator_index,
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

    pub fn index_of_first_new_validator(&self) -> ValidatorIndex {
        self.max_validator_index + 1
    }

    pub fn indices_for_adjacent_delta(&self, added: usize) -> Range<ValidatorIndex> {
        let first = self.index_of_first_new_validator();
        first..(first + usize_to_u64(added))
    }

    pub fn compute_from_beacon_state(bs: &BeaconState, lido_withdrawal_credentials: &Hash256) -> Self {
        Self::compute(bs.slot, &bs.validators, lido_withdrawal_credentials)
    }

    pub fn merge_validator_delta(
        &self,
        slot: Slot,
        validator_delta: &ValidatorDelta,
        lido_withdrawal_credentials: &Hash256,
    ) -> Self {
        let mut new_deposited = self.deposited_lido_validator_indices.to_vec().clone();
        // pending deposit is a bit special - we want to conveniently add and remove to it
        // and convert to sorted at the end. This list will generally be small (<10**3, roughly)
        // so additional overhead of list -> set -> list -> sort should be small/negligible
        let mut new_pending_deposit: HashSet<u64> =
            self.pending_deposit_lido_validator_indices.iter().copied().collect();
        let mut new_exited = self.exited_lido_validator_indices.to_vec().clone();

        let epoch = eth_consensus_layer::epoch(slot).unwrap();

        if !validator_delta.all_added.is_empty() {
            assert!(validator_delta.all_added[0].index == self.index_of_first_new_validator());
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
            // It is expected that the caller will filter out non-Lido validators, but worth double-checking
            assert!(
                validator.is_lido(lido_withdrawal_credentials),
                "Passed non-lido validator"
            );

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
                    panic!(
                        "Invalid status transition for Validator {}: {:?} => {:?}",
                        validator_with_index.index, &old_status, &new_status
                    )
                }
            }
        }

        let pending_deposit_vec: Vec<u64> = new_pending_deposit.into_iter().sorted().collect_vec();
        let exited_deposit_vec: Vec<u64> = new_exited.into_iter().sorted().collect_vec();

        let deposited_list: ValidatorIndexList = new_deposited.into();
        let pending_deposit_list: ValidatorIndexList = pending_deposit_vec.into();
        let exited_list: ValidatorIndexList = exited_deposit_vec.into();

        Self {
            slot,
            epoch,
            max_validator_index: self.max_validator_index + usize_to_u64(validator_delta.all_added.len()),
            deposited_lido_validator_indices: deposited_list,
            pending_deposit_lido_validator_indices: pending_deposit_list,
            exited_lido_validator_indices: exited_list,
        }
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

#[derive(PartialEq, Debug, Serialize, Deserialize)]
pub struct ValidatorWithIndex {
    pub index: ValidatorIndex,
    pub validator: Validator,
}

#[derive(PartialEq, Debug, Serialize, Deserialize)]
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
