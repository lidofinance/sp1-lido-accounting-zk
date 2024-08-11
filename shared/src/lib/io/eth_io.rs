use alloy_sol_types::sol;
use serde::{Deserialize, Serialize};

sol! {
    struct ReportSolidity {
        uint64 slot;
        uint64 all_lido_validators;
        uint64 exited_lido_validators;
        uint64 lido_cl_valance;
    }
}

#[derive(PartialEq, Eq, Clone, Debug, Serialize, Deserialize)]
pub struct ReportRust {
    pub slot: u64,
    pub all_lido_validators: u64,
    pub exited_lido_validators: u64,
    pub lido_cl_valance: u64,
}

impl From<ReportSolidity> for ReportRust {
    fn from(value: ReportSolidity) -> Self {
        Self {
            slot: value.slot,
            all_lido_validators: value.all_lido_validators,
            exited_lido_validators: value.exited_lido_validators,
            lido_cl_valance: value.lido_cl_valance,
        }
    }
}

impl From<ReportRust> for ReportSolidity {
    fn from(value: ReportRust) -> Self {
        Self {
            slot: value.slot,
            all_lido_validators: value.all_lido_validators,
            exited_lido_validators: value.exited_lido_validators,
            lido_cl_valance: value.lido_cl_valance,
        }
    }
}

sol! {
    struct ReportMetadataSolidity {
        uint64 slot;
        uint64 epoch;
        bytes32 lido_withdrawal_credentials;
        bytes32 beacon_block_hash;
    }
}

#[derive(PartialEq, Eq, Clone, Debug, Serialize, Deserialize)]
pub struct ReportMetadataRust {
    pub slot: u64,
    pub epoch: u64,
    pub lido_withdrawal_credentials: [u8; 32],
    pub beacon_block_hash: [u8; 32],
}

impl From<ReportMetadataSolidity> for ReportMetadataRust {
    fn from(value: ReportMetadataSolidity) -> Self {
        Self {
            slot: value.slot,
            epoch: value.epoch,
            lido_withdrawal_credentials: value.lido_withdrawal_credentials.into(),
            beacon_block_hash: value.beacon_block_hash.into(),
        }
    }
}

impl From<ReportMetadataRust> for ReportMetadataSolidity {
    fn from(value: ReportMetadataRust) -> Self {
        Self {
            slot: value.slot,
            epoch: value.epoch,
            lido_withdrawal_credentials: value.lido_withdrawal_credentials.into(),
            beacon_block_hash: value.beacon_block_hash.into(),
        }
    }
}

sol! {
    struct PublicValuesSolidity {
        ReportSolidity report;
        ReportMetadataSolidity metadata;
    }
}

#[derive(PartialEq, Eq, Clone, Debug, Serialize, Deserialize)]
pub struct PublicValuesRust {
    pub report: ReportRust,
    pub metadata: ReportMetadataRust,
}

impl From<PublicValuesSolidity> for PublicValuesRust {
    fn from(value: PublicValuesSolidity) -> Self {
        Self {
            report: value.report.into(),
            metadata: value.metadata.into(),
        }
    }
}

impl From<PublicValuesRust> for PublicValuesSolidity {
    fn from(value: PublicValuesRust) -> Self {
        Self {
            report: value.report.into(),
            metadata: value.metadata.into(),
        }
    }
}
