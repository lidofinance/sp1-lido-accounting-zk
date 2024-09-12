use std::collections::HashSet;

use log;
use sp1_lido_accounting_zk_shared::eth_consensus_layer::{BeaconState, Epoch, ValidatorIndex};
use sp1_lido_accounting_zk_shared::lido::{LidoValidatorState, ValidatorDelta, ValidatorWithIndex};
use sp1_lido_accounting_zk_shared::util::u64_to_usize;

pub struct ValidatorDeltaCompute<'a> {
    old_bs: &'a BeaconState,
    old_state: &'a LidoValidatorState,
    new_bs: &'a BeaconState,
}

fn check_epoch_based_change(old_bs_epoch: Epoch, new_bs_epoch: Epoch, old_epoch: Epoch, new_epoch: Epoch) -> bool {
    if old_epoch != new_epoch {
        return true;
    }
    if (old_epoch < old_bs_epoch) != (new_epoch < new_bs_epoch) {
        return true;
    }
    return false;
}

impl<'a> ValidatorDeltaCompute<'a> {
    pub fn new(old_bs: &'a BeaconState, old_state: &'a LidoValidatorState, new_bs: &'a BeaconState) -> Self {
        Self {
            old_bs,
            old_state,
            new_bs,
        }
    }

    fn compute_changed(&self) -> HashSet<ValidatorIndex> {
        let mut lido_changed_indices: HashSet<ValidatorIndex> = self
            .old_state
            .pending_deposit_lido_validator_indices
            .iter()
            .copied()
            .collect();

        // ballpark estimating ~32000 validators changed per oracle report should waaaay more than enough
        // Better estimate could be (new_slot - old_slot) * avg_changes_per_slot, but the impact is likely marginal
        // If underestimated, the vec will transparently resize and reallocate more memory, so the only
        // effect is slightly slower run time - which is ok, unless (again) this gets into shared and used in the ZK part
        lido_changed_indices.reserve(32000);

        let old_bs_epoch = self.old_bs.epoch();
        let new_bs_epoch = self.new_bs.epoch();

        for index in &self.old_state.deposited_lido_validator_indices {
            // for already deposited validators, we want to check if something material have changed:
            // this can only be activation epoch or exist epoch. Theoretically "slashed" can also be
            // relevant, but for now we have no use for it
            let index_usize = u64_to_usize(*index);
            let old_validator = &self.old_bs.validators[index_usize];
            let new_validator = &self.new_bs.validators[index_usize];

            assert!(
                old_validator.pubkey == new_validator.pubkey,
                "Validators at index {} in old and new beacon state have different pubkeys",
                index
            );
            if check_epoch_based_change(
                old_bs_epoch,
                new_bs_epoch,
                old_validator.exit_epoch,
                new_validator.exit_epoch,
            ) {
                lido_changed_indices.insert(*index);
            }
            if check_epoch_based_change(
                old_bs_epoch,
                new_bs_epoch,
                old_validator.activation_epoch,
                new_validator.activation_epoch,
            ) {
                lido_changed_indices.insert(*index);
            }
        }

        lido_changed_indices
    }

    fn read_validators(&self, indices: Vec<ValidatorIndex>) -> Vec<ValidatorWithIndex> {
        indices
            .iter()
            .filter_map(|index| {
                self.new_bs
                    .validators
                    .get(u64_to_usize(*index))
                    .map(|v| ValidatorWithIndex {
                        index: index.clone(),
                        validator: v.clone(),
                    })
            })
            .collect()
    }

    pub fn compute(&self) -> ValidatorDelta {
        log::debug!(
            "Validator count: old {}, new {}",
            self.old_bs.validators.len(),
            self.new_bs.validators.len()
        );

        let added_count = self.new_bs.validators.len() - self.old_bs.validators.len();
        let added = self.old_state.indices_for_adjacent_delta(added_count).collect();
        let mut changed: Vec<u64> = self.compute_changed().into_iter().collect();
        changed.sort(); // this is important - otherwise equality comparisons and hash computation won't work as expected

        ValidatorDelta {
            all_added: self.read_validators(added),
            lido_changed: self.read_validators(changed),
        }
    }
}
