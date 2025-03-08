use std::collections::HashSet;

use log;
use sp1_lido_accounting_zk_lib::eth_consensus_layer::{BeaconState, Epoch, ValidatorIndex, Validators};
use sp1_lido_accounting_zk_lib::io::eth_io::{BeaconChainSlot, HaveEpoch};
use sp1_lido_accounting_zk_lib::lido::{LidoValidatorState, ValidatorDelta, ValidatorWithIndex};
use sp1_lido_accounting_zk_lib::util::u64_to_usize;

#[derive(Debug, Clone)]
pub struct ValidatorDeltaComputeBeaconStateProjection<'a> {
    slot: BeaconChainSlot,
    validators: &'a Validators,
}

impl<'a> ValidatorDeltaComputeBeaconStateProjection<'a> {
    pub fn new(slot: BeaconChainSlot, validators: &'a Validators) -> Self {
        Self { slot, validators }
    }
    pub fn from_bs(bs: &'a BeaconState) -> Self {
        Self::new(BeaconChainSlot(bs.slot), &bs.validators)
    }
}

pub struct ValidatorDeltaCompute<'a> {
    old_bs: ValidatorDeltaComputeBeaconStateProjection<'a>,
    old_state: &'a LidoValidatorState,
    new_bs: ValidatorDeltaComputeBeaconStateProjection<'a>,
    // This flag disables some sanity checks
    // This should normally be set to true, except for the data tampering tests, where it gets in
    // the way of some tampering scenarios
    skip_verification: bool,
}

fn check_epoch_based_change(old_bs_epoch: Epoch, new_bs_epoch: Epoch, old_epoch: Epoch, new_epoch: Epoch) -> bool {
    if old_epoch != new_epoch {
        return true;
    }
    if (old_epoch < old_bs_epoch) != (new_epoch < new_bs_epoch) {
        return true;
    }
    false
}

impl<'a> ValidatorDeltaCompute<'a> {
    pub fn new(
        old_bs: ValidatorDeltaComputeBeaconStateProjection<'a>,
        old_state: &'a LidoValidatorState,
        new_bs: ValidatorDeltaComputeBeaconStateProjection<'a>,
        skip_verification: bool,
    ) -> Self {
        Self {
            old_bs,
            old_state,
            new_bs,
            skip_verification,
        }
    }

