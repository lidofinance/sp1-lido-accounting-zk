use crate::{
    eth_consensus_layer::{Balances, Hash256, Validators},
    io::eth_io::ReferenceSlot,
    lido::{LidoValidatorState, ValidatorOps, ValidatorStatus},
    util::{erroring_add, u64_to_usize, usize_to_u64, IntegerError},
};
use serde::{Deserialize, Serialize};

#[derive(PartialEq, Eq, Clone, Debug, Serialize, Deserialize)]
pub struct ReportData {
    pub slot: ReferenceSlot,
    pub epoch: u64,
    pub lido_withdrawal_credentials: Hash256,
    pub deposited_lido_validators: u64,
    pub exited_lido_validators: u64,
    pub lido_cl_balance: u64,
}

// Merge into ReportRust?
impl ReportData {
    pub fn compute(
        slot: ReferenceSlot,
        epoch: u64,
        validators: &Validators,
        balances: &Balances,
        lido_creds: Hash256,
    ) -> Result<Self, IntegerError> {
        let mut cl_balance: u64 = 0;
        let mut deposited: u64 = 0;
        let mut exited: u64 = 0;

        for (validator, balance) in validators.iter().zip(balances.iter()) {
            if validator.withdrawal_credentials != lido_creds {
                continue;
            }

            // IMPORTANT: exited and deposited statuses are exclusive
            // Exited should count towards both exited and deposited count
            match validator.status(epoch) {
                ValidatorStatus::FutureDeposit | ValidatorStatus::Deposited => {
                    deposited = erroring_add(deposited, 1)?;
                    cl_balance = erroring_add(cl_balance, *balance)?;
                }
                ValidatorStatus::Exited => {
                    exited = erroring_add(exited, 1)?;
                    deposited = erroring_add(deposited, 1)?;
                    cl_balance = erroring_add(cl_balance, *balance)?;
                }
            }
        }
        let result = Self {
            slot,
            epoch,
            lido_withdrawal_credentials: lido_creds,
            deposited_lido_validators: deposited,
            exited_lido_validators: exited,
            lido_cl_balance: cl_balance,
        };
        Ok(result)
    }

