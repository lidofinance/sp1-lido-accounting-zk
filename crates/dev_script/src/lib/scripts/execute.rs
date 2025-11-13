use std::sync::Arc;

use sp1_lido_accounting_scripts::beacon_state_reader::{
    BeaconStateReader, RefSlotResolver, StateId,
};
use sp1_lido_accounting_scripts::eth_client::ReportContract;
use sp1_lido_accounting_scripts::scripts::shared as shared_logic;

use sp1_lido_accounting_scripts::sp1_client_wrapper::SP1ClientWrapper;
use sp1_lido_accounting_zk_shared::eth_consensus_layer::Hash256;
use sp1_lido_accounting_zk_shared::io::eth_io::{BeaconChainSlot, ReferenceSlot};
use tokio::try_join;

use sp1_lido_accounting_scripts::scripts::prelude::ScriptRuntime;

async fn get_previous_bc_slot(
    maybe_previous_ref_slot: Option<ReferenceSlot>,
    ref_slot_resolver: Arc<impl RefSlotResolver>,
    contract: &ReportContract,
) -> anyhow::Result<BeaconChainSlot> {
    let result = match maybe_previous_ref_slot {
        Some(prev) => ref_slot_resolver.find_bc_slot_for_refslot(prev).await?,
        None => contract.get_latest_validator_state_slot().await?,
    };
    Ok(result)
}

pub async fn run(
    runtime: &ScriptRuntime,
    target_slot: ReferenceSlot,
    maybe_previous_slot: Option<ReferenceSlot>,
) -> anyhow::Result<()> {
    let refslot_resolver = runtime.ref_slot_resolver();
    let (actual_target_slot, actual_previous_slot) = try_join!(
        refslot_resolver.find_bc_slot_for_refslot(target_slot),
        get_previous_bc_slot(
            maybe_previous_slot,
            Arc::clone(&refslot_resolver),
            &runtime.lido_infra.report_contract
        ),
    )?;
    let target_state_id = StateId::Slot(actual_target_slot);
    let old_state_id = StateId::Slot(actual_previous_slot);
    let bs_reader = runtime.bs_reader();

    let (target_bh, target_bs, old_bs) = try_join!(
        bs_reader.read_beacon_block_header(&target_state_id),
        bs_reader.read_beacon_state(&target_state_id),
        bs_reader.read_beacon_state(&old_state_id)
    )?;

    let lido_withdrawal_credentials: Hash256 = runtime.lido_settings.withdrawal_credentials;

    let execution_layer_block_hash = target_bs.latest_execution_payload_header().block_hash;
    let withdrawal_vault_data = runtime
        .eth_infra
        .eth_client
        .get_withdrawal_vault_data(
            runtime.lido_settings.withdrawal_vault_address,
            execution_layer_block_hash,
        )
        .await?;

    let (program_input, public_values) = shared_logic::prepare_program_input(
        target_slot,
        &target_bs,
        &target_bh,
        &old_bs,
        &lido_withdrawal_credentials,
        withdrawal_vault_data,
    )?;

    tracing::info!("Executing program");
    let (exec_public_values, execution_report) =
        runtime.sp1_infra.sp1_client.execute(program_input).unwrap();

    tracing::info!(
        "Executed program with {} cycles",
        execution_report.total_instruction_count() + execution_report.total_syscall_count()
    );
    tracing::debug!("Full execution report:\n{}", execution_report);

    shared_logic::verify_public_values(&exec_public_values, &public_values)
        .expect("Failed to verify public inputs");
    tracing::info!("Successfully verified public values!");
    anyhow::Ok(())
}
