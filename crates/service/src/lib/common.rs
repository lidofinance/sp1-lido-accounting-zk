use prometheus::Registry;
use sp1_lido_accounting_scripts::scripts::{self};
use sp1_lido_accounting_zk_shared::io::eth_io::ReferenceSlot;

use crate::prometheus_metrics;

pub struct AppState {
    pub registry: Registry,
    pub env_vars: scripts::prelude::EnvVars,
    pub script_runtime: scripts::prelude::ScriptRuntime,
    pub submit_flags: scripts::submit::Flags,
}

impl AppState {
    pub fn log_config_full(&self) {
        tracing::info!(
            env_vars = ?self.env_vars.for_logging(false),
            "Env vars",
        );
        tracing::debug!(
            submit_flags = ?self.submit_flags,
            "Script flags",
        );
    }

    pub fn log_config_important(&self) {
        tracing::info!(
            env_vars = ?self.env_vars.for_logging(true),
            "Env vars",
        );
        tracing::info!(
            submit_flags = ?self.submit_flags,
            "Script flags",
        );
    }
}

pub async fn run_submit(
    state: &AppState,
    refslot: Option<ReferenceSlot>,
    previous_slot: Option<ReferenceSlot>,
) -> Result<String, anyhow::Error> {
    state.log_config_important();
    scripts::submit::run(
        &state.script_runtime,
        refslot,
        previous_slot,
        &state.submit_flags,
    )
    .await
    .map(|tx_hash| {
        let tx_hash_str = hex::encode(tx_hash);
        tracing::info!("Report transaction complete {}", tx_hash_str);
        tx_hash_str
    })
    .map_err(|e| {
        tracing::error!("Failed to submit report {}", e);
        e
    })
}
