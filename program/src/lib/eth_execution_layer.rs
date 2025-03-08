use alloy_primitives::U256;
use alloy_rlp::{RlpDecodable, RlpEncodable};

use std::fmt::Debug;

#[derive(Debug, RlpEncodable, RlpDecodable)]
pub struct EthAccountRlpValue {
    pub nonce: u64,
    pub balance: U256,
    pub storage_hash: [u8; 32],
    pub code_hash: [u8; 32],
}
