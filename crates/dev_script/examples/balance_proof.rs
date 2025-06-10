use std::sync::Arc;

use alloy_primitives::keccak256;
use alloy_rlp::Decodable;
use eth_trie::MemoryDB;
use eth_trie::{EthTrie, Trie};
use simple_logger::SimpleLogger;

use sp1_lido_accounting_scripts::scripts::prelude::EnvVars;
use sp1_lido_accounting_scripts::{
    beacon_state_reader::{BeaconStateReader, StateId},
    scripts,
};
use sp1_lido_accounting_zk_shared::eth_execution_layer::EthAccountRlpValue;

#[tokio::main]
async fn main() {
    SimpleLogger::new().env().init().unwrap();
    let env_vars = EnvVars::init_from_env_or_crash();

    let script_runtime = scripts::prelude::ScriptRuntime::init(&env_vars)
        .expect("Failed to initialize script runtime");

    let bs = script_runtime
        .bs_reader()
        .read_beacon_state(&StateId::Head)
        .await
        .expect("Failed to read bs");

    tracing::info!("Beacon slot: {}", bs.slot);

    let withdrawal_vault_data = script_runtime
        .eth_infra
        .eth_client
        .get_withdrawal_vault_data(
            script_runtime.lido_settings.withdrawal_vault_address,
            bs.latest_execution_payload_header.block_hash,
        )
        .await
        .expect("Failed to load balance proof");

    let key = keccak256(script_runtime.lido_settings.withdrawal_vault_address);

    tracing::info!("Balance: {}", withdrawal_vault_data.balance);
    let trie = EthTrie::new(Arc::new(MemoryDB::new(true)));
    let found = trie
        .verify_proof(
            bs.latest_execution_payload_header.state_root,
            key.as_slice(),
            withdrawal_vault_data.account_proof,
        )
        .expect("Verified");
    if let Some(value) = found {
        let decoded = EthAccountRlpValue::decode(&mut value.as_slice()).unwrap();
        tracing::info!("Decoded account state {:?}", decoded);
    } else {
        panic!("Key not found: {:?}", hex::encode(key));
    }
}
