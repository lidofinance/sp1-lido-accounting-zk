from dataclasses import dataclass

from hexbytes import HexBytes
from ssz import get_hash_tree_root
from ssz.hash import hash_eth2
from ssz.hashable_container import HashableContainer
from ssz.sedes import (
    Bitlist,
    Bitvector,
    ByteVector,
    List,
    Vector,
    boolean,
    byte,
    bytes4,
    bytes32,
    bytes48,
    uint8,
    uint64,
    uint256
)

from eth_typing import HexStr
import constants
from typing import NewType, Dict, Any
from dataclasses import fields

Hash32 = bytes32
Root = bytes32
Gwei = uint64
Epoch = uint64
Slot = uint64
CommitteeIndex = uint64
ValidatorIndex = uint64
BLSPubkey = bytes48
ExecutionAddress = ByteVector(20)
WithdrawalIndex = uint64
ParticipationFlags = uint8
Version = bytes4

EpochNumber = NewType('EpochNumber', int)
FrameNumber = NewType('FrameNumber', int)
StateRoot = NewType('StateRoot', HexStr)
BlockRoot = NewType('BlockRoot', HexStr)
SlotNumber = NewType('SlotNumber', int)

BlockHash = NewType('BlockHash', HexStr)
BlockNumber = NewType('BlockNumber', int)


@dataclass
class FromResponse:
    """
    Class for extending dataclass with custom from_response method, ignored extra fields
    """

    @classmethod
    def from_response(cls, **kwargs) -> 'Self':
        class_field_names = [field.name for field in fields(cls)]
        return cls(**{k: v for k, v in kwargs.items() if k in class_field_names})

@dataclass
class BlockHeaderMessage(FromResponse):
    slot: str
    proposer_index: str
    parent_root: BlockRoot
    state_root: StateRoot
    body_root: str

class InclusionProofUtilsTrait:

    def get_merkle_tree_leafs(self):
        return self.hash_tree.chunks

    @property
    def _raw_hash_tree_for_inclusion(self):
        # Last layer only has one value that is the hash tree root
        return self.hash_tree.raw_hash_tree[:-1]

    def _get_field_index(self, field_name):
        return self._meta.field_names.index(field_name)

    def construct_inclusion_proof(self, field_name, field_hash):
        field_index = self._get_field_index(field_name)
        raw_hash_tree = self._raw_hash_tree_for_inclusion

        assert field_hash == raw_hash_tree[0][field_index],\
            (f"Field hash does not match the expected one.\n"
             f"Computed field index {field_index}.\n"
             f"Expected field hash: {raw_hash_tree[0][field_index]}.\n"
             f"Given field hash   :{field_hash}")

        result = []
        for layer in raw_hash_tree:
            sibling_idx = field_index - 1 if field_index % 2 == 1 else field_index + 1
            result.append(layer[sibling_idx])
            field_index //= 2
        return result

    def verify_inclusion_proof(self, field_name, field_hash, inclusion_proof):
        field_index = self._get_field_index(field_name)

        raw_hash_tree = self._raw_hash_tree_for_inclusion
        if len(inclusion_proof) != len(raw_hash_tree):
            return False

        current_hash = field_hash
        for idx in range(len(inclusion_proof)):
            inclusion_step, layer = inclusion_proof[idx], raw_hash_tree[idx]
            assert current_hash == layer[field_index], \
                (f"Verification failed at layer {idx}, field_index {field_index}.\n"
                 f"Expected{layer[field_index]}, got {current_hash}")
            if field_index % 2 == 0:
                current_hash = hash_eth2(current_hash + inclusion_step)
            else:
                current_hash = hash_eth2(inclusion_step + current_hash)

            field_index //= 2

        return current_hash == get_hash_tree_root(self)


class Fork(HashableContainer):
    fields = [
        ("previous_version", Version),
        ("current_version", Version),
        ("epoch", Epoch)  # Epoch of latest fork
    ]


class Checkpoint(HashableContainer):
    fields = [
        ("epoch", Epoch),
        ("root", Root)
    ]


class BeaconBlockHeader(HashableContainer, InclusionProofUtilsTrait):
    fields = [
        ("slot", Slot),
        ("proposer_index", ValidatorIndex),
        ("parent_root", Root),
        ("state_root", Root),
        ("body_root", Root),
    ]

    @classmethod
    def from_api(cls, api_response: BlockHeaderMessage) -> 'BeaconBlockHeader':
        return cls.create(
            slot = int(api_response.slot),
            proposer_index = int(api_response.proposer_index),
            parent_root = HexBytes(api_response.parent_root),
            state_root = HexBytes(api_response.state_root),
            body_root = HexBytes(api_response.body_root),
        )

    @classmethod
    def from_json(cls, json_response: Dict[str, Any]) -> 'BeaconBlockHeader':
        return cls.from_api(BlockHeaderMessage.from_response(**json_response))


class Eth1Data(HashableContainer):
    fields = [
        ("deposit_root", Root),
        ("deposit_count", uint64),
        ("block_hash", Hash32),
    ]


