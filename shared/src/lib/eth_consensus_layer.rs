use ethereum_types::{H160, H256, U256};
use serde::{Deserialize, Serialize};
use ssz_derive::{Decode, Encode};
pub use ssz_types::{typenum, typenum::Unsigned, BitList, BitVector, FixedVector, VariableList};

use tree_hash::TreeHash;
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
    pub slot: Slot,
    pub proposer_index: CommitteeIndex,
    pub parent_root: Root,
    pub state_root: Root,
    pub body_root: Root,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Encode, Decode, TreeHash)]
pub struct Eth1Data {
    deposit_root: Root,
    // #[serde(with = "serde_utils::quoted_u64")]
    deposit_count: u64,
    block_hash: Hash256,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Encode, Decode, TreeHash)]
pub struct Validator {
    pub pubkey: BlsPublicKey,
    pub withdrawal_credentials: Hash256,
    // #[serde(with = "serde_utils::quoted_u64")]
    pub effective_balance: u64,
    pub slashed: bool,
    // #[serde(with = "serde_utils::quoted_u64")]
    pub activation_eligibility_epoch: Epoch,
    // #[serde(with = "serde_utils::quoted_u64")]
    pub activation_epoch: Epoch,
    // #[serde(with = "serde_utils::quoted_u64")]
    pub exit_epoch: Epoch,
    // #[serde(with = "serde_utils::quoted_u64")]
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
    // #[serde(with = "serde_utils::quoted_u64")]
    block_number: u64,
    // #[serde(with = "serde_utils::quoted_u64")]
    gas_limit: u64,
    // #[serde(with = "serde_utils::quoted_u64")]
    gas_used: u64,
    // #[serde(with = "serde_utils::quoted_u64")]
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
    // #[serde(with = "serde_utils::quoted_u64")]
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
    // #[serde(with = "serde_utils::quoted_u64")]
    pub eth1_deposit_index: u64,

    // Registry
    pub validators: Validators,
    // #[serde(with = "ssz_types::serde_utils::quoted_u64_var_list")]
    pub balances: Balances,

    // Randomness
    pub randao_mixes: FixedVector<Hash256, eth_spec::EpochsPerHistoricalVector>,

    // Slashings
    // #[serde(with = "ssz_types::serde_utils::quoted_u64_fixed_vec")]
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
    // #[serde(with = "ssz_types::serde_utils::quoted_u64_var_list")]
    pub inactivity_scores: VariableList<u64, eth_spec::ValidatorRegistryLimit>,

    // Light-client sync committees
    pub current_sync_committee: SyncCommittee,
    pub next_sync_committee: SyncCommittee,

    // Execution
    pub latest_execution_payload_header: ExecutionPayloadHeader,

    // Capella
    // #[serde(with = "serde_utils::quoted_u64")]
    pub next_withdrawal_index: u64,
    // #[serde(with = "serde_utils::quoted_u64")]
    pub next_withdrawal_validator_index: u64,
    // Deep history valid from Capella onwards.
    pub historical_summaries: VariableList<HistoricalSummary, eth_spec::HistoricalRootsLimit>,
}

// TODO: Derive?
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Encode, Decode, TreeHash)]
pub struct BeaconStatePrecomputedHashes {
    // Versioning
    pub genesis_time: Hash256,
    pub genesis_validators_root: Hash256,
    pub slot: Hash256,
    pub fork: Hash256,

    // History
    pub latest_block_header: Hash256,
    pub block_roots: Hash256,
    pub state_roots: Hash256,
    // Frozen in Capella, replaced by historical_summaries
    pub historical_roots: Hash256,

    // Ethereum 1.0 chain data
    pub eth1_data: Hash256,
    pub eth1_data_votes: Hash256,
    pub eth1_deposit_index: Hash256,

    // Registry
    pub validators: Hash256,
    pub balances: Hash256,

    // Randomness
    pub randao_mixes: Hash256,

    // Slashings
    pub slashings: Hash256,

