use crate::{
    eth_consensus_layer::{Balances, Hash256, Validators},
    lido::LidoValidatorState,
    util::{u64_to_usize, usize_to_u64},
};
use serde::{Deserialize, Serialize};

#[derive(PartialEq, Eq, Clone, Debug, Serialize, Deserialize)]
pub struct ReportData {
    pub slot: u64,
    pub epoch: u64,
    pub lido_withdrawal_credentials: Hash256,
    pub deposited_lido_validators: u64,
    pub exited_lido_validators: u64,
    pub lido_cl_balance: u64,
}

impl ReportData {
    pub fn compute(
        slot: u64,
        epoch: u64,
        validators: &Validators,
        balances: &Balances,
        lido_withdrawal_credentials: &Hash256,
    ) -> Self {
        let mut cl_balance: u64 = 0;
        let mut deposited: u64 = 0;
        let mut exited: u64 = 0;

        // make a clone to disentangle report lifetime from withdrawal credential lifetime
        let creds = lido_withdrawal_credentials.clone();

        for (validator, balance) in validators.iter().zip(balances.iter()) {
            if validator.withdrawal_credentials != creds {
                continue;
            }

            cl_balance += *balance;
            if epoch >= validator.activation_eligibility_epoch {
                deposited += 1;
            }
            if epoch >= validator.exit_epoch {
                exited += 1
            }
        }
        Self {
            slot,
            epoch,
            lido_withdrawal_credentials: creds,
            deposited_lido_validators: deposited,
            exited_lido_validators: exited,
            lido_cl_balance: cl_balance,
        }
    }

    pub fn compute_from_state(
        lido_validators_state: &LidoValidatorState,
        balances: &Balances,
        lido_withdrawal_credentials: &Hash256,
    ) -> Self {
        let mut cl_balance: u64 = 0;

        let deposited_indices = &lido_validators_state.deposited_lido_validator_indices;

        for index in deposited_indices {
            cl_balance += balances[u64_to_usize(*index)];
        }

        return Self {
            slot: lido_validators_state.slot,
            epoch: lido_validators_state.epoch,
            lido_withdrawal_credentials: lido_withdrawal_credentials.clone(),
            deposited_lido_validators: usize_to_u64(deposited_indices.len()),
            exited_lido_validators: usize_to_u64(lido_validators_state.exited_lido_validator_indices.len()),
            lido_cl_balance: cl_balance,
        };
    }
}