class Validator(HashableContainer):
    fields = [
        ("pubkey", BLSPubkey),
        ("withdrawal_credentials", bytes32),  # Commitment to pubkey for withdrawals
        ("effective_balance", Gwei),  # Balance at stake
        ("slashed", boolean),
        # Status epochs
        ("activation_eligibility_epoch", Epoch),  # When criteria for activation were met
        ("activation_epoch", Epoch),
        ("exit_epoch", Epoch),
        ("withdrawable_epoch", Epoch),  # When validator can withdraw funds
    ]


class AttestationData(HashableContainer):
    fields = [
        ("slot", Slot),
        ("index", CommitteeIndex),
        ("beacon_block_root", Root),
        ("source", Checkpoint),
        ("target", Checkpoint),
    ]


class PendingAttestation(HashableContainer):
    fields = [
        ("aggregation_bits", Bitlist(constants.MAX_VALIDATORS_PER_COMMITTEE)),
        ("data", AttestationData),
        ("inclusion_delay", Slot),
        ("proposer_index", ValidatorIndex),
    ]


class SyncCommittee(HashableContainer):
    fields = [
        ("pubkeys", Vector(BLSPubkey, constants.SYNC_COMMITTEE_SIZE)),
        ("aggregate_pubkey", BLSPubkey),
    ]


class ExecutionPayloadHeader(HashableContainer):
    # Execution block header fields
    fields = [
        ("parent_hash", Hash32),
        ("fee_recipient", ExecutionAddress),
        ("state_root", bytes32),
        ("receipts_root", bytes32),
        ("logs_bloom", ByteVector(constants.BYTES_PER_LOGS_BLOOM)),
        ("prev_randao", bytes32),
        ("block_number", uint64),
        ("gas_limit", uint64),
        ("gas_used", uint64),
        ("timestamp", uint64),
        # ("extra_data", ByteList(constants.MAX_EXTRA_DATA_BYTES)),
        # workaround - looks like ByteList is partially broken, but extra data is exactly bytes32
        ("extra_data", List(byte, constants.MAX_EXTRA_DATA_BYTES)),
        ("base_fee_per_gas", uint256),
        ("block_hash", Hash32),
        ("transactions_root", Root),
        ("withdrawals_root", Root),
        # Since Deneb
        ("blob_gas_used", uint64),
        ("excess_blob_gas", uint64),
    ]


class HistoricalSummary(HashableContainer):
    """
    `HistoricalSummary` matches the components of the phase0 `HistoricalBatch`
    making the two hash_tree_root-compatible.
    """
    fields = [
        ("block_summary_root", Root),
        ("state_summary_root", Root),
    ]


Validators = List(Validator, constants.VALIDATOR_REGISTRY_LIMIT)
Balances = List(Gwei, constants.VALIDATOR_REGISTRY_LIMIT)
JustificationBits = Bitvector(constants.JUSTIFICATION_BITS_LENGTH)


class BeaconState(HashableContainer, InclusionProofUtilsTrait):
    fields = [
        # Versioning
        ("genesis_time", uint64),
        ("genesis_validators_root", Root),
        ("slot", Slot),
        ("fork", Fork),
        # History
        ("latest_block_header", BeaconBlockHeader),
        ("block_roots", Vector(Root, constants.SLOTS_PER_HISTORICAL_ROOT)),
        ("state_roots", Vector(Root, constants.SLOTS_PER_HISTORICAL_ROOT)),
        ("historical_roots", List(Root, constants.HISTORICAL_ROOTS_LIMIT)),
        # Frozen in Capella, replaced by historical_summaries
        # Eth1
        ("eth1_data", Eth1Data),
        ("eth1_data_votes",
         List(Eth1Data, constants.EPOCHS_PER_ETH1_VOTING_PERIOD * constants.SLOTS_PER_EPOCH)),
        ("eth1_deposit_index", uint64),
        # Registry
        ("validators", Validators),
        ("balances", Balances),
        # Randomness
        ("randao_mixes", Vector(bytes32, constants.EPOCHS_PER_HISTORICAL_VECTOR)),
        # Slashings
        ("slashings", Vector(Gwei, constants.EPOCHS_PER_SLASHINGS_VECTOR)),
        # Per-epoch sums of slashed effective balances
        # Participation
        ("previous_epoch_participation", List(ParticipationFlags, constants.VALIDATOR_REGISTRY_LIMIT)),
        ("current_epoch_participation", List(ParticipationFlags, constants.VALIDATOR_REGISTRY_LIMIT)),
        # Finality
        ("justification_bits", JustificationBits),
        # Bit set for every recent justified epoch
        ("previous_justified_checkpoint", Checkpoint),
        ("current_justified_checkpoint", Checkpoint),
        ("finalized_checkpoint", Checkpoint),
        # Inactivity
        ("inactivity_scores", List(uint64, constants.VALIDATOR_REGISTRY_LIMIT)),
        # Sync
        ("current_sync_committee", SyncCommittee),
        ("next_sync_committee", SyncCommittee),
        # Execution
        ("latest_execution_payload_header", ExecutionPayloadHeader),  # (Modified in Capella)
        # Withdrawals
        ("next_withdrawal_index", WithdrawalIndex),  # (New in Capella)
        ("next_withdrawal_validator_index", ValidatorIndex),  # (New in Capella)
        # Deep history valid from Capella onwards
        ("historical_summaries", List(HistoricalSummary, constants.HISTORICAL_ROOTS_LIMIT)),
        # (New in Capella)
    ]
