use serde::{Deserialize, Serialize};
use ssz_derive::{Decode, Encode};

use crate::eth_consensus_layer::{
    Balances, BeaconBlockHeaderPrecomputedHashes, BeaconStatePrecomputedHashes, Validators,
};

#[derive(PartialEq, Clone, Debug, Serialize, Deserialize)]
pub struct ProgramInput {
    pub slot: u64,
    pub beacon_block_hash: [u8; 32],
    pub beacon_block_header: BeaconBlockHeaderPrecomputedHashes,
    pub beacon_state: BeaconStatePrecomputedHashes,
    pub validators_and_balances_proof: Vec<u8>,
    pub validators_and_balances: ValsAndBals,
}

#[derive(PartialEq, Clone, Debug, Serialize, Deserialize, Encode, Decode)]
pub struct ValsAndBals {
    // #[serde(with = "ssz_types::serde_utils::quoted_u64_var_list")]
    pub balances: Balances,
    pub validators: Validators,
}
