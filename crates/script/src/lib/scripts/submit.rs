use std::path::PathBuf;

use crate::beacon_state_reader::{BeaconStateReader, RefSlotResolver, StateId};
use crate::consts::NetworkInfo;
use crate::scripts::shared as shared_logic;
use crate::sp1_client_wrapper::SP1ClientWrapper;
use crate::{proof_storage, utils};

use crate::scripts::prelude::ScriptRuntime;
use alloy_primitives::{Address, TxHash};
use anyhow::{self, Context};
use sp1_lido_accounting_zk_shared::eth_consensus_layer::Hash256;
use sp1_lido_accounting_zk_shared::io::eth_io::ReferenceSlot;

#[derive(Debug, Default)]
pub struct Flags {
    pub verify: bool,
    pub store_input: bool,
    pub store_proof: bool,
    pub dry_run: bool,
}

pub async fn run(
    runtime: &ScriptRuntime,
    target_slot: Option<ReferenceSlot>,
    prev_slot: Option<ReferenceSlot>,
    flags: &Flags,
) -> anyhow::Result<TxHash> {
    let target_slot = if let Some(slot) = target_slot {
        slot
    } else {
        tracing::debug!("Reading target slot from hash consensus contract");
        let (refslot, _processing_deadline_slot) = runtime.lido_infra.hash_consensus_contract.get_refslot().await?;
        refslot
    };
    let actual_previous_slot = if let Some(prev) = prev_slot {
        tracing::debug!("Finding bc slot for previous report refslot {}", prev);
        runtime.ref_slot_resolver().find_bc_slot_for_refslot(prev).await?
    } else {
        tracing::debug!("Reading latest report slot from contract");
        runtime
            .lido_infra
            .report_contract
            .get_latest_validator_state_slot()
            .await?
    };
    let actual_target_slot = runtime
        .ref_slot_resolver()
        .find_bc_slot_for_refslot(target_slot)
        .await?;

    let network = runtime.network().as_str();

    tracing::info!(
        "Submitting report for network {:?}, target: (ref={:?}, actual={:?}), previous: (ref={:?}, actual={:?})",
        network,
        target_slot,
        actual_target_slot,
        prev_slot,
        actual_previous_slot
    );

    let lido_withdrawal_credentials: Hash256 = runtime.lido_settings.withdrawal_credentials;
    let lido_withdrawal_vault: Address = runtime.lido_settings.withdrawal_vault_address;

    let target_bh = runtime
        .bs_reader()
        .read_beacon_block_header(&StateId::Slot(actual_target_slot))
        .await?;
    let target_bs = runtime
        .bs_reader()
        .read_beacon_state(&StateId::Slot(actual_target_slot))
        .await?;
    let old_bs = runtime
        .bs_reader()
        .read_beacon_state(&StateId::Slot(actual_previous_slot))
        .await?;

    let execution_layer_block_hash = target_bs.latest_execution_payload_header.block_hash;
    let withdrawal_vault_data = runtime
        .eth_infra
        .eth_client
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
    )?;

    if flags.store_input {
        tracing::info!("Storing proof");
        let input_file_name = format!("input_{}_{}.json", network, target_slot);
        let input_path = PathBuf::from(std::env::var("PROOF_CACHE_DIR").expect("")).join(input_file_name);
        utils::write_json(&input_path, &program_input).expect("failed to write fixture");
        tracing::info!("Successfully written input to {input_path:?}");
    }

    if flags.dry_run {
        tracing::info!("Dry run mode enabled, skipping proof generation and verification");
        return Ok(TxHash::default());
    }

    let proof = runtime
        .sp1_infra
        .sp1_client
        .prove(program_input)
        .context("Failed to generate proof")?;
    tracing::info!("Generated proof");

    if flags.store_proof {
        tracing::info!("Storing proof");
        let file_name = format!("proof_{}_{}.json", network, target_slot);
        let proof_file = PathBuf::from(std::env::var("PROOF_CACHE_DIR").expect("")).join(file_name);
        let store_result =
            proof_storage::store_proof_and_metadata(&proof, runtime.sp1_infra.sp1_client.vk(), proof_file.as_path());
        if let Err(e) = store_result {
            tracing::warn!("Failed to store proof: {:?}", e);
        }
    }

    if flags.verify {
        shared_logic::verify_public_values(&proof.public_values, &public_values)
            .context("Public values from proof do not match expected ones")?;
        tracing::info!("Verified public values");

        runtime
            .sp1_infra
            .sp1_client
            .verify_proof(&proof)
            .context("Failed to verify proof")?;
        tracing::info!("Verified proof");
    }

    tracing::info!("Sending report");
    let tx_hash = runtime
        .lido_infra
        .report_contract
        .submit_report_data(proof.bytes(), proof.public_values.to_vec())
        .await?;
    Ok(tx_hash)
}
