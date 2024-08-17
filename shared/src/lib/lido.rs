use serde::{Deserialize, Serialize};
use ssz_types::VariableList;
use tree_hash_derive::TreeHash;

use crate::eth_consensus_layer::{self, BeaconState, Epoch, Hash256, Slot, ValidatorIndex, Validators};
use crate::eth_spec;
use crate::util::usize_to_u64;

#[derive(Serialize, Deserialize, TreeHash)]
pub struct LidoValidatorState {
    pub slot: Slot,
    pub epoch: Epoch,
    pub max_validator_index: ValidatorIndex,
    pub deposited_lido_validator_indices: VariableList<ValidatorIndex, eth_spec::ValidatorRegistryLimit>,
    pub future_deposit_lido_validator_indices: VariableList<ValidatorIndex, eth_spec::ValidatorRegistryLimit>,
    pub exited_lido_validator_indices: VariableList<ValidatorIndex, eth_spec::ValidatorRegistryLimit>,
}

impl LidoValidatorState {
    pub fn compute(slot: Slot, validators: &Validators, lido_withdrawal_credentials: &Hash256) -> Self {
        let mut deposited: Vec<ValidatorIndex> = vec![];
        let mut future_deposit: Vec<ValidatorIndex> = vec![];
        let mut exited: Vec<ValidatorIndex> = vec![];

        let epoch = eth_consensus_layer::epoch(slot).unwrap();
        let max_validator_index = usize_to_u64(validators.len()) - 1;

        for (idx, validator) in validators.iter().enumerate() {
            if validator.withdrawal_credentials != *lido_withdrawal_credentials {
                continue;
            }

            if epoch >= validator.activation_eligibility_epoch {
                deposited.push(usize_to_u64(idx));
            } else {
                future_deposit.push(usize_to_u64(idx));
            }
            if epoch >= validator.exit_epoch {
                exited.push(usize_to_u64(idx));
            }
        }
        Self {
            slot,
            epoch,
            max_validator_index,
            deposited_lido_validator_indices: deposited.into(),
            future_deposit_lido_validator_indices: future_deposit.into(),
            exited_lido_validator_indices: exited.into(),
        }
    }

    pub fn compute_from_beacon_state(bs: &BeaconState, lido_withdrawal_credentials: &Hash256) -> Self {
        Self::compute(bs.slot, &bs.validators, lido_withdrawal_credentials)
    }
}
