pub use typenum::*;

pub type MaxValidatorsPerCommittee = U2048;
pub type SlotsPerEth1VotingPeriod = U2048; // 64 epochs * 32 slots per epoch
pub type SlotsPerHistoricalRoot = U8192;
pub type EpochsPerHistoricalVector = U65536;
pub type EpochsPerSlashingsVector = U8192;
pub type HistoricalRootsLimit = U16777216;
pub type ValidatorRegistryLimit = U1099511627776;
pub type SyncCommitteeSize = U512;
pub type BytesPerLogBloom = U256;
pub type MaxExtraDataBytes = U32;

pub type SlotsPerEpoch = U32;

pub type JustificationBitsLength = U4;

pub type ReducedValidatorRegistryLimit = U268435456; // 2 ^ 28
