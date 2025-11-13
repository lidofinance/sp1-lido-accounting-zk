use sp1_lido_accounting_scripts::beacon_state_reader::{
    BeaconStateReader, RefSlotResolver, StateId,
};

use sp1_lido_accounting_scripts::scripts::shared as shared_logic;
use sp1_lido_accounting_scripts::sp1_client_wrapper::SP1ClientWrapper;
use sp1_lido_accounting_scripts::{proof_storage, utils};

use sp1_lido_accounting_zk_shared::io::program_io::WithdrawalVaultData;
use std::path::{Path, PathBuf};
use tokio::try_join;

use alloy_primitives::Address;
use sp1_lido_accounting_zk_shared::eth_consensus_layer::Hash256;
use sp1_lido_accounting_zk_shared::io::eth_io::ReferenceSlot;

use sp1_lido_accounting_scripts::scripts::prelude::ScriptRuntime;

fn store_withdrawal_vault_data(data: &WithdrawalVaultData, proof_file: &Path) {
    utils::write_json(proof_file, &data).expect("failed to write fixture");
}

pub async fn run(
    runtime: &ScriptRuntime,
    target_slot: ReferenceSlot,
    previous_slot: ReferenceSlot,
    fixture_files: Vec<PathBuf>,
    withdrawal_vault_fixture_files: Vec<PathBuf>,
) -> anyhow::Result<()> {
    let bs_reader = runtime.bs_reader();
    let ref_slot_resolver = runtime.ref_slot_resolver();

    let (actual_target_slot, actual_previous_slot) = try_join!(
        ref_slot_resolver.find_bc_slot_for_refslot(target_slot),
        ref_slot_resolver.find_bc_slot_for_refslot(previous_slot)
    )?;
    let target_state_id = StateId::Slot(actual_target_slot);
    let previous_state_id = StateId::Slot(actual_previous_slot);
    let (target_bh, target_bs, old_bs) = try_join!(
        bs_reader.read_beacon_block_header(&target_state_id),
        bs_reader.read_beacon_state(&target_state_id),
        bs_reader.read_beacon_state(&previous_state_id)
    )?;

    let lido_withdrawal_credentials: Hash256 = runtime.lido_settings.withdrawal_credentials;

    let lido_withdrawal_vault: Address = runtime.lido_settings.withdrawal_vault_address;
    let execution_layer_block_hash = target_bs.latest_execution_payload_header().block_hash;
    let withdrawal_vault_data = runtime
        .eth_infra
        .eth_client
        .get_withdrawal_vault_data(lido_withdrawal_vault, execution_layer_block_hash)
        .await?;

    for fixture_file in withdrawal_vault_fixture_files {
        store_withdrawal_vault_data(&withdrawal_vault_data, fixture_file.as_path());
    }

    let (program_input, public_values) = shared_logic::prepare_program_input(
        ReferenceSlot(target_slot.0),
        &target_bs,
        &target_bh,
        &old_bs,
        &lido_withdrawal_credentials,
        withdrawal_vault_data,
    )?;

    let proof = runtime
        .sp1_infra
        .sp1_client
        .prove(program_input)
        .expect("Failed to generate proof");
    tracing::info!("Generated proof");

    runtime
        .sp1_infra
        .sp1_client
        .verify_proof(&proof)
        .expect("Failed to verify proof");
    tracing::info!("Verified proof");

    shared_logic::verify_public_values(&proof.public_values, &public_values)
        .expect("Failed to verify public inputs");
    tracing::info!("Verified public values");

    for fixture_file in fixture_files {
        proof_storage::store_proof_and_metadata(
            &proof,
            runtime.sp1_infra.sp1_client.vk(),
            fixture_file.as_path(),
        )
        .expect("Failed to store proof and metadata");
    }

    anyhow::Ok(())
}