    pub fn compute_from_state(
        reference_slot: ReferenceSlot,
        lido_validators_state: &LidoValidatorState,
        balances: &Balances,
        lido_withdrawal_credentials: &Hash256,
    ) -> Self {
        let mut cl_balance: u64 = 0;

        let pending_indices = &lido_validators_state.pending_deposit_lido_validator_indices;
        let deposited_indices = &lido_validators_state.deposited_lido_validator_indices;
        let exited_indices = &lido_validators_state.exited_lido_validator_indices;

        for index in pending_indices {
            cl_balance += balances[u64_to_usize(*index)];
        }

        // IMPORTANT: for LidoValidatorState, exited is already included into deposited
        // so exited indices should only count towards exited count, but not cl_balance or
        // deposited count - they are already included into deposited
        for index in deposited_indices {
            cl_balance += balances[u64_to_usize(*index)];
        }

        Self {
            slot: reference_slot,
            epoch: lido_validators_state.epoch,
            lido_withdrawal_credentials: *lido_withdrawal_credentials,
            deposited_lido_validators: usize_to_u64(deposited_indices.len()) + usize_to_u64(pending_indices.len()),
            exited_lido_validators: usize_to_u64(exited_indices.len()),
            lido_cl_balance: cl_balance,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::eth_consensus_layer::test_utils::proptest_utils::*;
    use crate::eth_consensus_layer::{Gwei, Hash256, Validator, WithdrawalCredentials};
    use crate::io::eth_io::{BeaconChainSlot, HaveEpoch, ReferenceSlot};
    use hex_literal::hex;
    use itertools::Itertools;
    use proptest::prelude::*;
    use rand::seq::SliceRandom;

    fn gen_balance() -> impl Strategy<Value = Gwei> {
        1u64..100_000_000u64
    }

    #[derive(Clone, Debug, PartialEq)]
    struct ValidatorWithBalance {
        validator: Validator,
        balance: Gwei,
    }

    #[derive(Debug, Clone)]
    struct ValidatorSetup {
        slot: BeaconChainSlot,
        lido_creds: WithdrawalCredentials,
        lido_pending: Vec<ValidatorWithBalance>,
        lido_deposited: Vec<ValidatorWithBalance>,
        lido_exited: Vec<ValidatorWithBalance>,
        others: Vec<ValidatorWithBalance>,
    }

    fn find_validator_indices(container: &[ValidatorWithBalance], targets: &[ValidatorWithBalance]) -> Vec<u64> {
        container
            .iter()
            .enumerate()
            .filter_map(|(idx, validator)| {
                if targets.contains(validator) {
                    Some(usize_to_u64(idx))
                } else {
                    None
                }
            })
            .collect()
    }

    impl ValidatorSetup {
        fn all_validators(&self, shuffle: bool) -> Vec<ValidatorWithBalance> {
            let mut all_validators = [
                self.lido_deposited.clone(),
                self.lido_pending.clone(),
                self.lido_exited.clone(),
                self.others.clone(),
            ]
            .concat();

            if shuffle {
                // Shuffle all_validators for randomized order
                let mut rng = rand::rng();
                all_validators.shuffle(&mut rng);
            }
            all_validators
        }

        fn total_deposited_balance(&self) -> u64 {
            self.lido_deposited.iter().map(|v| v.balance).sum()
        }

        fn total_exited_balance(&self) -> u64 {
            self.lido_exited.iter().map(|v| v.balance).sum()
        }

        fn total_pending_balance(&self) -> u64 {
            self.lido_pending.iter().map(|v| v.balance).sum()
        }

        fn to_lido_validator_state(&self, shuffle: bool) -> (LidoValidatorState, Balances) {
            let all_validators = self.all_validators(shuffle);

            let balances: Vec<Gwei> = all_validators.iter().map(|v| v.balance).collect();

            let mut depostied_indices: Vec<u64> = find_validator_indices(&all_validators, &self.lido_deposited);
            let pending_indices: Vec<u64> = find_validator_indices(&all_validators, &self.lido_pending);
            let exited_indices: Vec<u64> = find_validator_indices(&all_validators, &self.lido_exited);

            // IMPORTANT: for lido validator state, exited is included into deposited
            depostied_indices.extend(exited_indices.clone());

            let state = LidoValidatorState {
                slot: self.slot,
                epoch: self.slot.epoch(),
                max_validator_index: all_validators.len().try_into().expect("Should convert fine"),
                deposited_lido_validator_indices: depostied_indices.into(),
                exited_lido_validator_indices: exited_indices.into(),
                pending_deposit_lido_validator_indices: pending_indices.into(),
            };

            (state, balances.into())
        }
    }

    fn gen_validator_with_balance<V, W>(
        validator_strategy: V,
        withdrawal_creds_strategy: W,
    ) -> impl Strategy<Value = ValidatorWithBalance>
    where
        V: Strategy<Value = Validator>,
        W: Strategy<Value = WithdrawalCredentials>,
    {
        (validator_strategy, withdrawal_creds_strategy, gen_balance()).prop_map(
            |(mut validator, withdrawal_creds, balance)| {
                validator.withdrawal_credentials = withdrawal_creds;
                ValidatorWithBalance { validator, balance }
            },
        )
    }

    fn gen_validator_setup() -> impl Strategy<Value = ValidatorSetup> {
        (10_000u64..100_000u64, 0usize..16, 0usize..16, 0usize..16, 0usize..16)
            .prop_flat_map(|(slot_number, deposited, pending, exited, other)| {
                let slot = BeaconChainSlot(slot_number);
                let epoch = slot.epoch();

                let lido_creds: Hash256 =
                    hex!("1010101010101010101010101010101010101010101010101010101010101010").into();

                (
                    Just(slot),
                    Just(lido_creds),
                    prop::collection::vec(
                        gen_validator_with_balance(deposited_validator(epoch), Just(lido_creds)),
                        deposited,
                    ),
                    prop::collection::vec(
                        gen_validator_with_balance(pending_validator(epoch), Just(lido_creds)),
                        pending,
                    ),
                    prop::collection::vec(
                        gen_validator_with_balance(exited_validator(epoch), Just(lido_creds)),
                        exited,
                    ),
                    prop::collection::vec(
                        gen_validator_with_balance(gen_validator(epoch), Hash256::arbitrary()),
                        other,
                    ),
                )
            })
            .prop_map(
                |(slot, lido_creds, deposited, pending, exited, others)| ValidatorSetup {
                    slot,
                    lido_creds,
                    lido_deposited: deposited,
                    lido_pending: pending,
                    lido_exited: exited,
                    others,
                },
            )
    }

    proptest! {
        #[test]
        fn test_compute(
            validator_setup in gen_validator_setup()
        ) {
            let refslot = ReferenceSlot(validator_setup.slot.0);

            let pending_count = validator_setup.lido_pending.len() as u64;
            let deposited_count = validator_setup.lido_deposited.len() as u64;
            let exited_count = validator_setup.lido_exited.len() as u64;
            let expected_deposit_count = pending_count + deposited_count + exited_count;
            let expected_balance: u64 = validator_setup.total_pending_balance() + validator_setup.total_deposited_balance() + validator_setup.total_exited_balance();

            let all_validators: Vec<ValidatorWithBalance> = validator_setup.all_validators(true);
            let validators: Vec<Validator> = all_validators.iter().map(|v| v.validator.clone()).collect_vec();
            let balances: Vec<Gwei> = all_validators.iter().map(|v| v.balance).collect_vec();

            let report = ReportData::compute(refslot, refslot.epoch(), &validators.into(), &balances.into(), validator_setup.lido_creds).expect("Must no fail");

            prop_assert_eq!(report.slot, refslot);
            prop_assert_eq!(report.epoch, refslot.epoch());
            prop_assert_eq!(report.lido_withdrawal_credentials, validator_setup.lido_creds);
            prop_assert_eq!(report.deposited_lido_validators, expected_deposit_count);
            prop_assert_eq!(report.exited_lido_validators, exited_count);
            prop_assert_eq!(report.lido_cl_balance, expected_balance);
        }
    }

    proptest! {
        #[test]
        fn test_compute_from_state(
            validator_setup in gen_validator_setup(),

        ) {
            let refslot = ReferenceSlot(validator_setup.slot.0);
            let (state, balances) = validator_setup.to_lido_validator_state(true);

            let pending_count = validator_setup.lido_pending.len() as u64;
            let deposited_count = validator_setup.lido_deposited.len() as u64;
            let exited_count = validator_setup.lido_exited.len() as u64;
            let expected_deposit_count = pending_count + deposited_count + exited_count;
            let expected_balance: u64 = validator_setup.total_pending_balance() + validator_setup.total_deposited_balance() + validator_setup.total_exited_balance();

            let report = ReportData::compute_from_state(refslot, &state, &balances, &validator_setup.lido_creds);

            prop_assert_eq!(report.slot, refslot);
            prop_assert_eq!(report.epoch, refslot.epoch());
            prop_assert_eq!(report.lido_withdrawal_credentials, validator_setup.lido_creds);
            prop_assert_eq!(report.deposited_lido_validators, expected_deposit_count);
            prop_assert_eq!(report.exited_lido_validators, exited_count);
            prop_assert_eq!(report.lido_cl_balance, expected_balance);
        }
    }

    proptest! {
        #[test]
        fn test_compute_and_compute_from_state_align(
            validator_setup in gen_validator_setup(),

        ) {
            let refslot = ReferenceSlot(validator_setup.slot.0);

            let (state, balances) = validator_setup.to_lido_validator_state(true);
            let report_from_state = ReportData::compute_from_state(refslot, &state, &balances, &validator_setup.lido_creds);

            let all_validators: Vec<ValidatorWithBalance> = validator_setup.all_validators(true);
            let validators: Vec<Validator> = all_validators.iter().map(|v| v.validator.clone()).collect_vec();
            let balances: Vec<Gwei> = all_validators.iter().map(|v| v.balance).collect_vec();
            let report = ReportData::compute(refslot, refslot.epoch(), &validators.into(), &balances.into(), validator_setup.lido_creds).expect("Must no fail");

            prop_assert_eq!(report, report_from_state);
        }
    }
}
