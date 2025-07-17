use std::collections::HashSet;

use derive_more;
use sp1_lido_accounting_zk_shared::eth_consensus_layer::{BeaconState, BlsPublicKey, ValidatorIndex, Validators};
use sp1_lido_accounting_zk_shared::io::eth_io::{BeaconChainSlot, HaveEpoch};
use sp1_lido_accounting_zk_shared::lido::{
    LidoValidatorState, ValidatorDelta, ValidatorOps, ValidatorStatus, ValidatorWithIndex,
};
use sp1_lido_accounting_zk_shared::util::{u64_to_usize, usize_to_u64, IntegerError};

use crate::InputChecks;

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

#[derive(derive_more::Debug, thiserror::Error)]
pub enum Error {
    #[error("Validators at index {index} in old and new beacon state have different pubkeys {old:?} != {new:?}")]
    ValidatorPubkeyMismatch {
        index: ValidatorIndex,
        #[debug("{:#?}", old)]
        old: BlsPublicKey,
        #[debug("{:#?}", new)]
        new: BlsPublicKey,
    },
    #[error("Invalid validator state transition {index}: {old_status:?} => {new_status:?}")]
    InvalidValidatorStateTransition {
        index: ValidatorIndex,
        old_status: ValidatorStatus,
        new_status: ValidatorStatus,
    },

    #[error(transparent)]
    IntegerError(#[from] IntegerError),
}

pub struct ValidatorDeltaCompute<'a> {
    old_bs: ValidatorDeltaComputeBeaconStateProjection<'a>,
    old_state: &'a LidoValidatorState,
    new_bs: ValidatorDeltaComputeBeaconStateProjection<'a>,
}

impl<'a> ValidatorDeltaCompute<'a> {
    pub fn new(
        old_bs: ValidatorDeltaComputeBeaconStateProjection<'a>,
        old_state: &'a LidoValidatorState,
        new_bs: ValidatorDeltaComputeBeaconStateProjection<'a>,
    ) -> Self {
        Self {
            old_bs,
            old_state,
            new_bs,
        }
    }

    fn compute_changed(&self) -> Result<HashSet<ValidatorIndex>, Error> {
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

        for &index in &self.old_state.deposited_lido_validator_indices {
            // for already deposited validators, we want to check if something material have changed:
            // this can only be activation epoch or exist epoch. Theoretically "slashed" can also be
            // relevant, but for now we have no use for it
            let index_usize = u64_to_usize(index);
            let old_validator = &self.old_bs.validators[index_usize];
            let new_validator = &self.new_bs.validators[index_usize];

            if old_validator.pubkey != new_validator.pubkey && !InputChecks::is_relaxed() {
                return Err(Error::ValidatorPubkeyMismatch {
                    index,
                    old: old_validator.pubkey.clone(),
                    new: new_validator.pubkey.clone(),
                });
            }

            let old_status = old_validator.status(old_bs_epoch);
            let new_status = new_validator.status(new_bs_epoch);

            match (&old_status, &new_status) {
                (ValidatorStatus::FutureDeposit, ValidatorStatus::Deposited)
                | (ValidatorStatus::FutureDeposit, ValidatorStatus::Exited)
                | (ValidatorStatus::Deposited, ValidatorStatus::Exited) => {
                    lido_changed_indices.insert(index);
                }
                // illegal transitions - blow up
                (ValidatorStatus::Exited, ValidatorStatus::Deposited)
                | (ValidatorStatus::Exited, ValidatorStatus::FutureDeposit)
                | (ValidatorStatus::Deposited, ValidatorStatus::FutureDeposit) => {
                    if !InputChecks::is_relaxed() {
                        return Err(Error::InvalidValidatorStateTransition {
                            index,
                            old_status,
                            new_status,
                        });
                    }
                }
                // noops - safe to skip
                // No change - noop
                (ValidatorStatus::Deposited, ValidatorStatus::Deposited)
                | (ValidatorStatus::FutureDeposit, ValidatorStatus::FutureDeposit)
                | (ValidatorStatus::Exited, ValidatorStatus::Exited) => {}
            }
        }

        Ok(lido_changed_indices)
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

    pub fn compute(&self) -> Result<ValidatorDelta, Error> {
        tracing::debug!(
            "Validator count: old {}, new {}",
            self.old_bs.validators.len(),
            self.new_bs.validators.len()
        );

        let added_count = self.new_bs.validators.len() - self.old_bs.validators.len();
        let added = self
            .old_state
            .indices_for_adjacent_delta(usize_to_u64(added_count))?
            .collect();
        let mut changed: Vec<u64> = self.compute_changed()?.into_iter().collect();
        changed.sort(); // this is important - otherwise equality comparisons and hash computation won't work as expected

        Ok(ValidatorDelta {
            all_added: self.read_validators(added),
            lido_changed: self.read_validators(changed),
        })
    }
}

#[cfg(test)]
mod test {
    use rand::Rng;
    use sp1_lido_accounting_zk_shared::{
        eth_consensus_layer::{BlsPublicKey, Hash256, Validator, Validators},
        io::eth_io::{BeaconChainSlot, HaveEpoch},
        lido::{LidoValidatorState, ValidatorDelta, ValidatorStatus, ValidatorWithIndex},
        util::usize_to_u64,
    };

