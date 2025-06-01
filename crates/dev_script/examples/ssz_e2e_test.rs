use hex::FromHex;
use serde_json::Value;
use sp1_lido_accounting_scripts::prometheus_metrics;
use std::path::PathBuf;
use std::sync::Arc;
use tree_hash::TreeHash;

use sp1_lido_accounting_dev_scripts::synthetic::{
    BalanceGenerationMode, GenerationSpec, SyntheticBeaconStateCreator,
};

use sp1_lido_accounting_scripts::beacon_state_reader::{
    file::FileBasedBeaconStateReader, BeaconStateReader, StateId,
};
use sp1_lido_accounting_zk_shared::{
    eth_consensus_layer::BeaconBlockHeaderFields, merkle_proof::FieldProof,
};
use sp1_lido_accounting_zk_shared::{
    eth_consensus_layer::{BeaconBlockHeader, BeaconState, BeaconStateFields, Hash256},
    io::eth_io::BeaconChainSlot,
};

use simple_logger::SimpleLogger;

fn hex_str_to_h256(hex_str: &str) -> Hash256 {
    <[u8; 32]>::from_hex(hex_str)
        .expect("Couldn't parse hex_str as H256")
        .into()
}

fn verify_bh_parts(beacon_state_header: &BeaconBlockHeader, manifesto: &Value) {
    assert_eq!(
        beacon_state_header.slot.tree_hash_root(),
        hex_str_to_h256(manifesto["parts"]["slot"].as_str().unwrap())
    );
    assert_eq!(
        beacon_state_header.proposer_index.tree_hash_root(),
        hex_str_to_h256(manifesto["parts"]["proposer_index"].as_str().unwrap())
    );
    assert_eq!(
        beacon_state_header.parent_root.tree_hash_root(),
        hex_str_to_h256(manifesto["parts"]["parent_root"].as_str().unwrap())
    );
    assert_eq!(
        beacon_state_header.state_root.tree_hash_root(),
        hex_str_to_h256(manifesto["parts"]["state_root"].as_str().unwrap())
    );
    assert_eq!(
        beacon_state_header.body_root.tree_hash_root(),
        hex_str_to_h256(manifesto["parts"]["body_root"].as_str().unwrap())
    );
}

