use alloy_sol_types::{sol, SolType};
use serde::{Deserialize, Serialize};
use ssz_derive::{Decode, Encode};
use ssz_types::VariableList;

use crate::{
    eth_consensus_layer::{Balances, BeaconBlockHeaderPrecomputedHashes, BeaconStatePrecomputedHashes, Validator},
    eth_spec,
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

type Validators = VariableList<Validator, eth_spec::ValidatorRegistryLimit>;

#[derive(PartialEq, Clone, Debug, Serialize, Deserialize, Encode, Decode)]
pub struct ValsAndBals {
    // #[serde(with = "ssz_types::serde_utils::quoted_u64_var_list")]
    pub balances: Balances,
    pub validators: Validators,
}

#[derive(PartialEq, Eq, Clone, Debug)]
pub struct PublicValuesRust {
    pub slot: u64,
    pub beacon_block_hash: [u8; 32],
}

/// The public values encoded as a tuple that can be easily deserialized inside Solidity.
pub type PublicValuesSolidity = sol! {
    tuple(uint64, bytes32)
};

impl TryFrom<&[u8]> for PublicValuesRust {
    type Error = alloy_sol_types::Error;

    fn try_from(value: &[u8]) -> core::result::Result<Self, Self::Error> {
        let (slot, block_hash) = PublicValuesSolidity::abi_decode(value, false)?;
        core::result::Result::Ok(Self {
            slot,
            beacon_block_hash: block_hash.into(),
        })
    }
}
