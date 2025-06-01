use crate::beacon_state_reader::{BeaconStateReader, RefSlotResolver, StateId};
use crate::consts::NetworkInfo;
use crate::scripts::shared as shared_logic;
use crate::sp1_client_wrapper::SP1ClientWrapper;

use crate::scripts::prelude::ScriptRuntime;
use alloy_primitives::{Address, TxHash};
use anyhow::{self, Context};
use sp1_lido_accounting_zk_shared::eth_consensus_layer::Hash256;
use sp1_lido_accounting_zk_shared::io::eth_io::{BeaconChainSlot, ReferenceSlot};
use tracing::Instrument;

#[derive(Debug, Default)]
pub struct Flags {
    pub verify: bool,
    pub dry_run: bool,
}

struct ResolvedSlotValues {
    /// This is the reference slot for report
    /// The report will be recorded for this slot in the on-chain contract
    report_slot: ReferenceSlot,
    /// Slot used to fetch BeaconState and other components.
    /// Most often equal to report_slot, unless there were no block for that slot -
    /// in that case see RefSlotResolver and implementations
    target_slot: BeaconChainSlot,
    /// This is the actual (not reference) slot for the old report
    previous_slot: BeaconChainSlot,
}

async fn resolve_slot_values(
    runtime: &ScriptRuntime,
    target_slot: Option<ReferenceSlot>,
    prev_slot: Option<ReferenceSlot>,
) -> anyhow::Result<ResolvedSlotValues> {
    let resolved_target_slot = if let Some(slot) = target_slot {
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
        .find_bc_slot_for_refslot(resolved_target_slot)
        .await?;

    let is_finalized = runtime
        .ref_slot_resolver()
        .is_finalized_slot(actual_target_slot)
        .await?;

    if !is_finalized {
        tracing::error!(
            target_slot = ?resolved_target_slot,
            "Target slot {actual_target_slot} resolved for reference slot {resolved_target_slot} is not yet finalized."
        );
        return Err(anyhow::anyhow!(
            "Target slot {actual_target_slot} is not yet finalized - aborting"
        ));
    }
    Ok(ResolvedSlotValues {
        report_slot: resolved_target_slot,
        target_slot: actual_target_slot,
        previous_slot: actual_previous_slot,
    })
}

async fn run_with_span(
    runtime: &ScriptRuntime,
    resolved_slot_values: ResolvedSlotValues,
    target_slot: Option<ReferenceSlot>,
    prev_slot: Option<ReferenceSlot>,
    flags: &Flags,
) -> anyhow::Result<TxHash> {
    tracing::info!(
        "Submitting report for network {:?}, target: (ref={:?}, actual={:?}), previous: (ref={:?}, actual={:?})",
        runtime.network().as_str(),
        target_slot,
        resolved_slot_values.target_slot,
        prev_slot,
        resolved_slot_values.previous_slot
    );

    let lido_withdrawal_credentials: Hash256 = runtime.lido_settings.withdrawal_credentials;
    let lido_withdrawal_vault: Address = runtime.lido_settings.withdrawal_vault_address;

    let target_bh = runtime
        .bs_reader()
        .read_beacon_block_header(&StateId::Slot(resolved_slot_values.target_slot))
        .await?;
    let target_bs = runtime
        .bs_reader()
        .read_beacon_state(&StateId::Slot(resolved_slot_values.target_slot))
        .await?;
    let old_bs = runtime
        .bs_reader()
        .read_beacon_state(&StateId::Slot(resolved_slot_values.previous_slot))
        .await?;

    let execution_layer_block_hash = target_bs.latest_execution_payload_header.block_hash;
    let withdrawal_vault_data = runtime
        .eth_infra
        .eth_client
        .get_withdrawal_vault_data(lido_withdrawal_vault, execution_layer_block_hash)
        .await?;

    let (program_input, public_values) = shared_logic::prepare_program_input(
        resolved_slot_values.report_slot,
        &target_bs,
        &target_bh,
        &old_bs,
        &lido_withdrawal_credentials,
        withdrawal_vault_data,
        true,
    )?;

    if flags.dry_run {
        tracing::info!("Dry run mode enabled, skipping proof generation and verification");
        return Ok(TxHash::default());
    }

    let proof = runtime
        .sp1_infra
        .sp1_client
        .prove(program_input)
        .inspect(|_proof| tracing::info!("Successfully obtained proof"))
        .inspect_err(|e| tracing::error!("Failed to obtain proof: {e:?}"))?;
    tracing::info!("Generated proof");

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
        .await
        .inspect(|tx_hash| tracing::info!("Report accepted, transaction: {tx_hash}"))
        .inspect_err(|e| tracing::error!("Failed to submit report: {e:?}"))?;
    Ok(tx_hash)
}

pub async fn run(
    runtime: &ScriptRuntime,
    target_slot: Option<ReferenceSlot>,
    prev_slot: Option<ReferenceSlot>,
    flags: &Flags,
) -> anyhow::Result<TxHash> {
    let resolved_slot_values = resolve_slot_values(runtime, target_slot, prev_slot)
        .await
        .inspect(|val| tracing::info!(report_slot=?val.report_slot, target_slot=?val.target_slot, previous_slot=?val.previous_slot, "Resolved ref slot argument to values"))
        .inspect_err(|e| {
            tracing::error!(
                target_slot_input = ?target_slot,
                previous_slot_input = ?prev_slot,
                "Failed to resolve arguments {e:?}"
            )
        })?;

    let submit_span = tracing::info_span!(
        "span:submit",
        report_slot=?resolved_slot_values.report_slot,
        target_slot=?resolved_slot_values.target_slot,
        previous_slot=?resolved_slot_values.previous_slot
    );

    run_with_span(runtime, resolved_slot_values, target_slot, prev_slot, flags)
        .instrument(submit_span)
        .await
}
