use alloy_sol_types::sol;
use serde::{Deserialize, Serialize};

use crate::io::serde_utils::serde_hex_as_string;

pub mod conversions {
    pub fn u64_to_uint256(value: u64) -> alloy_primitives::U256 {
        value
            .try_into()
            .unwrap_or_else(|_| panic!("Failed to convert {} to u256", value))
    }

    pub fn uint256_to_u64(value: alloy_primitives::U256) -> u64 {
        value
            .try_into()
            .unwrap_or_else(|_| panic!("Failed to convert {} to u64", value))
    }
}

sol! {
    struct ReportSolidity {
        uint256 slot;
        uint256 deposited_lido_validators;
        uint256 exited_lido_validators;
        uint256 lido_cl_valance;
    }
}

#[derive(PartialEq, Eq, Clone, Debug, Serialize, Deserialize)]
pub struct ReportRust {
    pub slot: u64,
    pub deposited_lido_validators: u64,
    pub exited_lido_validators: u64,
    pub lido_cl_balance: u64,
}

impl From<ReportSolidity> for ReportRust {
    fn from(value: ReportSolidity) -> Self {
        Self {
            slot: conversions::uint256_to_u64(value.slot),
            deposited_lido_validators: conversions::uint256_to_u64(value.deposited_lido_validators),
            exited_lido_validators: conversions::uint256_to_u64(value.exited_lido_validators),
            lido_cl_balance: conversions::uint256_to_u64(value.lido_cl_valance),
        }
    }
}

impl From<ReportRust> for ReportSolidity {
    fn from(value: ReportRust) -> Self {
        Self {
            slot: conversions::u64_to_uint256(value.slot),
            deposited_lido_validators: conversions::u64_to_uint256(value.deposited_lido_validators),
            exited_lido_validators: conversions::u64_to_uint256(value.exited_lido_validators),
            lido_cl_valance: conversions::u64_to_uint256(value.lido_cl_balance),
        }
    }
}

sol! {
    struct LidoValidatorStateSolidity {
        uint256 slot;
        bytes32 merkle_root;
    }
}

impl From<LidoValidatorStateSolidity> for LidoValidatorStateRust {
    fn from(value: LidoValidatorStateSolidity) -> Self {
        Self {
            slot: value.slot.try_into().expect("Failed to convert uint256 to u64"),
            merkle_root: value.merkle_root.into(),
        }
    }
}

impl From<LidoValidatorStateRust> for LidoValidatorStateSolidity {
    fn from(value: LidoValidatorStateRust) -> Self {
        Self {
            slot: value.slot.try_into().expect("Failed to convert u64 to uint256"),
            merkle_root: value.merkle_root.into(),
        }
    }
}

#[derive(PartialEq, Eq, Clone, Debug, Serialize, Deserialize)]
pub struct LidoValidatorStateRust {
    pub slot: u64,
    #[serde(with = "serde_hex_as_string::FixedHexStringProtocol::<32>")]
    pub merkle_root: [u8; 32],
}

sol! {
    struct ReportMetadataSolidity {
        uint256 slot;
        uint256 epoch;
        bytes32 lido_withdrawal_credentials;
        bytes32 beacon_block_hash;
        LidoValidatorStateSolidity state_for_previous_report;
        LidoValidatorStateSolidity new_state;
    }
}

#[derive(PartialEq, Eq, Clone, Debug, Serialize, Deserialize)]
pub struct ReportMetadataRust {
    pub slot: u64,
    pub epoch: u64,
    #[serde(with = "serde_hex_as_string::FixedHexStringProtocol::<32>")]
    pub lido_withdrawal_credentials: [u8; 32],
    #[serde(with = "serde_hex_as_string::FixedHexStringProtocol::<32>")]
    pub beacon_block_hash: [u8; 32],
    pub state_for_previous_report: LidoValidatorStateRust,
    pub new_state: LidoValidatorStateRust,
}

impl From<ReportMetadataSolidity> for ReportMetadataRust {
    fn from(value: ReportMetadataSolidity) -> Self {
        Self {
            slot: conversions::uint256_to_u64(value.slot),
            epoch: conversions::uint256_to_u64(value.epoch),
            lido_withdrawal_credentials: value.lido_withdrawal_credentials.into(),
            beacon_block_hash: value.beacon_block_hash.into(),
            state_for_previous_report: value.state_for_previous_report.into(),
            new_state: value.new_state.into(),
        }
    }
}

impl From<ReportMetadataRust> for ReportMetadataSolidity {
    fn from(value: ReportMetadataRust) -> Self {
        Self {
            slot: conversions::u64_to_uint256(value.slot),
            epoch: conversions::u64_to_uint256(value.epoch),
            lido_withdrawal_credentials: value.lido_withdrawal_credentials.into(),
            beacon_block_hash: value.beacon_block_hash.into(),
            state_for_previous_report: value.state_for_previous_report.into(),
            new_state: value.new_state.into(),
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
