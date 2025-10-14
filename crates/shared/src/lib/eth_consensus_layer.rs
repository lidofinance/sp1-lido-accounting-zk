use crate::merkle_proof::MerkleTreeFieldLeaves;

use alloy_primitives::U256;
use derive_more::Debug;
use serde::{Deserialize, Serialize};
use sp1_lido_accounting_zk_shared_merkle_tree_leaves_derive::MerkleTreeFieldLeaves;
use ssz_derive::{Decode, Encode};
pub use ssz_types::{typenum, typenum::Unsigned, BitList, BitVector, FixedVector, VariableList};

use tree_hash::TreeHash;
use tree_hash_derive::TreeHash;

pub type Address = alloy_primitives::Address;
pub type CommitteeIndex = u64;
pub type Hash256 = alloy_primitives::B256;
pub type Root = Hash256;
pub type BlsPublicKey = FixedVector<u8, typenum::U48>;
pub type BlsSignature = FixedVector<u8, typenum::U96>;
pub type ForkVersion = FixedVector<u8, typenum::U4>;
pub type Version = FixedVector<u8, typenum::U4>;
pub type ParticipationFlags = u8;

use crate::eth_spec;

pub type Slot = u64;
pub type Epoch = u64;
pub type ValidatorIndex = u64;
pub type WithdrawalCredentials = Hash256;
pub type Gwei = u64;

// Re-export
pub type SlotsPerEpoch = eth_spec::SlotsPerEpoch;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Encode, Decode, TreeHash)]
pub struct Fork {
    pub previous_version: Version,
    pub current_version: Version,
    pub epoch: Epoch,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Encode, Decode, TreeHash)]
pub struct Checkpoint {
    pub epoch: Epoch,
    pub root: Root,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Encode, Decode, TreeHash, MerkleTreeFieldLeaves)]
pub struct BeaconBlockHeader {
    pub slot: Slot,
    pub proposer_index: CommitteeIndex,
    pub parent_root: Root,
    pub state_root: Root,
    pub body_root: Root,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Encode, Decode, TreeHash)]
pub struct Eth1Data {
    pub deposit_root: Root,
    // #[serde(with = "serde_utils::quoted_u64")]
    pub deposit_count: u64,
    pub block_hash: Hash256,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Encode, Decode, TreeHash)]
pub struct Validator {
    #[debug("{:#?}", hex::encode(pubkey.to_vec()))]
    pub pubkey: BlsPublicKey,
    pub withdrawal_credentials: WithdrawalCredentials,
    // #[serde(with = "serde_utils::quoted_u64")]
    pub effective_balance: Gwei,
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
    pub aggregation_bits: BitList<eth_spec::MaxValidatorsPerCommittee>,
    pub data: AttestationData,
    pub inclusion_delay: Slot,
    pub proposer_index: CommitteeIndex,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Encode, Decode, TreeHash)]
pub struct SyncCommittee {
    pub pubkeys: FixedVector<BlsPublicKey, eth_spec::SyncCommitteeSize>,
    pub aggregate_pubkey: BlsPublicKey,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Encode, Decode, TreeHash, MerkleTreeFieldLeaves)]
pub struct ExecutionPayloadHeader {
    pub parent_hash: Hash256,
    pub fee_recipient: Address,
    pub state_root: Root,
    pub receipts_root: Root,
    #[debug("{:#?}", logs_bloom.to_vec())]
    pub logs_bloom: FixedVector<u8, eth_spec::BytesPerLogBloom>,
    pub prev_randao: Hash256,
    // #[serde(with = "serde_utils::quoted_u64")]
    pub block_number: u64,
    // #[serde(with = "serde_utils::quoted_u64")]
    pub gas_limit: u64,
    // #[serde(with = "serde_utils::quoted_u64")]
    pub gas_used: u64,
    // #[serde(with = "serde_utils::quoted_u64")]
    pub timestamp: u64,
    pub extra_data: VariableList<u8, eth_spec::MaxExtraDataBytes>,
    pub base_fee_per_gas: U256,
    pub block_hash: Hash256,
    pub transactions_root: Root,
    pub withdrawals_root: Root,
    // Since Deneb
    pub blob_gas_used: u64,
    pub excess_blob_gas: u64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Encode, Decode, TreeHash)]
pub struct HistoricalSummary {
    pub block_summary_root: Root,
    pub state_summary_root: Root,
}

