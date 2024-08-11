use alloy_sol_types::{sol, SolType};
use serde::{Deserialize, Serialize};
use ssz_derive::{Decode, Encode};

use crate::{
    eth_consensus_layer::{Balances, BeaconBlockHeaderPrecomputedHashes, BeaconStatePrecomputedHashes, Validators},
    report::ReportData,
};

#[derive(PartialEq, Clone, Debug, Serialize, Deserialize)]
pub struct ProgramInput {
    pub slot: u64,
    pub beacon_block_hash: [u8; 32],
    pub beacon_block_header: BeaconBlockHeaderPrecomputedHashes,
    pub beacon_state: BeaconStatePrecomputedHashes,
    pub validators_and_balances_proof: Vec<u8>,
    pub validators_and_balances: ValsAndBals,
}

#[derive(PartialEq, Clone, Debug, Serialize, Deserialize, Encode, Decode)]
pub struct ValsAndBals {
    // #[serde(with = "ssz_types::serde_utils::quoted_u64_var_list")]
    pub balances: Balances,
    pub validators: Validators,
}

#[derive(PartialEq, Eq, Clone, Debug)]
pub struct PublicValuesRust {
    pub slot: u64,
    pub beacon_block_hash: [u8; 32],
    pub report: ReportData,
}

sol! {
    struct ReportSolidity {
        uint64 slot;
        uint64 epoch;
        bytes32 lido_withdrawal_credentials;
        uint64 all_lido_validators;
        uint64 exited_lido_validators;
        uint64 lido_cl_valance;
    }
}

sol! {
    struct PublicValuesSolidity {
        uint64 slot;
        bytes32 beacon_block_hash;
        ReportSolidity report;
    }
}

impl TryFrom<&[u8]> for PublicValuesRust {
    type Error = alloy_sol_types::Error;

    fn try_from(value: &[u8]) -> core::result::Result<Self, Self::Error> {
        let solidity_values: PublicValuesSolidity = PublicValuesSolidity::abi_decode(value, true)?;
        core::result::Result::Ok(Self {
            slot: solidity_values.slot,
            beacon_block_hash: solidity_values.beacon_block_hash.into(),
            report: solidity_values.report.into(),
        })
    }
}

impl From<ReportSolidity> for ReportData {
    fn from(value: ReportSolidity) -> Self {
        let withdrawal_creds: [u8; 32] = value.lido_withdrawal_credentials.into();
        Self {
            slot: value.slot,
            epoch: value.epoch,
            lido_withdrawal_credentials: withdrawal_creds.into(),
            all_lido_validators: value.all_lido_validators,
            exited_lido_validators: value.exited_lido_validators,
            lido_cl_valance: value.lido_cl_valance,
        }
    }
}