    // Participation (Altair and later)
    pub previous_epoch_participation: Hash256,
    pub current_epoch_participation: Hash256,

    // Finality
    pub justification_bits: Hash256,
    pub previous_justified_checkpoint: Hash256,
    pub current_justified_checkpoint: Hash256,
    pub finalized_checkpoint: Hash256,

    // Inactivity
    pub inactivity_scores: Hash256,

    // Light-client sync committees
    pub current_sync_committee: Hash256,
    pub next_sync_committee: Hash256,

    // Execution
    pub latest_execution_payload_header: Hash256,

    // Capella
    pub next_withdrawal_index: Hash256,
    pub next_withdrawal_validator_index: Hash256,
    // Deep history valid from Capella onwards.
    pub historical_summaries: Hash256,
}

impl From<&BeaconState> for BeaconStatePrecomputedHashes {
    fn from(value: &BeaconState) -> Self {
        Self {
            genesis_time: value.genesis_time.tree_hash_root(),
            genesis_validators_root: value.genesis_validators_root.tree_hash_root(),
            slot: value.slot.tree_hash_root(),
            fork: value.fork.tree_hash_root(),
            latest_block_header: value.latest_block_header.tree_hash_root(),
            block_roots: value.block_roots.tree_hash_root(),
            state_roots: value.state_roots.tree_hash_root(),
            historical_roots: value.historical_roots.tree_hash_root(),
            eth1_data: value.eth1_data.tree_hash_root(),
            eth1_data_votes: value.eth1_data_votes.tree_hash_root(),
            eth1_deposit_index: value.eth1_deposit_index.tree_hash_root(),
            validators: value.validators.tree_hash_root(),
            balances: value.balances.tree_hash_root(),
            randao_mixes: value.randao_mixes.tree_hash_root(),
            slashings: value.slashings.tree_hash_root(),
            previous_epoch_participation: value.previous_epoch_participation.tree_hash_root(),
            current_epoch_participation: value.current_epoch_participation.tree_hash_root(),
            justification_bits: value.justification_bits.tree_hash_root(),
            previous_justified_checkpoint: value.previous_justified_checkpoint.tree_hash_root(),
            current_justified_checkpoint: value.current_justified_checkpoint.tree_hash_root(),
            finalized_checkpoint: value.finalized_checkpoint.tree_hash_root(),
            inactivity_scores: value.inactivity_scores.tree_hash_root(),
            current_sync_committee: value.current_sync_committee.tree_hash_root(),
            next_sync_committee: value.next_sync_committee.tree_hash_root(),
            latest_execution_payload_header: value.latest_execution_payload_header.tree_hash_root(),
            next_withdrawal_index: value.next_withdrawal_index.tree_hash_root(),
            next_withdrawal_validator_index: value.next_withdrawal_validator_index.tree_hash_root(),
            historical_summaries: value.historical_summaries.tree_hash_root(),
        }
    }
}

impl From<BeaconState> for BeaconStatePrecomputedHashes {
    fn from(value: BeaconState) -> Self {
        let borrowed: &BeaconState = &value;
        borrowed.into()
    }
}

// TODO: Derive?
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Encode, Decode, TreeHash)]
pub struct BeaconBlockHeaderPrecomputedHashes {
    pub slot: Hash256,
    pub proposer_index: Hash256,
    pub parent_root: Hash256,
    pub state_root: Hash256,
    pub body_root: Hash256,
}

impl From<&BeaconBlockHeader> for BeaconBlockHeaderPrecomputedHashes {
    fn from(value: &BeaconBlockHeader) -> Self {
        Self {
            slot: value.slot.tree_hash_root(),
            proposer_index: value.proposer_index.tree_hash_root(),
            parent_root: value.parent_root,
            state_root: value.state_root,
            body_root: value.body_root,
        }
    }
}

impl From<BeaconBlockHeader> for BeaconBlockHeaderPrecomputedHashes {
    fn from(value: BeaconBlockHeader) -> Self {
        let borrowed: &BeaconBlockHeader = &value;
        borrowed.into()
    }
}
