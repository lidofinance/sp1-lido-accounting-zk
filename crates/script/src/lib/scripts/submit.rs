use crate::beacon_state_reader::{BeaconStateReader, RefSlotResolver, StateId};
use crate::consts::NetworkInfo;
use crate::prometheus_metrics::{self, Metrics};
use crate::scripts::shared as shared_logic;
use crate::sp1_client_wrapper::SP1ClientWrapper;

use crate::scripts::prelude::ScriptRuntime;
use alloy_primitives::{Address, TxHash};
use anyhow::{self, Context};
use chrono::Utc;
use sp1_lido_accounting_zk_shared::eth_consensus_layer::Hash256;
use sp1_lido_accounting_zk_shared::io::eth_io::{BeaconChainSlot, HaveEpoch, PublicValuesRust, ReferenceSlot};
use sp1_lido_accounting_zk_shared::io::program_io::ProgramInput;
use sp1_lido_accounting_zk_shared::util::usize_to_u64;
use sp1_sdk::ExecutionReport;
use tracing::Instrument;

#[derive(Debug, Default)]
pub struct Flags {
    pub verify_input: bool,
    pub verify_proof: bool,
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

async fn prepare_input(
    runtime: &ScriptRuntime,
    resolved_slot_values: &ResolvedSlotValues,
) -> anyhow::Result<(ProgramInput, PublicValuesRust)> {
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
    Ok((program_input, public_values))
}

fn bump_outcome(runtime: &ScriptRuntime, outcome: &str) {
    runtime.metrics.execution.outcome.with_label_values(&[outcome]).inc();
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

    let (program_input, public_values) = prepare_input(runtime, &resolved_slot_values).await.inspect_err(|e| {
        tracing::error!("Failed to prepare program input: {e:?}");
        bump_outcome(runtime, prometheus_metrics::outcome::ERROR);
    })?;

    if flags.dry_run {
        tracing::info!("Dry run mode enabled, skipping proof generation and verification");
        return Ok(TxHash::default());
    }

    let (_, execution_report) = runtime
        .sp1_infra
        .sp1_client
        .execute(program_input.clone())
        .inspect(|_proof| tracing::info!("Successfully obtained proof"))
        .inspect_err(|e| {
            tracing::error!("Failed to execute program locally: {e:?}");
            bump_outcome(runtime, prometheus_metrics::outcome::ERROR)
        })?;

    let report_timestamp = Utc::now().timestamp();
    // report metrics
    let metric_report = report_metrics(
        &runtime.metrics,
        &resolved_slot_values,
        &program_input,
        &public_values,
        &execution_report,
        report_timestamp,
    );
    match metric_report {
        Ok(_) => tracing::debug!("Reported metrics"),
        Err(e) => tracing::warn!("Failed to report metrics {e:?}"),
    }

    let proof = runtime
        .sp1_infra
        .sp1_client
        .prove(program_input)
        .inspect(|_proof| tracing::info!("Successfully obtained proof"))
        .inspect_err(|e| {
            tracing::error!("Failed to obtain proof: {e:?}");
            bump_outcome(runtime, prometheus_metrics::outcome::ERROR)
        })?;
    tracing::info!("Generated proof");

    if flags.verify_input {
        shared_logic::verify_public_values(&proof.public_values, &public_values)
            .inspect_err(|e| {
                tracing::error!("Public values failed verification: {e:?}");
                bump_outcome(runtime, prometheus_metrics::outcome::ERROR)
            })
            .context("Public values from proof do not match expected ones")?;
        tracing::info!("Verified public values");
    }

    if flags.verify_proof {
        runtime
            .sp1_infra
            .sp1_client
            .verify_proof(&proof)
            .inspect_err(|e| {
                tracing::error!("Failed to verify proof: {e:?}");
                bump_outcome(runtime, prometheus_metrics::outcome::ERROR)
            })
            .context("Failed to verify proof")?;
        tracing::info!("Verified proof");
    }

    tracing::info!("Sending report");
    let tx_hash = runtime
        .lido_infra
        .report_contract
        .submit_report_data(proof.bytes(), proof.public_values.to_vec())
        .await
        .inspect(|tx_hash| {
            tracing::info!("Report accepted, transaction: {tx_hash}");
            runtime
                .metrics
                .execution
                .outcome
                .with_label_values(&[prometheus_metrics::outcome::SUCCESS])
                .inc();
        })
        .inspect_err(|e| {
            tracing::error!("Failed to submit report: {e:?}");
            runtime
                .metrics
                .execution
                .outcome
                .with_label_values(&[prometheus_metrics::outcome::REJECTION])
                .inc();
        })?;
    Ok(tx_hash)
}

const GWEI_TO_WEI: u64 = 1_000_000_000u64;

fn report_metrics(
    metrics: &Metrics,
    resolved_slot_values: &ResolvedSlotValues,
    program_input: &ProgramInput,
    public_values: &PublicValuesRust,
    execution_report: &ExecutionReport,
    report_timestamp: i64,
) -> anyhow::Result<()> {
    metrics.report.refslot.set(resolved_slot_values.report_slot.0);
    metrics.report.refslot.set(resolved_slot_values.report_slot.epoch());
    metrics.report.old_slot.set(resolved_slot_values.previous_slot.0);
    metrics.report.timestamp.set(report_timestamp);

    let total_validators = program_input.validators_and_balances.total_validators;
    let wv_balance_gwei: u64 = public_values
        .report
        .lido_withdrawal_vault_balance
        .checked_div(alloy_primitives::U256::from(GWEI_TO_WEI))
        .expect("Divisor is nonzero, guaranteed")
        .to();

    metrics.report.num_validators.set(total_validators);
    metrics
        .report
        .num_lido_validators
        .set(public_values.report.deposited_lido_validators);
    metrics.report.cl_balance_gwei.set(public_values.report.lido_cl_balance);
    metrics.report.withdrawal_vault_balance_gwei.set(wv_balance_gwei);

    let added = usize_to_u64(program_input.validators_and_balances.validators_delta.all_added.len())?;
    let changed = usize_to_u64(
        program_input
            .validators_and_balances
            .validators_delta
            .lido_changed
            .len(),
    )?;

    metrics.report.state_new_validators.set(added);
    metrics.report.state_changed_validators.set(changed);

    let total_cycles: u64 = execution_report.cycle_tracker.values().sum();
    metrics.execution.sp1_cycle_count.set(total_cycles);
    Ok(())
}

pub async fn run(
    runtime: &ScriptRuntime,
    target_slot: Option<ReferenceSlot>,
    prev_slot: Option<ReferenceSlot>,
    flags: &Flags,
) -> anyhow::Result<TxHash> {
    let timer = runtime.metrics.execution.execution_time_seconds.start_timer();
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

    let result = run_with_span(runtime, resolved_slot_values, target_slot, prev_slot, flags)
        .instrument(submit_span)
        .await;

    timer.observe_duration();

    result
}