    use super::{ValidatorDeltaCompute, ValidatorDeltaComputeBeaconStateProjection};

    const SLOT: BeaconChainSlot = BeaconChainSlot(1200);
    const FUTURE_SLOT: BeaconChainSlot = BeaconChainSlot(1200 + 12 * 10); // 10 epochs forward

    mod creds {
        use hex_literal::hex;
        use lazy_static::lazy_static;
        use sp1_lido_accounting_zk_shared::eth_consensus_layer::Hash256;

        // Not real ones, just test double that we'll treat as lido
        lazy_static! {
            pub static ref LIDO: Hash256 =
                hex!("00112233445566778899aabbccddeeff00112233445566778899aabbccddeeff").into();
            pub static ref NON_LIDO: Hash256 =
                hex!("ffeeddccbbaa99887766554433221100ffeeddccbbaa99887766554433221100").into();
        }
    }

    fn random_pubkey() -> BlsPublicKey {
        let mut vals: [u8; 48] = [0; 48];
        rand::rng().fill(&mut vals);
        vals.to_vec().into()
    }

    fn create_validator(creds: Hash256, slot: BeaconChainSlot, status: ValidatorStatus) -> Validator {
        let epoch = slot.epoch();
        let (activation_eligible, activated, withdrawable, exited) = match status {
            ValidatorStatus::FutureDeposit => (u64::MAX, u64::MAX, u64::MAX, u64::MAX),
            ValidatorStatus::Deposited => (epoch - 2, epoch + 1, u64::MAX, u64::MAX),
            ValidatorStatus::Exited => (epoch - 3, epoch - 2, epoch + 1, epoch - 1),
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
        );

        compute.compute().expect("Failed to compute validator delta")
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
        let original_validators = vec![
            /*0*/ create_validator(*creds::LIDO, SLOT, ValidatorStatus::Deposited),
            /*1*/ create_validator(*creds::NON_LIDO, SLOT, ValidatorStatus::Deposited),
            /*2*/ create_validator(*creds::NON_LIDO, SLOT, ValidatorStatus::Deposited),
            /*3*/ create_validator(*creds::LIDO, SLOT + 5, ValidatorStatus::Deposited),
            /*4*/ create_validator(*creds::LIDO, FUTURE_SLOT + 5, ValidatorStatus::FutureDeposit),
            /*5*/ create_validator(*creds::LIDO, SLOT + 15, ValidatorStatus::Exited),
            /*6*/ create_validator(*creds::NON_LIDO, SLOT + 15, ValidatorStatus::Exited),
            /*7*/ create_validator(*creds::NON_LIDO, FUTURE_SLOT + 20, ValidatorStatus::FutureDeposit),
            /*8*/ create_validator(*creds::LIDO, FUTURE_SLOT + 35, ValidatorStatus::FutureDeposit),
        ];
        // state: 0, 3 - lido deposited, 4, 8 lido pending, 5 - lido exited, 1,2,6,7-non-lido

        let mut new_validators = original_validators.clone();
        new_validators[0].exit_epoch = FUTURE_SLOT.epoch() - 3; // deposited => exited
        new_validators[1].exit_epoch = FUTURE_SLOT.epoch() - 3; // non lido deposited => exited
        new_validators[4].activation_eligibility_epoch = FUTURE_SLOT.epoch() - 5; // pending => deposited
        new_validators[8].exit_epoch = FUTURE_SLOT.epoch() - 5; // pending => exited
        let mut extra_new = vec![
            /* 9*/ create_validator(*creds::LIDO, FUTURE_SLOT, ValidatorStatus::Deposited),
            /*10*/ create_validator(*creds::LIDO, FUTURE_SLOT, ValidatorStatus::Exited),
            /*11*/ create_validator(*creds::LIDO, FUTURE_SLOT, ValidatorStatus::FutureDeposit),
            /*12*/ create_validator(*creds::NON_LIDO, FUTURE_SLOT, ValidatorStatus::Deposited),
        ];
        new_validators.append(&mut extra_new);
        // state: 3, 4, 9 - lido deposited, 8, 11 - lido pending, 5, 10 - lido exited, 1,2,6,7,12-non-lido
        // added: 9, 10, 11, 12; lido_changed: 0, 4, 8

        // This could come in handy for debugging
        // tracing_helpers::setup_logger(tracing_helpers::LoggingConfig::default_for_test());
        // tracing::info!("Target epoch: {:?}", FUTURE_SLOT.epoch());
        // for (idx, validator) in new_validators.iter().enumerate() {
        //     tracing::info!(
        //         "Validator {idx} eligibility: {}: {:?}",
        //         validator.activation_eligibility_epoch,
        //         validator.status(FUTURE_SLOT.epoch())
        //     );
        // }

        let expected_added: Vec<ValidatorWithIndex> = [9_usize, 10, 11, 12]
            .iter()
            .map(|idx| ValidatorWithIndex {
                index: usize_to_u64(*idx),
                validator: new_validators[*idx].clone(),
            })
            .collect();

        let expected_changed: Vec<ValidatorWithIndex> = [0_usize, 4, 8]
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
