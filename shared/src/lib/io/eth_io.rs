use std::{
    fmt,
    ops::{Add, Sub},
};

use alloy_sol_types::sol;
use derivative::Derivative;
use ethereum_types::Address;
use serde::{Deserialize, Serialize};
use tree_hash::TreeHash;
use typenum::Unsigned;

use crate::{
    eth_consensus_layer::{BeaconBlockHeader, BeaconState, Epoch, Hash256, Slot},
    eth_spec,
    io::serde_utils::serde_hex_as_string,
};

use super::program_io::WithdrawalVaultData;

mod derivatives {
    use super::*;
    pub fn slice_as_hash(val: &[u8], f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "0x{}", hex::encode(val))
    }
}

pub mod conversions {
    use crate::eth_consensus_layer::Address;

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

    pub fn alloy_address_to_h160(value: alloy_primitives::Address) -> Address {
        let addr_bytes: [u8; 20] = value.into();
        addr_bytes.into()
    }

    pub fn h160_to_alloy_address(value: Address) -> alloy_primitives::Address {
        value.to_fixed_bytes().into()
    }
}

sol! {
    #[derive(Debug)]
    struct ReportSolidity {
        uint256 reference_slot;
        uint256 deposited_lido_validators;
        uint256 exited_lido_validators;
        uint256 lido_cl_valance;
        uint256 lido_withdrawal_vault_balance;
    }
}

#[derive(PartialEq, Eq, Clone, Copy, Debug, Serialize, Deserialize)]
pub struct ReferenceSlot(pub Slot);

impl Add<u64> for ReferenceSlot {
    type Output = Self;

    fn add(self, rhs: u64) -> Self::Output {
        Self(self.0 + rhs)
    }
}

impl Sub<u64> for ReferenceSlot {
    type Output = Self;

    fn sub(self, rhs: u64) -> Self::Output {
        Self(self.0 - rhs)
    }
}

impl fmt::Display for ReferenceSlot {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl From<ReferenceSlot> for alloy_primitives::U256 {
    fn from(value: ReferenceSlot) -> Self {
        conversions::u64_to_uint256(value.0)
    }
}

impl TreeHash for ReferenceSlot {
    fn tree_hash_type() -> tree_hash::TreeHashType {
        tree_hash::TreeHashType::Basic
    }

    fn tree_hash_packed_encoding(&self) -> tree_hash::PackedEncoding {
        self.0.tree_hash_packed_encoding()
    }

    fn tree_hash_packing_factor() -> usize {
        tree_hash::HASHSIZE / 8
    }

    #[allow(clippy::cast_lossless)] // Lint does not apply to all uses of this macro.
    fn tree_hash_root(&self) -> Hash256 {
        self.0.tree_hash_root()
    }
}

#[derive(PartialEq, Eq, Clone, Copy, Hash, Debug, Serialize, Deserialize)]
pub struct BeaconChainSlot(pub Slot);

impl Add<u64> for BeaconChainSlot {
    type Output = Self;

    fn add(self, rhs: u64) -> Self::Output {
        Self(self.0 + rhs)
    }
}

impl Sub<u64> for BeaconChainSlot {
    type Output = Self;

    fn sub(self, rhs: u64) -> Self::Output {
        Self(self.0 - rhs)
    }
}

impl fmt::Display for BeaconChainSlot {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl From<BeaconChainSlot> for alloy_primitives::U256 {
    fn from(value: BeaconChainSlot) -> Self {
        conversions::u64_to_uint256(value.0)
    }
}

impl TreeHash for BeaconChainSlot {
    fn tree_hash_type() -> tree_hash::TreeHashType {
        tree_hash::TreeHashType::Basic
    }

    fn tree_hash_packed_encoding(&self) -> tree_hash::PackedEncoding {
        self.0.tree_hash_packed_encoding()
    }

    fn tree_hash_packing_factor() -> usize {
        tree_hash::HASHSIZE / 8
    }

