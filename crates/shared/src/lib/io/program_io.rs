use serde::{Deserialize, Serialize};

use crate::{
    eth_consensus_layer::{
        Address, Balances, BeaconBlockHeaderPrecomputedHashes, BeaconStatePrecomputedHashes, ExecutionPayloadHeader,
        ExecutionPayloadHeaderFields, Hash256,
    },
    io::eth_io::{BeaconChainSlot, ReferenceSlot},
    io::serde_utils::serde_hex_as_string,
    lido::{Error, LidoValidatorState, ValidatorDelta},
    merkle_proof::FieldProof,
};

#[derive(PartialEq, Debug, Clone, Serialize, Deserialize)]
pub struct ProgramInput {
    pub reference_slot: ReferenceSlot,
    pub bc_slot: BeaconChainSlot,
    pub beacon_block_hash: Hash256,
    pub beacon_block_header: BeaconBlockHeaderPrecomputedHashes,
    pub beacon_state: BeaconStatePrecomputedHashes,
    // Technically this could've been done by passing the execution payload header as a full structure
    // on beacon state (instead of a precomputed hash), but this requires more significant refactoring
    // and makes code handling proving beacon state somewhat harder. So passing one additional field we
    // actually need + inclusion proof as a separate field is a simpler approach.
    pub latest_execution_header_data: ExecutionPayloadHeaderData,
    pub validators_and_balances: ValsAndBals,
    pub old_lido_validator_state: LidoValidatorState,
    pub new_lido_validator_state_hash: Hash256,
    pub withdrawal_vault_data: WithdrawalVaultData,
}

impl ProgramInput {
    pub fn compute_new_state(&self) -> Result<LidoValidatorState, Error> {
        self.old_lido_validator_state.merge_validator_delta(
            self.bc_slot,
            &self.validators_and_balances.validators_delta,
            &self.validators_and_balances.lido_withdrawal_credentials,
        )
    }
}

#[derive(PartialEq, Debug, Clone, Serialize, Deserialize)]
pub struct ValsAndBals {
    pub lido_withdrawal_credentials: Hash256,

    pub balances: Balances, // all balances

    // Caveat: for now we can get away with verifying total_validators
    // passing ALL balances - since balances.len() == validators.len()
    // If we can move away from passing all balances to passing only relevant
    // ones, this verification won't hold anymore.
    pub total_validators: u64,

    pub validators_delta: ValidatorDelta,
    pub added_validators_inclusion_proof: Vec<u8>,
    pub changed_validators_inclusion_proof: Vec<u8>,
}

#[derive(PartialEq, Debug, Clone, Serialize, Deserialize)]
pub struct WithdrawalVaultData {
    #[serde(with = "serde_hex_as_string::VecOfHexStringProtocol")]
    pub account_proof: Vec<Vec<u8>>,
    pub balance: alloy_primitives::U256,
    pub vault_address: Address,
}

#[derive(PartialEq, Debug, Clone, Serialize, Deserialize)]
pub struct ExecutionPayloadHeaderData {
    pub state_root: Hash256,
    pub state_root_inclusion_proof: Vec<u8>,
}

impl ExecutionPayloadHeaderData {
    pub fn new(exec_payload: &ExecutionPayloadHeader) -> ExecutionPayloadHeaderData {
        Self {
            state_root: exec_payload.state_root,
            state_root_inclusion_proof: exec_payload
                .get_serialized_multiproof(&[ExecutionPayloadHeaderFields::state_root]),
        }
    }
}

impl From<&ExecutionPayloadHeader> for ExecutionPayloadHeaderData {
    fn from(exec_payload: &ExecutionPayloadHeader) -> Self {
        Self::new(exec_payload)
    }
}
