use hex::FromHex;
use log;
use serde_json::Value;

use std::path::PathBuf;
use tree_hash::TreeHash;

use sp1_lido_accounting_zk_shared::beacon_state_reader::local_synthetic::{
    BalanceGenerationMode, SyntheticBeaconStateReader,
};
use sp1_lido_accounting_zk_shared::beacon_state_reader::BeaconStateReader;
use sp1_lido_accounting_zk_shared::eth_consensus_layer::{BeaconState, Hash256};

use simple_logger::SimpleLogger;

fn hex_str_to_h256(hex_str: &str) -> Hash256 {
    <[u8; 32]>::from_hex(hex_str)
        .expect("Couldn't parse hex_str as H256")
        .into()
}

fn verify_parts(beacon_state: &BeaconState, manifesto: &Value) {
    assert_eq!(
        beacon_state.genesis_time.tree_hash_root(),
        hex_str_to_h256(manifesto["parts"]["genesis_time"].as_str().unwrap()),
        "Field genesis_time mismatch"
    );
    assert_eq!(
        beacon_state.genesis_validators_root.tree_hash_root(),
        hex_str_to_h256(manifesto["parts"]["genesis_validators_root"].as_str().unwrap()),
        "Field genesis_validators_root mismatch"
    );
    assert_eq!(
        beacon_state.slot.tree_hash_root(),
        hex_str_to_h256(manifesto["parts"]["slot"].as_str().unwrap()),
        "Field slot mismatch"
    );
    assert_eq!(
        beacon_state.fork.tree_hash_root(),
        hex_str_to_h256(manifesto["parts"]["fork"].as_str().unwrap()),
        "Field fork mismatch"
    );
    assert_eq!(
        beacon_state.latest_block_header.tree_hash_root(),
        hex_str_to_h256(manifesto["parts"]["latest_block_header"].as_str().unwrap()),
        "Field latest_block_header mismatch"
    );
    assert_eq!(
        beacon_state.block_roots.tree_hash_root(),
        hex_str_to_h256(manifesto["parts"]["block_roots"].as_str().unwrap()),
        "Field block_roots mismatch"
    );
    assert_eq!(
        beacon_state.state_roots.tree_hash_root(),
        hex_str_to_h256(manifesto["parts"]["state_roots"].as_str().unwrap()),
        "Field state_roots mismatch"
    );
    assert_eq!(
        beacon_state.historical_roots.tree_hash_root(),
        hex_str_to_h256(manifesto["parts"]["historical_roots"].as_str().unwrap()),
        "Field historical_roots mismatch"
    );
    assert_eq!(
        beacon_state.eth1_data.tree_hash_root(),
        hex_str_to_h256(manifesto["parts"]["eth1_data"].as_str().unwrap()),
        "Field eth1_data mismatch"
    );
    assert_eq!(
        beacon_state.eth1_data_votes.tree_hash_root(),
        hex_str_to_h256(manifesto["parts"]["eth1_data_votes"].as_str().unwrap()),
        "Field eth1_data_votes mismatch"
    );
    assert_eq!(
        beacon_state.eth1_deposit_index.tree_hash_root(),
        hex_str_to_h256(manifesto["parts"]["eth1_deposit_index"].as_str().unwrap()),
        "Field eth1_deposit_index mismatch"
    );
    assert_eq!(
        beacon_state.validators.tree_hash_root(),
        hex_str_to_h256(manifesto["parts"]["validators"].as_str().unwrap()),
        "Field validators mismatch"
    );
    assert_eq!(
        beacon_state.balances.tree_hash_root(),
        hex_str_to_h256(manifesto["parts"]["balances"].as_str().unwrap()),
        "Field balances mismatch"
    );
    assert_eq!(
        beacon_state.randao_mixes.tree_hash_root(),
        hex_str_to_h256(manifesto["parts"]["randao_mixes"].as_str().unwrap()),
        "Field randao_mixes mismatch"
    );
    assert_eq!(
        beacon_state.slashings.tree_hash_root(),
        hex_str_to_h256(manifesto["parts"]["slashings"].as_str().unwrap()),
        "Field slashings mismatch"
    );
    assert_eq!(
        beacon_state.previous_epoch_participation.tree_hash_root(),
        hex_str_to_h256(manifesto["parts"]["previous_epoch_participation"].as_str().unwrap()),
        "Field previous_epoch_participation mismatch"
    );
    assert_eq!(
        beacon_state.current_epoch_participation.tree_hash_root(),
        hex_str_to_h256(manifesto["parts"]["current_epoch_participation"].as_str().unwrap()),
        "Field current_epoch_participation mismatch"
    );
    assert_eq!(
        beacon_state.justification_bits.tree_hash_root(),
        hex_str_to_h256(manifesto["parts"]["justification_bits"].as_str().unwrap()),
        "Field justification_bits mismatch"
    );
    assert_eq!(
        beacon_state.previous_justified_checkpoint.tree_hash_root(),
        hex_str_to_h256(manifesto["parts"]["previous_justified_checkpoint"].as_str().unwrap()),
        "Field previous_justified_checkpoint mismatch"
    );
    assert_eq!(
        beacon_state.current_justified_checkpoint.tree_hash_root(),
        hex_str_to_h256(manifesto["parts"]["current_justified_checkpoint"].as_str().unwrap()),
        "Field current_justified_checkpoint mismatch"
    );
    assert_eq!(
        beacon_state.finalized_checkpoint.tree_hash_root(),
        hex_str_to_h256(manifesto["parts"]["finalized_checkpoint"].as_str().unwrap()),
        "Field finalized_checkpoint mismatch"
    );
    assert_eq!(
        beacon_state.inactivity_scores.tree_hash_root(),
        hex_str_to_h256(manifesto["parts"]["inactivity_scores"].as_str().unwrap()),
        "Field inactivity_scores mismatch"
    );
    assert_eq!(
        beacon_state.current_sync_committee.tree_hash_root(),
        hex_str_to_h256(manifesto["parts"]["current_sync_committee"].as_str().unwrap()),
        "Field current_sync_committee mismatch"
    );
    assert_eq!(
        beacon_state.next_sync_committee.tree_hash_root(),
        hex_str_to_h256(manifesto["parts"]["next_sync_committee"].as_str().unwrap()),
        "Field next_sync_committee mismatch"
    );
    assert_eq!(
        beacon_state.latest_execution_payload_header.tree_hash_root(),
        hex_str_to_h256(manifesto["parts"]["latest_execution_payload_header"].as_str().unwrap()),
        "Field latest_execution_payload_header mismatch"
    );
    assert_eq!(
        beacon_state.next_withdrawal_index.tree_hash_root(),
        hex_str_to_h256(manifesto["parts"]["next_withdrawal_index"].as_str().unwrap()),
        "Field next_withdrawal_index mismatch"
    );
    assert_eq!(
        beacon_state.next_withdrawal_validator_index.tree_hash_root(),
        hex_str_to_h256(manifesto["parts"]["next_withdrawal_validator_index"].as_str().unwrap()),
        "Field next_withdrawal_validator_index mismatch"
    );
    assert_eq!(
        beacon_state.historical_summaries.tree_hash_root(),
        hex_str_to_h256(manifesto["parts"]["historical_summaries"].as_str().unwrap()),
        "Field historical_summaries mismatch"
    );
}

