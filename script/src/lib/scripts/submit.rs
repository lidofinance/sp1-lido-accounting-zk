use std::path::PathBuf;

use crate::beacon_state_reader::{BeaconStateReader, StateId};
use crate::consts::NetworkInfo;
use crate::eth_client::{Contract, EthELClient};
use crate::scripts::shared as shared_logic;
use crate::sp1_client_wrapper::SP1ClientWrapper;
use crate::{proof_storage, utils};

use alloy_primitives::{Address, TxHash};
use anyhow::{self, Context};
use sp1_lido_accounting_zk_shared::eth_consensus_layer::Hash256;
use sp1_lido_accounting_zk_shared::io::eth_io::ReferenceSlot;

pub struct Flags {
    pub verify: bool,
    pub store_input: bool,
    pub store_proof: bool,
}

pub async fn run(
    client: &impl SP1ClientWrapper,
    bs_reader: &impl BeaconStateReader,
    contract: &Contract,
    eth_client: &EthELClient,
    target_slot: ReferenceSlot,
    prev_slot: Option<ReferenceSlot>,
    network: impl NetworkInfo,
    flags: Flags,
) -> anyhow::Result<TxHash> {
    let actual_previous_slot = if let Some(prev) = prev_slot {
        tracing::debug!("Finding bc slot for previous report refslot {}", prev);
        bs_reader.find_bc_slot_for_refslot(prev).await?
    } else {
        tracing::debug!("Reading latest report slot from contract");
        contract.get_latest_validator_state_slot().await?
    };
    let actual_target_slot = bs_reader.find_bc_slot_for_refslot(target_slot).await?;

    tracing::info!(
        "Submitting report for network {:?}, target: (ref={:?}, actual={:?}), previous: (ref={:?}, actual={:?})",
        network.as_str(),
        target_slot,
        actual_target_slot,
        prev_slot,
        actual_previous_slot
    );

    let network_config = network.get_config();
    let lido_withdrawal_credentials: Hash256 = network_config.lido_withdrawal_credentials.into();
    let lido_withdrawal_vault: Address = network_config.lido_withdrwawal_vault_address.into();

    let target_bh = bs_reader
        .read_beacon_block_header(&StateId::Slot(actual_target_slot))
        .await?;
    let target_bs = bs_reader.read_beacon_state(&StateId::Slot(actual_target_slot)).await?;
    let old_bs = bs_reader
        .read_beacon_state(&StateId::Slot(actual_previous_slot))
        .await?;

    let execution_layer_block_hash = target_bs.latest_execution_payload_header.block_hash;
    let withdrawal_vault_data = eth_client
        .get_withdrawal_vault_data(lido_withdrawal_vault, execution_layer_block_hash)
        .await?;

    let (program_input, public_values) = shared_logic::prepare_program_input(
        target_slot,
        &target_bs,
        &target_bh,
        &old_bs,
        &lido_withdrawal_credentials,
        withdrawal_vault_data,
        true,
    );

    if flags.store_input {
        tracing::info!("Storing proof");
        let input_file_name = format!("input_{}_{}.json", network.as_str(), target_slot);
        let input_path = PathBuf::from(std::env::var("PROOF_CACHE_DIR").expect("")).join(input_file_name);
        utils::write_json(&input_path, &program_input).expect("failed to write fixture");
        tracing::info!("Successfully written input to {input_path:?}");
    }

    let proof = client.prove(program_input).context("Failed to generate proof")?;
    tracing::info!("Generated proof");

    if flags.store_proof {
        tracing::info!("Storing proof");
        let file_name = format!("proof_{}_{}.json", network.as_str(), target_slot);
        let proof_file = PathBuf::from(std::env::var("PROOF_CACHE_DIR").expect("")).join(file_name);
        proof_storage::store_proof_and_metadata(&proof, client.vk(), proof_file.as_path());
    }

    if flags.verify {
        shared_logic::verify_public_values(&proof.public_values, &public_values)
            .context("Public values from proof do not match expected ones")?;
        tracing::info!("Verified public values");

        client.verify_proof(&proof).context("Failed to verify proof")?;
        tracing::info!("Verified proof");
    }

    tracing::info!("Sending report");
    let tx_hash = contract
        .submit_report_data(proof.bytes(), proof.public_values.to_vec())
        .await
        .context("Failed to submit report")?;
    Ok(tx_hash)
}
