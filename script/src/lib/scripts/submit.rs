use std::path::PathBuf;

use crate::beacon_state_reader::{BeaconStateReader, StateId};
use crate::consts::NetworkInfo;
use crate::eth_client::Contract;
use crate::proof_storage;
use crate::scripts::shared as shared_logic;
use crate::sp1_client_wrapper::SP1ClientWrapper;

use alloy_primitives::TxHash;
use anyhow::{self, Context};
use sp1_lido_accounting_zk_shared::eth_consensus_layer::Hash256;

pub struct Flags {
    pub verify: bool,
    pub store: bool,
}

pub async fn run(
    client: &impl SP1ClientWrapper,
    bs_reader: &impl BeaconStateReader,
    contract: Contract,
    target_slot: u64,
    prev_slot: Option<u64>,
    network: impl NetworkInfo,
    flags: Flags,
) -> anyhow::Result<TxHash> {
    let previous_slot = if let Some(prev) = prev_slot {
        prev
    } else {
        contract.get_latest_report_slot().await?
    };

    log::info!(
        "Submitting report for network {:?}, slot: {}, previous_slot: {}",
        network.as_str(),
        target_slot,
        previous_slot,
    );

    let lido_withdrawal_credentials: Hash256 = network.get_config().lido_withdrawal_credentials.into();

    let target_bh = bs_reader.read_beacon_block_header(&StateId::Slot(target_slot)).await?;
    let target_bs = bs_reader.read_beacon_state(&StateId::Slot(target_slot)).await?;
    let old_bs = bs_reader.read_beacon_state(&StateId::Slot(previous_slot)).await?;

    let (program_input, public_values) =
        shared_logic::prepare_program_input(&target_bs, &target_bh, &old_bs, &lido_withdrawal_credentials, true);
    let proof = client.prove(program_input).context("Failed to generate proof")?;
    log::info!("Generated proof");

    if flags.store {
        log::info!("Storing proof");
        let file_name = format!("proof_{}_{}.json", network.as_str(), target_slot);
        let proof_file = PathBuf::from(std::env::var("PROOF_CACHE_DIR").expect("")).join(file_name);
        proof_storage::store_proof_and_metadata(&proof, client.vk(), proof_file.as_path());
    }

    if flags.verify {
        shared_logic::verify_public_values(&proof.public_values, &public_values)
            .context("Public values from proof do not match expected ones")?;
        log::info!("Verified public values");

        client.verify_proof(&proof).context("Failed to verify proof")?;
        log::info!("Verified proof");
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
        .await
        .context("Failed to submit report")?;
    Ok(tx_hash)
}
