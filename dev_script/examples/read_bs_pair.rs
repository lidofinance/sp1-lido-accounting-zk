use sp1_lido_accounting_zk_shared::eth_spec;
use typenum::Unsigned;

use tree_hash::TreeHash;

use dotenvy::dotenv;
use simple_logger::SimpleLogger;
use sp1_lido_accounting_scripts::beacon_state_reader::reqwest::{BeaconChainRPC, CachedReqwestBeaconStateReader};
use sp1_lido_accounting_scripts::beacon_state_reader::{BeaconStateReader, StateId};
use std::env;
use std::path::PathBuf;

#[tokio::main]
async fn main() {
    dotenv().ok();
    SimpleLogger::new().env().init().unwrap();

    let consensus_layer_rpc_url = env::var("CONSENSUS_LAYER_RPC").expect("Failed to read CONSENSUS_LAYER_RPC env var");
    let bs_endpoint = env::var("BEACON_STATE_RPC").expect("Failed to read BEACON_STATE_RPC env var");
    let file_store = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../temp");

    let bs_reader = CachedReqwestBeaconStateReader::new(&consensus_layer_rpc_url, &bs_endpoint, &file_store);

    let finalized_slot = bs_reader
        .get_finalized_slot()
        .await
        .expect("Failed to read finalized slot");
    let previous_slot = finalized_slot - eth_spec::SlotsPerEpoch::to_u64() * 2;

    tracing::info!("Loading beacon states for slots: current {finalized_slot}, previous {previous_slot}");

    let beacon_state1 = bs_reader
        .read_beacon_state(&StateId::Slot(previous_slot))
        .await
        .expect("Failed to read beacon state");
    let beacon_block_header1 = bs_reader
        .read_beacon_block_header(&StateId::Slot(previous_slot))
        .await
        .expect("Failed to read beacon block header");
    tracing::info!(
        "Read Beacon State for slot {:?}, with {} validators, beacon block hash: {}",
        beacon_state1.slot,
        beacon_state1.validators.to_vec().len(),
        hex::encode(beacon_block_header1.tree_hash_root())
    );

    let beacon_state2 = bs_reader
        .read_beacon_state(&StateId::Slot(finalized_slot))
        .await
        .expect("Failed to read beacon state");
    let beacon_block_header2 = bs_reader
        .read_beacon_block_header(&StateId::Slot(finalized_slot))
        .await
        .expect("Failed to read beacon block header");
    tracing::info!(
        "Read Beacon State for slot {:?}, with {} validators, beacon block hash: {}",
        beacon_state2.slot,
        beacon_state2.validators.to_vec().len(),
        hex::encode(beacon_block_header2.tree_hash_root())
    );
}
