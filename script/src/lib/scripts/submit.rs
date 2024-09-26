use std::path::PathBuf;

use crate::beacon_state_reader::BeaconStateReader;
use crate::consts::Network;
use crate::proof_storage;
use crate::scripts::prelude::Contract;
use crate::scripts::shared as shared_logic;
use crate::sp1_client_wrapper::SP1ClientWrapper;

use alloy_primitives::TxHash;
use sp1_lido_accounting_zk_shared::eth_consensus_layer::Hash256;

pub struct Flags {
    pub verify: bool,
    pub store: bool,
}

pub async fn run(
    client: SP1ClientWrapper,
    bs_reader: impl BeaconStateReader,
    contract: Contract,
    target_slot: u64,
    prev_slot: Option<u64>,
    network: Network,
    flags: Flags,
) -> anyhow::Result<TxHash> {
    let previous_slot = if let Some(prev) = prev_slot {
        prev
    } else {
        contract.get_latest_report_slot().await?
    };

    log::info!(
        "Submitting report for network {:?}, slot: {}, previous_slot: {}",
        network,
        target_slot,
        previous_slot,
    );

    let lido_withdrawal_credentials: Hash256 = network.get_config().lido_withdrawal_credentials.into();

    let target_bh = bs_reader.read_beacon_block_header(target_slot).await?;
    let target_bs = bs_reader.read_beacon_state(target_slot).await?;
    let old_bs = bs_reader.read_beacon_state(previous_slot).await?;

    let (program_input, public_values) =
        shared_logic::prepare_program_input(&target_bs, &target_bh, &old_bs, &lido_withdrawal_credentials);

    let proof = client.prove(program_input).expect("Failed to generate proof");
    log::info!("Generated proof");

    if flags.verify {
        client.verify_proof(&proof).expect("Failed to verify proof");
        log::info!("Verified proof");
    }

    shared_logic::verify_public_values(&proof.public_values, &public_values).expect("Failed to verify public inputs");
    log::info!("Verified public values");

    if flags.store {
        let file_name = format!("proof_{}_{}.json", network.as_str(), target_slot);
        let proof_file = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../temp/proofs")
            .join(file_name);
        proof_storage::store_proof_and_metadata(&proof, client.vk(), proof_file.as_path());
    }

    log::info!("Sending report");
    let tx_hash = contract
        .submit_report_data(
            target_bs.slot,
            public_values.report,
            public_values.metadata,
            proof.bytes(),
            proof.public_values.to_vec(),
        )
        .await?;
    anyhow::Ok(tx_hash)
}
