use std::env;
use std::sync::Arc;

use alloy_primitives::keccak256;
use alloy_rlp::Decodable;
use eth_trie::MemoryDB;
use eth_trie::{EthTrie, Trie};
use simple_logger::SimpleLogger;
use sp1_lido_accounting_scripts::beacon_state_reader::BeaconStateReaderEnum;
use sp1_lido_accounting_scripts::{
    beacon_state_reader::{BeaconStateReader, StateId},
    consts::{self, NetworkInfo},
    scripts,
};
use sp1_lido_accounting_zk_shared::eth_execution_layer::EthAccountRlpValue;

#[tokio::main]
async fn main() {
    SimpleLogger::new().env().init().unwrap();
    let chain = env::var("EVM_CHAIN").expect("EVM_CHAIN env var not set");
    let network = consts::read_network(&chain);
    let reader = BeaconStateReaderEnum::new_from_env(&network);
    let bs = reader
        .read_beacon_state(&StateId::Head)
        .await
        .expect("Failed to read bs");

    log::info!("Beacon slot: {}", bs.slot);
    let network = consts::read_network(&chain);
    let network_config = network.get_config();

    let (eth_client, contract) = scripts::prelude::initialize_eth();
    let withdrawal_vault_data = eth_client
        .get_withdrawal_vault_data(
            network_config.lido_withdrwawal_vault_address.into(),
            bs.latest_execution_payload_header.block_hash,
        )
        .await
        .expect("Failed to load balance proof");

    let key = keccak256(network_config.lido_withdrwawal_vault_address);

    log::info!("Balance: {}", withdrawal_vault_data.balance);
    let trie = EthTrie::new(Arc::new(MemoryDB::new(true)));
    let found = trie
        .verify_proof(
            bs.latest_execution_payload_header.state_root.to_fixed_bytes().into(),
            key.as_slice(),
            withdrawal_vault_data.account_proof,
        )
        .expect("Verified");
    if let Some(value) = found {
        let decoded = EthAccountRlpValue::decode(&mut value.as_slice()).unwrap();
        log::info!("Decoded account state {:?}", decoded);
    } else {
        panic!("Key not found: {:?}", hex::encode(key));
    }
}