fn verify_bs_parts(beacon_state: &BeaconState, manifesto: &Value) {
    assert_eq!(
        beacon_state.genesis_time.tree_hash_root(),
        hex_str_to_h256(manifesto["parts"]["genesis_time"].as_str().unwrap()),
        "Field genesis_time mismatch"
    );
    assert_eq!(
        beacon_state.genesis_validators_root.tree_hash_root(),
        hex_str_to_h256(
            manifesto["parts"]["genesis_validators_root"]
                .as_str()
                .unwrap()
        ),
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
        hex_str_to_h256(
            manifesto["parts"]["previous_epoch_participation"]
                .as_str()
                .unwrap()
        ),
        "Field previous_epoch_participation mismatch"
    );
    assert_eq!(
        beacon_state.current_epoch_participation.tree_hash_root(),
        hex_str_to_h256(
            manifesto["parts"]["current_epoch_participation"]
                .as_str()
                .unwrap()
        ),
        "Field current_epoch_participation mismatch"
    );
    assert_eq!(
        beacon_state.justification_bits.tree_hash_root(),
        hex_str_to_h256(manifesto["parts"]["justification_bits"].as_str().unwrap()),
        "Field justification_bits mismatch"
    );
    assert_eq!(
        beacon_state.previous_justified_checkpoint.tree_hash_root(),
        hex_str_to_h256(
            manifesto["parts"]["previous_justified_checkpoint"]
                .as_str()
                .unwrap()
        ),
        "Field previous_justified_checkpoint mismatch"
    );
    assert_eq!(
        beacon_state.current_justified_checkpoint.tree_hash_root(),
        hex_str_to_h256(
            manifesto["parts"]["current_justified_checkpoint"]
                .as_str()
                .unwrap()
        ),
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
        hex_str_to_h256(
            manifesto["parts"]["current_sync_committee"]
                .as_str()
                .unwrap()
        ),
        "Field current_sync_committee mismatch"
    );
    assert_eq!(
        beacon_state.next_sync_committee.tree_hash_root(),
        hex_str_to_h256(manifesto["parts"]["next_sync_committee"].as_str().unwrap()),
        "Field next_sync_committee mismatch"
    );
    assert_eq!(
        beacon_state
            .latest_execution_payload_header
            .tree_hash_root(),
        hex_str_to_h256(
            manifesto["parts"]["latest_execution_payload_header"]
                .as_str()
                .unwrap()
        ),
        "Field latest_execution_payload_header mismatch"
    );
    assert_eq!(
        beacon_state.next_withdrawal_index.tree_hash_root(),
        hex_str_to_h256(
            manifesto["parts"]["next_withdrawal_index"]
                .as_str()
                .unwrap()
        ),
        "Field next_withdrawal_index mismatch"
    );
    assert_eq!(
        beacon_state
            .next_withdrawal_validator_index
            .tree_hash_root(),
        hex_str_to_h256(
            manifesto["parts"]["next_withdrawal_validator_index"]
                .as_str()
                .unwrap()
        ),
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
    let env = std::env::var("EVM_CHAIN").expect("EVM_CHAIN not set");
    let ssz_folder = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../temp/")
        .join(env);
    let creator = SyntheticBeaconStateCreator::new(&ssz_folder, false, true)
        .expect("Failed to create synthetic beacon state creator");
    let reader = FileBasedBeaconStateReader::new(
        &ssz_folder,
        Arc::new(prometheus_metrics::build_service_metrics(
            "namespace",
            "file_reader",
        )),
    )
    .expect("Failed to create beacon state reader");

    let slot = 1000000;
    let generation_spec = GenerationSpec {
        slot,
        non_lido_validators: 2_u64.pow(11),
        deposited_lido_validators: 2_u64.pow(11),
        exited_lido_validators: 0,
        pending_deposit_lido_validators: 0,
        balances_generation_mode: BalanceGenerationMode::FIXED,
        shuffle: false,
        base_slot: None,
        overwrite: false,
    };

    let slot = BeaconChainSlot(1000000);

    creator
        .evict_cache(slot.0)
        .expect("Failed to evict cached data");
    creator
        .create_beacon_state(generation_spec)
        .await
        .expect("Failed to create beacon state");

    reader
        .read_beacon_state(&StateId::Slot(slot))
        .await
        .unwrap_or_else(|_| panic!("Failed to evict cache for slot {}", slot));

    let beacon_state = reader
        .read_beacon_state(&StateId::Slot(slot))
        .await
        .expect("Failed to read beacon state");
    let beacon_block_header = reader
        .read_beacon_block_header(&StateId::Slot(slot))
        .await
        .expect("Failed to read beacon block header");
    tracing::info!(
        "Read Beacon State for slot {:?}, with {} validators",
        beacon_state.slot,
        beacon_state.validators.to_vec().len(),
    );

    // Step 2: Compute merkle roots
    let bs_merkle: Hash256 = beacon_state.tree_hash_root();
    let bh_merkle: Hash256 = beacon_block_header.tree_hash_root();

    // Step 2.1: compare against expected ones
    let manifesto = creator
        .read_manifesto(slot.0)
        .await
        .expect("Failed to read manifesto json");
    let manifesto_bs_merkle: Hash256 =
        hex_str_to_h256(manifesto["beacon_state"]["hash"].as_str().unwrap());
    let manifesto_bh_merkle: Hash256 =
        hex_str_to_h256(manifesto["beacon_block_header"]["hash"].as_str().unwrap());
    tracing::debug!("Beacon state merkle (computed): {}", hex::encode(bs_merkle));
    tracing::debug!(
        "Beacon state merkle (manifest): {}",
        hex::encode(manifesto_bs_merkle)
    );
    tracing::debug!(
        "Beacon block header merkle (computed): {}",
        hex::encode(bh_merkle)
    );
    tracing::debug!(
        "Beacon block header merkle (manifest): {}",
        hex::encode(manifesto_bh_merkle)
    );
    verify_bs_parts(&beacon_state, &manifesto["beacon_state"]);
    verify_bh_parts(&beacon_block_header, &manifesto["beacon_block_header"]);
    assert_eq!(bs_merkle, manifesto_bs_merkle);
    assert_eq!(bh_merkle, manifesto_bh_merkle);

    // Step 3: compute sum
    let total_balance: u64 = beacon_state.balances.iter().sum();
    let manifesto_total_balance = manifesto["report"]["total_balance"].as_u64().unwrap();
    tracing::debug!("Total balance (computed): {}", total_balance);
    tracing::debug!("Total balance (manifest): {}", manifesto_total_balance);
    assert_eq!(total_balance, manifesto_total_balance);

    // Step 4: get and verify multiproof for validators+balances fields in BeaconState
    // Step 4.1: get multiproof
    let bs_indices = [BeaconStateFields::validators, BeaconStateFields::balances];

    let bs_proof = beacon_state.get_members_multiproof(&bs_indices);
    tracing::debug!(
        "BeaconState proof hashes: {:?}",
        bs_proof.proof_hashes_hex()
    );

    // Step 4.2: verify multiproof
    let bs_leaves: Vec<Hash256> = vec![
        beacon_state.validators.tree_hash_root(),
        beacon_state.balances.tree_hash_root(),
    ];
    let verification_result =
        beacon_state.verify_instance(&bs_proof, &bs_indices, bs_leaves.as_slice());
    match verification_result {
        Ok(()) => tracing::info!("BeaconState Verification succeeded"),
        Err(error) => tracing::error!("Verification failed: {:?}", error),
    }

    // Step 5: get and verify multiproof for beacon state hash in BeaconBlockHeader
    // Step 5.1: get multiproof
    let bh_indices = [BeaconBlockHeaderFields::state_root];

    let bh_proof = beacon_block_header.get_members_multiproof(&bh_indices);
    tracing::debug!(
        "BeaconBlockHeader proof hashes: {:?}",
        bh_proof.proof_hashes_hex()
    );

    // Step 5.2: verify multiproof
    let bh_leaves = vec![bs_merkle];
    let verification_result =
        beacon_block_header.verify_instance(&bh_proof, &bh_indices, bh_leaves.as_slice());
    match verification_result {
        Ok(()) => tracing::info!("BeaconBlockHeader Verification succeeded"),
        Err(error) => tracing::error!("Verification failed: {:?}", error),
    }
}