#[tokio::main]
async fn main() {
    SimpleLogger::new().env().init().unwrap();
    // Step 1. obtain SSZ-serialized beacon state
    // For now using a "synthetic" generator based on reference implementation (py-ssz)
    let reader = SyntheticBeaconStateReader::new(
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../temp"),
        2_u64.pow(12),
        2_u64.pow(6),
        BalanceGenerationMode::SEQUENTIAL,
        true,
        true,
    );

    let slot = 1000000;
    let beacon_state = reader
        .read_beacon_state(slot)
        .await
        .expect("Failed to read beacon state");
    log::info!(
        "Read Beacon State for slot {:?}, with {} validators",
        beacon_state.slot,
        beacon_state.validators.to_vec().len(),
    );

    // Step 2: Compute merkle roots
    let bs_merkle: Hash256 = beacon_state.tree_hash_root();

    // Step 2.1: compare against expected ones
    let manifesto = reader
        .read_manifesto(slot)
        .await
        .expect("Failed to read manifesto json");
    let manifesto_bs_merkle: Hash256 = hex_str_to_h256(manifesto["beacon_block_hash"].as_str().unwrap());
    log::debug!("Beacon state merkle (computed): {}", hex::encode(bs_merkle));
    log::debug!("Beacon state merkle (manifest): {}", hex::encode(manifesto_bs_merkle));
    verify_parts(&beacon_state, &manifesto);

    // Step 3: compute sum
    let total_balance: u64 = beacon_state.balances.iter().sum();
    let manifesto_total_balance = manifesto["report"]["total_balance"].as_u64().unwrap();
    log::debug!("Total balance (computed): {}", total_balance);
    log::debug!("Total balance (manifest): {}", manifesto_total_balance);
    assert_eq!(total_balance, manifesto_total_balance);

    // assert!(bs_merkle == root.into())
}
