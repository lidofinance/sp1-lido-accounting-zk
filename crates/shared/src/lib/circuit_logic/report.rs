use crate::{
    eth_consensus_layer::{Balances, Hash256, Validators},
    io::eth_io::ReferenceSlot,
    lido::LidoValidatorState,
    util::{u64_to_usize, usize_to_u64, ConversionError},
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

#[derive(thiserror::Error, Debug)]
pub enum Error {
    #[error("Failed to convert between numeric types")]
    ConversionError(#[from] ConversionError),
}

// Merge into ReportRust?
impl ReportData {
    pub fn compute(
        slot: ReferenceSlot,
        epoch: u64,
        validators: &Validators,
        balances: &Balances,
        lido_withdrawal_credentials: &Hash256,
    ) -> Self {
        let mut cl_balance: u64 = 0;
        let mut deposited: u64 = 0;
        let mut exited: u64 = 0;

        // make a clone to disentangle report lifetime from withdrawal credential lifetime
        let creds = *lido_withdrawal_credentials;

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
        reference_slot: ReferenceSlot,
        lido_validators_state: &LidoValidatorState,
        balances: &Balances,
        lido_withdrawal_credentials: &Hash256,
    ) -> Result<Self, Error> {
        let mut cl_balance: u64 = 0;

        let deposited_indices = &lido_validators_state.deposited_lido_validator_indices;

        for index in deposited_indices {
            cl_balance += balances[u64_to_usize(*index)?];
        }

        let res = Self {
            slot: reference_slot,
            epoch: lido_validators_state.epoch,
            lido_withdrawal_credentials: *lido_withdrawal_credentials,
            deposited_lido_validators: usize_to_u64(deposited_indices.len())?,
            exited_lido_validators: usize_to_u64(lido_validators_state.exited_lido_validator_indices.len())?,
            lido_cl_balance: cl_balance,
        };
        Ok(res)
    }
}
