use crate::eth_consensus_layer::{Balances, Hash256, Validators};
use serde::{Deserialize, Serialize};

#[derive(PartialEq, Eq, Clone, Debug, Serialize, Deserialize)]
pub struct ReportData {
    pub slot: u64,
    pub epoch: u64,
    pub lido_withdrawal_credentials: Hash256,
    pub all_lido_validators: u64,
    pub exited_lido_validators: u64,
    pub lido_cl_valance: u64,
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
        let mut active: u64 = 0;
        let mut exited: u64 = 0;

        // make a clone to disentangle report lifetime from withdrawal credential lifetime
        let creds = lido_withdrawal_credentials.clone();

        for (validator, balance) in validators.iter().zip(balances.iter()) {
            if validator.withdrawal_credentials != creds {
                continue;
            }

            cl_balance += *balance;
            active += 1;
            if validator.exit_epoch <= epoch {
                exited += 1
            }
        }
        Self {
            slot,
            epoch,
            lido_withdrawal_credentials: creds,
            all_lido_validators: active,
            exited_lido_validators: exited,
            lido_cl_valance: cl_balance,
        }
    }
}
