pub use ssz_types::typenum::*;

// https://github.com/ethereum/consensus-specs/blob/dev/specs/phase0/beacon-chain.md#misc
const FAR_FUTURE_EPOCH: u64 = u64::MAX;
const JUSTIFICATION_BITS_LENGTH: u64 = 4;
const MAX_VALIDATORS_PER_COMMITTEE: u64 = 2_u64.pow(11);
pub type MaxValidatorsPerCommittee = U2048;
// https://github.com/ethereum/consensus-specs/blob/dev/specs/phase0/beacon-chain.md#time-parameters-1
const MIN_VALIDATOR_WITHDRAWABILITY_DELAY: u64 = 2_u64.pow(8);
const SHARD_COMMITTEE_PERIOD: u64 = 256;
const MIN_ATTESTATION_INCLUSION_DELAY: u64 = 2_u64.pow(0);
const SLOTS_PER_EPOCH: u64 = 2_u64.pow(5);
const MIN_SEED_LOOKAHEAD: u64 = 2_u64.pow(0);
const MAX_SEED_LOOKAHEAD: u64 = 2_u64.pow(2);
const MIN_EPOCHS_TO_INACTIVITY_PENALTY: u64 = 2_u64.pow(2);
const EPOCHS_PER_ETH1_VOTING_PERIOD: u64 = 2_u64.pow(6);
pub type SlotsPerEth1VotingPeriod = U2048; // 64 epochs * 32 slots per epoch
const SLOTS_PER_HISTORICAL_ROOT: u64 = 2_u64.pow(13);
pub type SlotsPerHistoricalRoot = U8192;
// https://github.com/ethereum/consensus-specs/blob/dev/specs/phase0/beacon-chain.md#state-list-lengths
const EPOCHS_PER_HISTORICAL_VECTOR: u64 = 2_u64.pow(16);
pub type EpochsPerHistoricalVector = U65536;
const EPOCHS_PER_SLASHINGS_VECTOR: u64 = 2_u64.pow(13);
pub type EpochsPerSlashingsVector = U8192;
const HISTORICAL_ROOTS_LIMIT: u64 = 2_u64.pow(24);
pub type HistoricalRootsLimit = U16777216;
const VALIDATOR_REGISTRY_LIMIT: u64 = 2_u64.pow(40);
pub type ValidatorRegistryLimit = U1099511627776;
// https://github.com/ethereum/consensus-specs/blob/dev/specs/phase0/beacon-chain.md#gwei-values
const EFFECTIVE_BALANCE_INCREMENT: u64 = 2_u64.pow(0) * 10_u64.pow(9);
const MAX_EFFECTIVE_BALANCE: u64 = 32 * 10_u64.pow(9);
// https://github.com/ethereum/consensus-specs/blob/dev/specs/capella/beacon-chain.md#execution
const MAX_WITHDRAWALS_PER_PAYLOAD: u64 = 2_u64.pow(4);
// https://github.com/ethereum/consensus-specs/blob/dev/specs/phase0/beacon-chain.md#validator-cycle
const MIN_PER_EPOCH_CHURN_LIMIT: u64 = 2_u64.pow(2);
const CHURN_LIMIT_QUOTIENT: u64 = 2_u64.pow(16);
// https://github.com/ethereum/consensus-specs/blob/dev/specs/phase0/beacon-chain.md#max-operations-per-block
const MAX_ATTESTATIONS: u64 = 2_u64.pow(7);

// https://github.com/ethereum/consensus-specs/blob/dev/specs/altair/beacon-chain.md#sync-committee
const SYNC_COMMITTEE_SIZE: u64 = 2_u64.pow(9);
pub type SyncCommitteeSize = U512;
const BYTES_PER_LOGS_BLOOM: u64 = 2_u64.pow(8);
pub type BytesPerLogBloom = U256;
const MAX_EXTRA_DATA_BYTES: u64 = 2_u64.pow(5);
pub type MaxExtraDataBytes = U32;

pub type JustificationBitsLength = U4;
