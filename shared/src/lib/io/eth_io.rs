use alloy_sol_types::sol;
use serde::{Deserialize, Serialize};

mod serde_hex_as_string {
    use serde::de::Error;
    use serde::{Deserialize, Deserializer, Serializer};

    pub struct HexStringProtocol<const N: usize> {}

    impl<const N: usize> HexStringProtocol<N> {
        pub fn serialize<'a, S>(value: &'a [u8], serializer: S) -> Result<S::Ok, S::Error>
        where
            S: Serializer,
        {
            let res = format!("0x{}", hex::encode(value));
            serializer.serialize_str(&res)
        }

        pub fn deserialize<'de, D>(deserializer: D) -> Result<[u8; N], D::Error>
        where
            D: Deserializer<'de>,
        {
            let s: &str = Deserialize::deserialize(deserializer)?;
            let mut slice: [u8; N] = [0; N];
            hex::decode_to_slice(s, &mut slice).map_err(Error::custom)?;
            Ok(slice)
        }
    }
}

sol! {
    struct ReportSolidity {
        uint64 slot;
        uint64 deposited_lido_validators;
        uint64 exited_lido_validators;
        uint64 lido_cl_valance;
    }
}

#[derive(PartialEq, Eq, Clone, Debug, Serialize, Deserialize)]
pub struct ReportRust {
    pub slot: u64,
    pub deposited_lido_validators: u64,
    pub exited_lido_validators: u64,
    pub lido_cl_valance: u64,
}

impl From<ReportSolidity> for ReportRust {
    fn from(value: ReportSolidity) -> Self {
        Self {
            slot: value.slot,
            deposited_lido_validators: value.deposited_lido_validators,
            exited_lido_validators: value.exited_lido_validators,
            lido_cl_valance: value.lido_cl_valance,
        }
    }
}

impl From<ReportRust> for ReportSolidity {
    fn from(value: ReportRust) -> Self {
        Self {
            slot: value.slot,
            deposited_lido_validators: value.deposited_lido_validators,
            exited_lido_validators: value.exited_lido_validators,
            lido_cl_valance: value.lido_cl_valance,
        }
    }
}

sol! {
    struct LidoValidatorStateSolidity {
        uint64 slot;
        bytes32 merkle_root;
    }
}

impl From<LidoValidatorStateSolidity> for LidoValidatorStateRust {
    fn from(value: LidoValidatorStateSolidity) -> Self {
        Self {
            slot: value.slot,
            merkle_root: value.merkle_root.into(),
        }
    }
}

impl From<LidoValidatorStateRust> for LidoValidatorStateSolidity {
    fn from(value: LidoValidatorStateRust) -> Self {
        Self {
            slot: value.slot,
            merkle_root: value.merkle_root.into(),
        }
    }
}

#[derive(PartialEq, Eq, Clone, Debug, Serialize, Deserialize)]
pub struct LidoValidatorStateRust {
    pub slot: u64,
    #[serde(with = "serde_hex_as_string::HexStringProtocol::<32>")]
    pub merkle_root: [u8; 32],
}

sol! {
    struct ReportMetadataSolidity {
        uint64 slot;
        uint64 epoch;
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
    #[serde(with = "serde_hex_as_string::HexStringProtocol::<32>")]
    pub lido_withdrawal_credentials: [u8; 32],
    #[serde(with = "serde_hex_as_string::HexStringProtocol::<32>")]
    pub beacon_block_hash: [u8; 32],
    pub state_for_previous_report: LidoValidatorStateRust,
    pub new_state: LidoValidatorStateRust,
}

impl From<ReportMetadataSolidity> for ReportMetadataRust {
    fn from(value: ReportMetadataSolidity) -> Self {
        Self {
            slot: value.slot,
            epoch: value.epoch,
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
            slot: value.slot,
            epoch: value.epoch,
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

#[derive(PartialEq, Eq, Clone, Debug, Serialize, Deserialize)]
pub struct ContractDeployParametersRust {
    pub network: String,
    #[serde(with = "serde_hex_as_string::HexStringProtocol::<20>")]
    pub verifier: [u8; 20],
    pub vkey: String,
    #[serde(with = "serde_hex_as_string::HexStringProtocol::<32>")]
    pub withdrawal_credentials: [u8; 32],
    pub genesis_timestamp: u64,
    pub initial_validator_state: LidoValidatorStateRust,
}
