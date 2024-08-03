use ethereum_types::{H160, H256, U256};
use serde::{Deserialize, Serialize};
use ssz_derive::{Decode, Encode};
pub use ssz_types::{typenum, typenum::Unsigned, BitList, BitVector, FixedVector, VariableList};

use tree_hash_derive::TreeHash;

pub type Address = H160;
pub type CommitteeIndex = u64;
pub type Hash256 = H256;
pub type Root = Hash256;
pub type BlsPublicKey = FixedVector<u8, typenum::U48>;
pub type ForkVersion = FixedVector<u8, typenum::U4>;
pub type Version = FixedVector<u8, typenum::U4>;
pub type ParticipationFlags = u8;

use crate::eth_spec;

type Slot = u64;
type Epoch = u64;

// Re-export
pub type SlotsPerEpoch = eth_spec::SlotsPerEpoch;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Encode, Decode, TreeHash)]
pub struct Fork {
    previous_version: Version,
    current_version: Version,
    epoch: Epoch,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Encode, Decode, TreeHash)]
pub struct Checkpoint {
    epoch: Epoch,
    root: Root,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Encode, Decode, TreeHash)]
pub struct BeaconBlockHeader {
    slot: Slot,
    proposer_index: CommitteeIndex,
    parent_root: Root,
    state_root: Root,
    body_root: Root,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Encode, Decode, TreeHash)]
pub struct Eth1Data {
    deposit_root: Root,
    #[serde(with = "serde_utils::quoted_u64")]
    deposit_count: u64,
    block_hash: Hash256,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Encode, Decode, TreeHash)]
pub struct Validator {
    pub pubkey: BlsPublicKey,
    pub withdrawal_credentials: Hash256,
    #[serde(with = "serde_utils::quoted_u64")]
    pub effective_balance: u64,
    pub slashed: bool,
    pub activation_eligibility_epoch: Epoch,
    pub activation_epoch: Epoch,
    pub exit_epoch: Epoch,
    pub withdrawable_epoch: Epoch,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Encode, Decode, TreeHash)]
pub struct AttestationData {
    pub slot: Slot,
    pub index: CommitteeIndex,
    pub beacon_block_root: Root,
    pub source: Checkpoint,
    pub target: Checkpoint,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Encode, Decode, TreeHash)]
pub struct PendingAttestation {
    aggregation_bits: BitList<eth_spec::MaxValidatorsPerCommittee>,
    data: AttestationData,
    inclusion_delay: Slot,
    proposer_index: CommitteeIndex,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Encode, Decode, TreeHash)]
pub struct SyncCommittee {
    pubkeys: FixedVector<BlsPublicKey, eth_spec::SyncCommitteeSize>,
    aggregate_pubkey: BlsPublicKey,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Encode, Decode, TreeHash)]
pub struct ExecutionPayloadHeader {
    parent_hash: Hash256,
    fee_recipient: Address,
    state_root: Root,
    receipts_root: Root,
    logs_bloom: FixedVector<u8, eth_spec::BytesPerLogBloom>,
    prev_randao: Hash256,
    #[serde(with = "serde_utils::quoted_u64")]
    block_number: u64,
    #[serde(with = "serde_utils::quoted_u64")]
    gas_limit: u64,
    #[serde(with = "serde_utils::quoted_u64")]
    gas_used: u64,
    #[serde(with = "serde_utils::quoted_u64")]
    timestamp: u64,
    extra_data: VariableList<u8, eth_spec::MaxExtraDataBytes>,
    // workaround - looks like ByteList is partially broken, but extra data is exactly bytes32
    // extra_data: Hash256
    base_fee_per_gas: U256,
    block_hash: Hash256,
    transactions_root: Root,
    withdrawals_root: Root,
    // excess_data_gas: Uint256
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Encode, Decode, TreeHash)]
pub struct HistoricalSummary {
    block_summary_root: Root,
    state_summary_root: Root,
}

pub type Validators = VariableList<Validator, eth_spec::ValidatorRegistryLimit>;
pub type Balances = VariableList<u64, eth_spec::ValidatorRegistryLimit>;

// Simplified https://github.com/sigp/lighthouse/blob/master/consensus/types/src/beacon_state.rs#L212
// Primarily - flattening the "superstruct" part on different eth specs,
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Encode, Decode, TreeHash)]
pub struct BeaconState {
    // Versioning
    #[serde(with = "serde_utils::quoted_u64")]
    pub genesis_time: u64,
    pub genesis_validators_root: Hash256,
    pub slot: Slot,
    pub fork: Fork,

    // History
    pub latest_block_header: BeaconBlockHeader,
    pub block_roots: FixedVector<Hash256, eth_spec::SlotsPerHistoricalRoot>,
    pub state_roots: FixedVector<Hash256, eth_spec::SlotsPerHistoricalRoot>,
    // Frozen in Capella, replaced by historical_summaries
    pub historical_roots: VariableList<Hash256, eth_spec::HistoricalRootsLimit>,

    // Ethereum 1.0 chain data
    pub eth1_data: Eth1Data,
    pub eth1_data_votes: VariableList<Eth1Data, eth_spec::SlotsPerEth1VotingPeriod>,
    #[serde(with = "serde_utils::quoted_u64")]
    pub eth1_deposit_index: u64,

    // Registry
    pub validators: Validators,
    #[serde(with = "ssz_types::serde_utils::quoted_u64_var_list")]
    pub balances: Balances,

    // Randomness
    pub randao_mixes: FixedVector<Hash256, eth_spec::EpochsPerHistoricalVector>,

    // Slashings
    #[serde(with = "ssz_types::serde_utils::quoted_u64_fixed_vec")]
    pub slashings: FixedVector<u64, eth_spec::EpochsPerSlashingsVector>,

    // Participation (Altair and later)
    pub previous_epoch_participation: VariableList<ParticipationFlags, eth_spec::ValidatorRegistryLimit>,
    pub current_epoch_participation: VariableList<ParticipationFlags, eth_spec::ValidatorRegistryLimit>,

    // Finality
    pub justification_bits: BitVector<eth_spec::JustificationBitsLength>,
    pub previous_justified_checkpoint: Checkpoint,
    pub current_justified_checkpoint: Checkpoint,
    pub finalized_checkpoint: Checkpoint,

    // Inactivity
    #[serde(with = "ssz_types::serde_utils::quoted_u64_var_list")]
    pub inactivity_scores: VariableList<u64, eth_spec::ValidatorRegistryLimit>,

    // Light-client sync committees
    pub current_sync_committee: SyncCommittee,
    pub next_sync_committee: SyncCommittee,

    // Execution
    pub latest_execution_payload_header: ExecutionPayloadHeader,

    // Capella
    #[serde(with = "serde_utils::quoted_u64")]
    pub next_withdrawal_index: u64,
    #[serde(with = "serde_utils::quoted_u64")]
    pub next_withdrawal_validator_index: u64,
    // Deep history valid from Capella onwards.
    pub historical_summaries: VariableList<HistoricalSummary, eth_spec::HistoricalRootsLimit>,
}
