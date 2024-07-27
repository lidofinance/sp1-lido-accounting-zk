use alloy_sol_types::{sol, SolType};

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
