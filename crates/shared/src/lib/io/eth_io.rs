use std::{
    fmt,
    ops::{Add, AddAssign, Sub, SubAssign},
};

use alloy_primitives::{
    ruint::{FromUintError, ToUintError},
    Address,
};
use alloy_sol_types::sol;
use derive_more::Debug;
use serde::{Deserialize, Serialize};
use tree_hash::TreeHash;
use typenum::Unsigned;

use crate::{
    eth_consensus_layer::{BeaconBlockHeader, BeaconState, Epoch, Hash256, Slot},
    eth_spec,
    io::serde_utils::serde_hex_as_string,
};
use thiserror::Error;

use super::program_io::WithdrawalVaultData;

#[derive(Debug, Error)]
pub enum Error {
    #[error("Conversion error: failed to convert {value} to uint256: {error:?}")]
    ToUint256Error {
        value: u64,
        error: ToUintError<alloy_primitives::U256>,
    },

    #[error("Conversion error: failed to convert {value} to u64: {error:?}")]
    FromUint256Error {
        value: alloy_primitives::U256,
        error: FromUintError<u64>,
    },
}

pub mod conversions {
    use super::Error;
    pub fn u64_to_uint256(value: u64) -> Result<alloy_primitives::U256, Error> {
        value.try_into().map_err(|error| Error::ToUint256Error { value, error })
    }

