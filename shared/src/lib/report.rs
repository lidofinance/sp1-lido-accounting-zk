use crate::eth_consensus_layer::{Balances, Hash256, Validators};
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
            if validator.activation_eligibility_epoch >= epoch {
                deposited += 1;
            }
            if validator.exit_epoch <= epoch {
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
}