pub type Validators = VariableList<Validator, eth_spec::ValidatorRegistryLimit>;
pub type Balances = VariableList<Gwei, eth_spec::ValidatorRegistryLimit>;
pub type JustificationBits = BitVector<eth_spec::JustificationBitsLength>;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Encode, Decode, TreeHash)]
pub struct PendingDeposit {
    pub pubkey: BlsPublicKey,
    pub withdrawal_credentials: WithdrawalCredentials,
    pub amount: Gwei,
    pub signature: BlsSignature,
    pub slot: Slot,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Encode, Decode, TreeHash)]
pub struct PendingPartialWithdrawal {
    pub validator_index: ValidatorIndex,
    pub amount: Gwei,
    pub withdrawable_epoch: Epoch,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Encode, Decode, TreeHash)]
pub struct PendingConsolidation {
    pub source_index: ValidatorIndex,
    pub target_index: ValidatorIndex,
}

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
    pub justification_bits: JustificationBits,
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

    // Electra
    // #[serde(with = "serde_utils::quoted_u64")]
    pub deposit_requests_start_index: u64,
    // #[serde(with = "serde_utils::quoted_u64")]
    pub deposit_balance_to_consume: Gwei,
    // #[serde(with = "serde_utils::quoted_u64")]
    pub exit_balance_to_consume: Gwei,
    pub earliest_exit_epoch: Epoch,
    // #[serde(with = "serde_utils::quoted_u64")]
    pub consolidation_balance_to_consume: Gwei,
    pub earliest_consolidation_epoch: Epoch,
    pub pending_deposits: VariableList<PendingDeposit, eth_spec::PendingDepositsLimit>,
    pub pending_partial_withdrawals: VariableList<PendingPartialWithdrawal, eth_spec::PendingPartialWithdrawalsLimit>,
    pub pending_consolidations: VariableList<PendingConsolidation, eth_spec::PendingConsolidationsLimit>,

    // Fulu
    pub proposer_lookahead:
        FixedVector<ValidatorIndex, typenum::Prod<typenum::Add1<eth_spec::MinSeedLookahead>, eth_spec::SlotsPerEpoch>>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Encode, Decode, TreeHash, MerkleTreeFieldLeaves)]
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
    // Electra
    pub deposit_requests_start_index: Hash256,
    // #[serde(with = "serde_utils::quoted_u64")]
    pub deposit_balance_to_consume: Hash256,
    // #[serde(with = "serde_utils::quoted_u64")]
    pub exit_balance_to_consume: Hash256,
    pub earliest_exit_epoch: Hash256,
    // #[serde(with = "serde_utils::quoted_u64")]
    pub consolidation_balance_to_consume: Hash256,
    pub earliest_consolidation_epoch: Hash256,
    pub pending_deposits: Hash256,
    pub pending_partial_withdrawals: Hash256,
    pub pending_consolidations: Hash256,
    // Fulu
    pub proposer_lookahead: Hash256,
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
            deposit_requests_start_index: value.deposit_requests_start_index.tree_hash_root(),
            deposit_balance_to_consume: value.deposit_balance_to_consume.tree_hash_root(),
            exit_balance_to_consume: value.exit_balance_to_consume.tree_hash_root(),
            earliest_exit_epoch: value.earliest_exit_epoch.tree_hash_root(),
            consolidation_balance_to_consume: value.consolidation_balance_to_consume.tree_hash_root(),
            earliest_consolidation_epoch: value.earliest_consolidation_epoch.tree_hash_root(),
            pending_deposits: value.pending_deposits.tree_hash_root(),
            pending_partial_withdrawals: value.pending_partial_withdrawals.tree_hash_root(),
            pending_consolidations: value.pending_consolidations.tree_hash_root(),
            proposer_lookahead: value.proposer_lookahead.tree_hash_root(),
        }
    }
}

impl From<BeaconState> for BeaconStatePrecomputedHashes {
    fn from(value: BeaconState) -> Self {
        let borrowed: &BeaconState = &value;
        borrowed.into()
    }
}

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

pub type BeaconStateFields = BeaconStatePrecomputedHashesFields;

impl MerkleTreeFieldLeaves for BeaconState {
    const FIELD_COUNT: usize = 28;
    type TFields = BeaconStateFields;
    fn get_leaf_index(field_name: &Self::TFields) -> usize {
        BeaconStatePrecomputedHashes::get_leaf_index(field_name)
    }

    fn get_fields(&self) -> Vec<Hash256> {
        let precomp: BeaconStatePrecomputedHashes = self.into();
        precomp.get_fields()
    }
}