    pub fn uint256_to_u64(value: alloy_primitives::U256) -> Result<u64, Error> {
        value
            .try_into()
            .map_err(|error| Error::FromUint256Error { value, error })
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

impl AddAssign<u64> for ReferenceSlot {
    fn add_assign(&mut self, rhs: u64) {
        self.0 += rhs;
    }
}

impl SubAssign<u64> for ReferenceSlot {
    fn sub_assign(&mut self, rhs: u64) {
        self.0 -= rhs;
    }
}

impl fmt::Display for ReferenceSlot {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl TryFrom<alloy_primitives::U256> for ReferenceSlot {
    type Error = Error;

    fn try_from(value: alloy_primitives::U256) -> Result<Self, Self::Error> {
        let val = conversions::uint256_to_u64(value)?;
        Ok(ReferenceSlot(val))
    }
}

impl TryFrom<ReferenceSlot> for alloy_primitives::U256 {
    type Error = Error;

    fn try_from(value: ReferenceSlot) -> Result<Self, Self::Error> {
        let val = conversions::u64_to_uint256(value.0)?;
        Ok(val)
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

impl AddAssign<u64> for BeaconChainSlot {
    fn add_assign(&mut self, rhs: u64) {
        self.0 += rhs;
    }
}

impl Sub<u64> for BeaconChainSlot {
    type Output = Self;

    fn sub(self, rhs: u64) -> Self::Output {
        Self(self.0 - rhs)
    }
}

impl SubAssign<u64> for BeaconChainSlot {
    fn sub_assign(&mut self, rhs: u64) {
        self.0 -= rhs;
    }
}

impl fmt::Display for BeaconChainSlot {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl TryFrom<alloy_primitives::U256> for BeaconChainSlot {
    type Error = Error;

    fn try_from(value: alloy_primitives::U256) -> Result<Self, Self::Error> {
        let val = conversions::uint256_to_u64(value)?;
        Ok(BeaconChainSlot(val))
    }
}

impl TryFrom<BeaconChainSlot> for alloy_primitives::U256 {
    type Error = Error;

    fn try_from(value: BeaconChainSlot) -> Result<Self, Self::Error> {
        let val = conversions::u64_to_uint256(value.0)?;
        Ok(val)
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

impl TryFrom<ReportSolidity> for ReportRust {
    type Error = Error;

    fn try_from(value: ReportSolidity) -> Result<Self, Self::Error> {
        let result = Self {
            reference_slot: value.reference_slot.try_into()?,
            deposited_lido_validators: conversions::uint256_to_u64(value.deposited_lido_validators)?,
            exited_lido_validators: conversions::uint256_to_u64(value.exited_lido_validators)?,
            lido_cl_balance: conversions::uint256_to_u64(value.lido_cl_valance)?,
            lido_withdrawal_vault_balance: value.lido_withdrawal_vault_balance,
        };
        Ok(result)
    }
}

impl TryFrom<ReportRust> for ReportSolidity {
    type Error = Error;

    fn try_from(value: ReportRust) -> Result<Self, Self::Error> {
        let result = Self {
            reference_slot: conversions::u64_to_uint256(value.reference_slot.0)?,
            deposited_lido_validators: conversions::u64_to_uint256(value.deposited_lido_validators)?,
            exited_lido_validators: conversions::u64_to_uint256(value.exited_lido_validators)?,
            lido_cl_valance: conversions::u64_to_uint256(value.lido_cl_balance)?,
            lido_withdrawal_vault_balance: value.lido_withdrawal_vault_balance,
        };
        Ok(result)
    }
}

sol! {
    #[derive(Debug)]
    struct LidoValidatorStateSolidity {
        uint256 slot;
        bytes32 merkle_root;
    }
}

impl TryFrom<LidoValidatorStateSolidity> for LidoValidatorStateRust {
    type Error = Error;

    fn try_from(value: LidoValidatorStateSolidity) -> Result<Self, Self::Error> {
        let result = Self {
            slot: value.slot.try_into()?,
            merkle_root: value.merkle_root.into(),
        };
        Ok(result)
    }
}

impl TryFrom<LidoValidatorStateRust> for LidoValidatorStateSolidity {
    type Error = Error;

    fn try_from(value: LidoValidatorStateRust) -> Result<Self, Self::Error> {
        let result = Self {
            slot: value.slot.try_into()?,
            merkle_root: value.merkle_root.into(),
        };
        Ok(result)
    }
}

#[derive(Debug, PartialEq, Eq, Clone, Serialize, Deserialize)]
pub struct LidoValidatorStateRust {
    pub slot: BeaconChainSlot,
    #[serde(with = "serde_hex_as_string::FixedHexStringProtocol::<32>")]
    #[debug("{:#?}", hex::encode(merkle_root))]
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
            vault_address: value.vault_address,
            balance: value.balance,
        }
    }
}

impl From<LidoWithdrawalVaultDataRust> for LidoWithdrawalVaultDataSolidity {
    fn from(value: LidoWithdrawalVaultDataRust) -> Self {
        Self {
            vault_address: value.vault_address,
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

#[derive(Debug, PartialEq, Eq, Clone, Serialize, Deserialize)]
pub struct ReportMetadataRust {
    pub bc_slot: BeaconChainSlot,
    pub epoch: u64,
    #[serde(with = "serde_hex_as_string::FixedHexStringProtocol::<32>")]
    #[debug("{:#?}", hex::encode(lido_withdrawal_credentials))]
    pub lido_withdrawal_credentials: [u8; 32],
    #[serde(with = "serde_hex_as_string::FixedHexStringProtocol::<32>")]
    #[debug("{:#?}", hex::encode(beacon_block_hash))]
    pub beacon_block_hash: [u8; 32],
    pub state_for_previous_report: LidoValidatorStateRust,
    pub new_state: LidoValidatorStateRust,
    pub withdrawal_vault_data: LidoWithdrawalVaultDataRust,
}

impl TryFrom<ReportMetadataSolidity> for ReportMetadataRust {
    type Error = Error;

    fn try_from(value: ReportMetadataSolidity) -> Result<Self, Self::Error> {
        let result = Self {
            bc_slot: value.bc_slot.try_into()?,
            epoch: conversions::uint256_to_u64(value.epoch)?,
            lido_withdrawal_credentials: value.lido_withdrawal_credentials.into(),
            beacon_block_hash: value.beacon_block_hash.into(),
            state_for_previous_report: value.state_for_previous_report.try_into()?,
            new_state: value.new_state.try_into()?,
            withdrawal_vault_data: value.withdrawal_vault_data.into(),
        };
        Ok(result)
    }
}

impl TryFrom<ReportMetadataRust> for ReportMetadataSolidity {
    type Error = Error;

    fn try_from(value: ReportMetadataRust) -> Result<Self, Self::Error> {
        let result = Self {
            bc_slot: value.bc_slot.try_into()?,
            epoch: conversions::u64_to_uint256(value.epoch)?,
            lido_withdrawal_credentials: value.lido_withdrawal_credentials.into(),
            beacon_block_hash: value.beacon_block_hash.into(),
            state_for_previous_report: value.state_for_previous_report.try_into()?,
            new_state: value.new_state.try_into()?,
            withdrawal_vault_data: value.withdrawal_vault_data.into(),
        };
        Ok(result)
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

impl TryFrom<PublicValuesSolidity> for PublicValuesRust {
    type Error = Error;

    fn try_from(value: PublicValuesSolidity) -> Result<Self, Self::Error> {
        let result = Self {
            report: value.report.try_into()?,
            metadata: value.metadata.try_into()?,
        };
        Ok(result)
    }
}

impl TryFrom<PublicValuesRust> for PublicValuesSolidity {
    type Error = Error;

    fn try_from(value: PublicValuesRust) -> Result<Self, Self::Error> {
        let result = Self {
            report: value.report.try_into()?,
            metadata: value.metadata.try_into()?,
        };
        Ok(result)
    }
}