    fn compute_changed(&self) -> HashSet<ValidatorIndex> {
        let mut lido_changed_indices: HashSet<ValidatorIndex> = self
            .old_state
            .pending_deposit_lido_validator_indices
            .iter()
            .copied()
            .collect();

        // ballpark estimating ~32000 validators changed per oracle report should be waaaay more than enough
        // Better estimate could be (new_slot - old_slot) * avg_changes_per_slot, but the impact is likely marginal
        // If underestimated, the vec will transparently resize and reallocate more memory, so the only
        // effect is slightly slower run time - which has negligible impact, unless this gets into shared and used in the ZK part
        lido_changed_indices.reserve(32000);

        let old_bs_epoch = self.old_bs.slot.epoch();
        let new_bs_epoch = self.new_bs.slot.epoch();

        for index in &self.old_state.deposited_lido_validator_indices {
            // for already deposited validators, we want to check if something material have changed:
            // this can only be activation epoch or exist epoch. Theoretically "slashed" can also be
            // relevant, but for now we have no use for it
            let index_usize = u64_to_usize(*index);
            let old_validator = &self.old_bs.validators[index_usize];
            let new_validator = &self.new_bs.validators[index_usize];

            if !self.skip_verification {
                assert!(
                    old_validator.pubkey == new_validator.pubkey,
                    "Validators at index {} in old and new beacon state have different pubkeys",
                    index
                );
            }
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
                        index: *index,
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

#[cfg(test)]
mod test {
    use rand::Rng;
    use sp1_lido_accounting_zk_lib::{
        eth_consensus_layer::{BlsPublicKey, Hash256, Validator, Validators},
        io::eth_io::{BeaconChainSlot, HaveEpoch},
        lido::{LidoValidatorState, ValidatorDelta, ValidatorStatus, ValidatorWithIndex},
        util::usize_to_u64,
    };

    use super::{ValidatorDeltaCompute, ValidatorDeltaComputeBeaconStateProjection};

    const SLOT: BeaconChainSlot = BeaconChainSlot(1200);
    const FUTURE_SLOT: BeaconChainSlot = BeaconChainSlot(1200 + 12 * 10); // 10 epochs forward

    mod creds {
        use lazy_static::lazy_static;
        use sp1_lido_accounting_zk_lib::eth_consensus_layer::Hash256;

        // Not real ones, just test double that we'll treat as lido
        lazy_static! {
            pub static ref LIDO: Hash256 = Hash256::random();
            pub static ref NON_LIDO: Hash256 = Hash256::random();
        }
    }

    fn random_pubkey() -> BlsPublicKey {
        let mut vals: [u8; 48] = [0; 48];
        rand::thread_rng().fill(&mut vals);
        vals.to_vec().into()
    }

    fn create_validator(creds: Hash256, slot: BeaconChainSlot, status: ValidatorStatus) -> Validator {
        let epoch = slot.epoch();
        let (activation_eligible, activated, withdrawable, exited) = match status {
            ValidatorStatus::FutureDeposit => (epoch - 10, u64::MAX, u64::MAX, u64::MAX),
            ValidatorStatus::Deposited => (epoch - 10, epoch + 5, u64::MAX, u64::MAX),
            ValidatorStatus::Exited => (epoch - 10, epoch - 5, epoch - 3, epoch - 1),
        };
        Validator {
            pubkey: random_pubkey(),
            withdrawal_credentials: creds,
            effective_balance: 32000000000,
            slashed: false,
            activation_eligibility_epoch: activation_eligible,
            activation_epoch: activated,
            exit_epoch: exited,
            withdrawable_epoch: withdrawable,
        }
    }

    fn default_validators() -> Vec<Validator> {
        vec![
            create_validator(*creds::LIDO, SLOT, ValidatorStatus::Deposited),
            create_validator(*creds::NON_LIDO, SLOT, ValidatorStatus::Deposited),
            create_validator(*creds::NON_LIDO, SLOT, ValidatorStatus::Deposited),
        ]
    }

    fn compute(old_validators: Vec<Validator>, new_validators: Vec<Validator>) -> ValidatorDelta {
        let old: Validators = old_validators.into();
        let new: Validators = new_validators.into();
        let old_state = LidoValidatorState::compute(SLOT, &old, &creds::LIDO);
        let compute = ValidatorDeltaCompute::new(
            ValidatorDeltaComputeBeaconStateProjection::new(SLOT, &old),
            &old_state,
            ValidatorDeltaComputeBeaconStateProjection::new(FUTURE_SLOT, &new),
            false,
        );

        compute.compute()
    }

    #[test]
    pub fn test_validator_delta_no_change() {
        let original_validators = default_validators();
        let new_validators = original_validators.clone();

        let actual = compute(original_validators, new_validators);
        assert!(actual.all_added.is_empty());
        assert!(actual.lido_changed.is_empty());
    }

    #[test]
    pub fn test_validator_delta_add_deposited_lido() {
        let original_validators = default_validators();
        let extra = create_validator(*creds::LIDO, SLOT + 1, ValidatorStatus::Deposited);
        let mut new_validators = original_validators.clone();
        new_validators.push(extra.clone());

        let actual = compute(original_validators, new_validators);
        assert_eq!(
            actual.all_added,
            vec![ValidatorWithIndex {
                index: 3,
                validator: extra
            }]
        );
        assert!(actual.lido_changed.is_empty());
    }

    #[test]
    pub fn test_validator_delta_add_exited_lido() {
        let original_validators = default_validators();
        let extra = create_validator(*creds::LIDO, SLOT + 1, ValidatorStatus::Exited);
        let mut new_validators = original_validators.clone();

        let expected_added = vec![ValidatorWithIndex {
            index: 3,
            validator: extra.clone(),
        }];
        new_validators.push(extra);

        let actual = compute(original_validators, new_validators);
        assert_eq!(actual.all_added, expected_added);
        assert!(actual.lido_changed.is_empty());
    }

    #[test]
    pub fn test_validator_delta_add_deposited_non_lido() {
        let original_validators = default_validators();
        let extra = create_validator(*creds::NON_LIDO, SLOT + 1, ValidatorStatus::Deposited);
        let mut new_validators = original_validators.clone();
        new_validators.push(extra.clone());

        let actual = compute(original_validators, new_validators);
        assert_eq!(
            actual.all_added,
            vec![ValidatorWithIndex {
                index: 3,
                validator: extra
            }]
        );
        assert!(actual.lido_changed.is_empty());
    }

    #[test]
    pub fn test_validator_delta_exit_non_lido() {
        let original_validators = default_validators();
        let mut new_validators = original_validators.clone();
        new_validators[1].exit_epoch = SLOT.epoch() - 5;

        let actual = compute(original_validators, new_validators);
        assert!(actual.all_added.is_empty());
        assert!(actual.lido_changed.is_empty());
    }

    #[test]
    pub fn test_validator_delta_exit_lido() {
        let idx = 0;
        let original_validators = default_validators();
        let mut new_validators = original_validators.clone();
        new_validators[idx].exit_epoch = SLOT.epoch() - 5;
        let expected_changed = vec![ValidatorWithIndex {
            index: usize_to_u64(idx),
            validator: new_validators[idx].clone(),
        }];

        let actual = compute(original_validators, new_validators);
        assert!(actual.all_added.is_empty());
        assert_eq!(actual.lido_changed, expected_changed);
    }

    #[test]
    pub fn test_validator_delta_all_changes() {
        let mut original_validators = default_validators();
        let mut extra_orig = vec![
            create_validator(*creds::LIDO, SLOT + 5, ValidatorStatus::Deposited),
            create_validator(*creds::LIDO, SLOT + 12, ValidatorStatus::FutureDeposit),
            create_validator(*creds::LIDO, SLOT + 15, ValidatorStatus::Exited),
            create_validator(*creds::NON_LIDO, SLOT + 15, ValidatorStatus::Exited),
            create_validator(*creds::NON_LIDO, SLOT + 20, ValidatorStatus::FutureDeposit),
        ];
        original_validators.append(&mut extra_orig);
        // state: 0, 3 - lido deposited, 4, lido pending, 5 - lido exited, 1,2,6,7-non-lido

        let mut new_validators = original_validators.clone();
        new_validators[0].exit_epoch = SLOT.epoch() + 3; // deposited => exited
        new_validators[1].exit_epoch = SLOT.epoch() + 3; // non lido deposited => exited
        new_validators[4].activation_epoch = FUTURE_SLOT.epoch() - 5; // pending => deposited
        let mut extra_new = vec![
            create_validator(*creds::LIDO, SLOT + 12, ValidatorStatus::Deposited),
            create_validator(*creds::LIDO, SLOT + 22, ValidatorStatus::Exited),
            create_validator(*creds::LIDO, SLOT + 22, ValidatorStatus::FutureDeposit),
            create_validator(*creds::NON_LIDO, SLOT + 30, ValidatorStatus::Deposited),
        ];
        new_validators.append(&mut extra_new);
        // state: 3, 4, 8 - lido deposited, 10 - lido pending, 5, 9 - lido exited, 1,2,6,7,11-non-lido
        // added: 8, 9, 10, 11; lido_changed: 0, 4

        let expected_added: Vec<ValidatorWithIndex> = [8_usize, 9, 10, 11]
            .iter()
            .map(|idx| ValidatorWithIndex {
                index: usize_to_u64(*idx),
                validator: new_validators[*idx].clone(),
            })
            .collect();

        let expected_changed: Vec<ValidatorWithIndex> = [0_usize, 4]
            .iter()
            .map(|idx| ValidatorWithIndex {
                index: usize_to_u64(*idx),
                validator: new_validators[*idx].clone(),
            })
            .collect();

        let actual = compute(original_validators, new_validators);
        assert_eq!(actual.all_added, expected_added);
        assert_eq!(actual.lido_changed, expected_changed);
    }
}