#[cfg(test)]
pub mod test_utils {
    use arbitrary::Arbitrary;

    use super::*;

    fn saturated_add(a: u64, b: u64) -> u64 {
        match a.checked_add(b) {
            Some(val) => val,
            None => u64::MAX,
        }
    }

    fn arb_saturated_add(a: u64, try_b: arbitrary::Result<u64>) -> arbitrary::Result<u64> {
        let b = try_b?;
        Ok(saturated_add(a, b))
    }

    impl<'a> Arbitrary<'a> for Validator {
        fn arbitrary(u: &mut arbitrary::Unstructured<'a>) -> arbitrary::Result<Self> {
            let pubkey = BlsPublicKey::arbitrary(u)?;
            let withdrawal_credentials = Hash256::arbitrary(u)?;
            let effective_balance = u64::arbitrary(u)?;
            let slashed = bool::arbitrary(u)?;
            let activation_eligibility_epoch: u64 = u64::arbitrary(u)?;
            let activation_epoch: u64 = arb_saturated_add(activation_eligibility_epoch, u64::arbitrary(u))?;
            let exit_epoch: u64 = arb_saturated_add(activation_epoch, u64::arbitrary(u))?;
            let withdrawable_epoch: u64 = arb_saturated_add(activation_epoch, u64::arbitrary(u))?;
            let validator = Validator {
                pubkey,
                withdrawal_credentials,
                effective_balance,
                slashed,
                activation_eligibility_epoch,
                activation_epoch,
                exit_epoch,
                withdrawable_epoch,
            };
            Ok(validator)
        }
    }

    pub mod proptest_utils {
        use proptest::prelude::*;
        use proptest_arbitrary_interop::arb;

        use super::{Epoch, Validator};

        prop_compose! {
            fn arbitrary_validator()(val in arb::<Validator>()) -> Validator {
                val
            }
        }

        prop_compose! {
            pub fn pending_validator(current_epoch: Epoch)(val in arbitrary_validator(), eligibility in current_epoch..u64::MAX) -> Validator {
                let mut newval = val.clone();
                newval.activation_eligibility_epoch = eligibility;
                newval.activation_epoch = u64::MAX;
                newval.withdrawable_epoch = u64::MAX;
                newval.exit_epoch = u64::MAX;
                newval
            }
        }

        prop_compose! {
            pub fn deposited_validator(current_epoch: Epoch)(
                val in arbitrary_validator(),
                eligibility in 0..current_epoch,
                activation in current_epoch..u64::MAX
            ) -> Validator {
                let mut newval = val.clone();
                newval.activation_eligibility_epoch = eligibility;
                newval.activation_epoch = activation;
                newval.withdrawable_epoch = u64::MAX;
                newval.exit_epoch = u64::MAX;
                newval
            }
        }

        prop_compose! {
            pub fn activated_validator(current_epoch: Epoch)
                (val in deposited_validator(current_epoch))
                (activation in val.activation_eligibility_epoch..current_epoch, val in Just(val))
                 -> Validator {
                let mut newval = val.clone();
                newval.activation_epoch = activation;
                newval.withdrawable_epoch = activation;
                newval
            }
        }

        prop_compose! {
            pub fn withdrawable_validator(current_epoch: Epoch)
                (val in activated_validator(current_epoch))
                (withdrawable in val.activation_epoch..current_epoch, val in Just(val)) -> Validator {
                let mut newval = val.clone();
                newval.withdrawable_epoch = withdrawable;
                newval
            }
        }

        prop_compose! {
            pub fn exited_validator(current_epoch: Epoch)(
                val in activated_validator(current_epoch)
            )(
                exited in val.activation_epoch..current_epoch, val in Just(val)
            ) -> Validator {
                let mut newval = val.clone();
                newval.exit_epoch = exited;
                newval
            }
        }

        pub fn gen_validator(current_epoch: Epoch) -> impl Strategy<Value = Validator> {
            prop_oneof![
                pending_validator(current_epoch),
                deposited_validator(current_epoch),
                activated_validator(current_epoch),
                withdrawable_validator(current_epoch),
                exited_validator(current_epoch)
            ]
        }

        pub fn gen_epoch_and_validator() -> impl Strategy<Value = (Epoch, Validator)> {
            (arb::<Epoch>()).prop_flat_map(|epoch| (Just(epoch), gen_validator(epoch)))
        }
    }
}