    #[allow(clippy::cast_lossless)] // Lint does not apply to all uses of this macro.
    fn tree_hash_root(&self) -> Hash256 {
        self.0.tree_hash_root()
    }
}

pub trait HaveSlotWithBlock {
    fn bc_slot(&self) -> BeaconChainSlot;
}

impl HaveSlotWithBlock for BeaconState {
    fn bc_slot(&self) -> BeaconChainSlot {
        BeaconChainSlot(self.slot)
    }
}

impl HaveSlotWithBlock for BeaconBlockHeader {
    fn bc_slot(&self) -> BeaconChainSlot {
        BeaconChainSlot(self.slot)
    }
}

pub trait HaveEpoch {
    fn epoch(&self) -> Epoch;
}

impl HaveEpoch for BeaconChainSlot {
    fn epoch(&self) -> Epoch {
        self.0 / eth_spec::SlotsPerEpoch::to_u64()
    }
}

impl HaveEpoch for ReferenceSlot {
    fn epoch(&self) -> Epoch {
        self.0 / eth_spec::SlotsPerEpoch::to_u64()
    }
}

impl HaveEpoch for BeaconState {
    fn epoch(&self) -> Epoch {
        self.slot / eth_spec::SlotsPerEpoch::to_u64()
    }
}

impl HaveEpoch for BeaconBlockHeader {
    fn epoch(&self) -> Epoch {
        self.slot / eth_spec::SlotsPerEpoch::to_u64()
    }
}

#[derive(PartialEq, Eq, Clone, Debug, Serialize, Deserialize)]
pub struct ReportRust {
    pub reference_slot: ReferenceSlot,
    pub deposited_lido_validators: u64,
    pub exited_lido_validators: u64,
    pub lido_cl_balance: u64,
    pub lido_withdrawal_vault_balance: alloy_primitives::U256,
}

impl From<ReportSolidity> for ReportRust {
    fn from(value: ReportSolidity) -> Self {
        Self {
            reference_slot: ReferenceSlot(conversions::uint256_to_u64(value.reference_slot)),
            deposited_lido_validators: conversions::uint256_to_u64(value.deposited_lido_validators),
            exited_lido_validators: conversions::uint256_to_u64(value.exited_lido_validators),
            lido_cl_balance: conversions::uint256_to_u64(value.lido_cl_valance),
            lido_withdrawal_vault_balance: value.lido_withdrawal_vault_balance,
        }
    }
}

impl From<ReportRust> for ReportSolidity {
    fn from(value: ReportRust) -> Self {
        Self {
            reference_slot: conversions::u64_to_uint256(value.reference_slot.0),
            deposited_lido_validators: conversions::u64_to_uint256(value.deposited_lido_validators),
            exited_lido_validators: conversions::u64_to_uint256(value.exited_lido_validators),
            lido_cl_valance: conversions::u64_to_uint256(value.lido_cl_balance),
            lido_withdrawal_vault_balance: value.lido_withdrawal_vault_balance,
        }
    }
}

sol! {
    #[derive(Debug)]
    struct LidoValidatorStateSolidity {
        uint256 slot;
        bytes32 merkle_root;
    }
}

impl From<LidoValidatorStateSolidity> for LidoValidatorStateRust {
    fn from(value: LidoValidatorStateSolidity) -> Self {
        Self {
            slot: BeaconChainSlot(conversions::uint256_to_u64(value.slot)),
            merkle_root: value.merkle_root.into(),
        }
    }
}

impl From<LidoValidatorStateRust> for LidoValidatorStateSolidity {
    fn from(value: LidoValidatorStateRust) -> Self {
        Self {
            slot: value.slot.into(),
            merkle_root: value.merkle_root.into(),
        }
    }
}

#[derive(Derivative, PartialEq, Eq, Clone, Serialize, Deserialize)]
#[derivative(Debug)]
pub struct LidoValidatorStateRust {
    pub slot: BeaconChainSlot,
    #[serde(with = "serde_hex_as_string::FixedHexStringProtocol::<32>")]
    #[derivative(Debug(format_with = "derivatives::slice_as_hash"))]
    pub merkle_root: [u8; 32],
}

sol! {
    #[derive(Debug)]
    struct LidoWithdrawalVaultDataSolidity {
        uint256 balance;
        address vault_address;
    }
}

#[derive(PartialEq, Eq, Clone, Debug, Serialize, Deserialize)]
pub struct LidoWithdrawalVaultDataRust {
    pub vault_address: Address,
    pub balance: alloy_primitives::U256,
}

impl From<LidoWithdrawalVaultDataSolidity> for LidoWithdrawalVaultDataRust {
    fn from(value: LidoWithdrawalVaultDataSolidity) -> Self {
        Self {
            vault_address: conversions::alloy_address_to_h160(value.vault_address),
            balance: value.balance,
        }
    }
}

impl From<LidoWithdrawalVaultDataRust> for LidoWithdrawalVaultDataSolidity {
    fn from(value: LidoWithdrawalVaultDataRust) -> Self {
        Self {
            vault_address: conversions::h160_to_alloy_address(value.vault_address),
            balance: value.balance,
        }
    }
}

impl From<WithdrawalVaultData> for LidoWithdrawalVaultDataRust {
    fn from(value: WithdrawalVaultData) -> Self {
        Self {
            vault_address: value.vault_address,
            balance: value.balance,
        }
    }
}

sol! {
    #[derive(Debug)]
    struct ReportMetadataSolidity {
        uint256 bc_slot;
        uint256 epoch;
        bytes32 lido_withdrawal_credentials;
        bytes32 beacon_block_hash;
        LidoValidatorStateSolidity state_for_previous_report;
        LidoValidatorStateSolidity new_state;
        LidoWithdrawalVaultDataSolidity withdrawal_vault_data;
    }
}

#[derive(Derivative, PartialEq, Eq, Clone, Serialize, Deserialize)]
#[derivative(Debug)]
pub struct ReportMetadataRust {
    pub bc_slot: BeaconChainSlot,
    pub epoch: u64,
    #[serde(with = "serde_hex_as_string::FixedHexStringProtocol::<32>")]
    #[derivative(Debug(format_with = "derivatives::slice_as_hash"))]
    pub lido_withdrawal_credentials: [u8; 32],
    #[serde(with = "serde_hex_as_string::FixedHexStringProtocol::<32>")]
    #[derivative(Debug(format_with = "derivatives::slice_as_hash"))]
    pub beacon_block_hash: [u8; 32],
    pub state_for_previous_report: LidoValidatorStateRust,
    pub new_state: LidoValidatorStateRust,
    pub withdrawal_vault_data: LidoWithdrawalVaultDataRust,
}

impl From<ReportMetadataSolidity> for ReportMetadataRust {
    fn from(value: ReportMetadataSolidity) -> Self {
        Self {
            bc_slot: BeaconChainSlot(conversions::uint256_to_u64(value.bc_slot)),
            epoch: conversions::uint256_to_u64(value.epoch),
            lido_withdrawal_credentials: value.lido_withdrawal_credentials.into(),
            beacon_block_hash: value.beacon_block_hash.into(),
            state_for_previous_report: value.state_for_previous_report.into(),
            new_state: value.new_state.into(),
            withdrawal_vault_data: value.withdrawal_vault_data.into(),
        }
    }
}

impl From<ReportMetadataRust> for ReportMetadataSolidity {
    fn from(value: ReportMetadataRust) -> Self {
        Self {
            bc_slot: conversions::u64_to_uint256(value.bc_slot.0),
            epoch: conversions::u64_to_uint256(value.epoch),
            lido_withdrawal_credentials: value.lido_withdrawal_credentials.into(),
            beacon_block_hash: value.beacon_block_hash.into(),
            state_for_previous_report: value.state_for_previous_report.into(),
            new_state: value.new_state.into(),
            withdrawal_vault_data: value.withdrawal_vault_data.into(),
        }
    }
}

sol! {
    #[derive(Debug)]
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
