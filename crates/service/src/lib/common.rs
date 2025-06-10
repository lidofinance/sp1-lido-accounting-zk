use std::sync::Arc;

use prometheus::{Encoder, Registry, TextEncoder};
use sp1_lido_accounting_scripts::scripts::{self};
use sp1_lido_accounting_zk_shared::io::eth_io::ReferenceSlot;

pub struct AppState {
    pub registry: Registry,
    pub env_vars: scripts::prelude::EnvVars,
    pub script_runtime: scripts::prelude::ScriptRuntime,
    pub submit_flags: scripts::submit::Flags,
    pub run_lock: Arc<tokio::sync::Mutex<()>>,
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

    pub fn report_metrics(&self) -> Result<(Vec<u8>, String), prometheus::Error> {
        // tracing::error!("Reporting on {:?}", &self.registry as *const _ as usize);
        let mut buffer = Vec::new();
        let encoder = TextEncoder::new();
        let mf = self.registry.gather();
        encoder.encode(&mf, &mut buffer)?;
        Ok((buffer, encoder.format_type().to_owned()))
    }
}

#[derive(thiserror::Error, Debug)]
pub enum Error {
    #[error("Already running submission script")]
    AlreadyRunning,
    #[error(transparent)]
    SubmitError(#[from] anyhow::Error),
}

pub async fn run_submit(
    state: &AppState,
    refslot: Option<ReferenceSlot>,
    previous_slot: Option<ReferenceSlot>,
) -> Result<String, Error> {
    match state.run_lock.try_lock() {
        Ok(_) => run_submit_impl(state, refslot, previous_slot).await,
        Err(e) => {
            tracing::debug!("Failed to acquite mutex lock - already running: {e:?}");
            Err(Error::AlreadyRunning)
        }
    }
}

pub async fn run_submit_impl(
    state: &AppState,
    refslot: Option<ReferenceSlot>,
    previous_slot: Option<ReferenceSlot>,
) -> Result<String, Error> {
    state.log_config_important();
    scripts::submit::run(
        &state.script_runtime,
        refslot,
        previous_slot,
        &state.submit_flags,
    )
    .await
    .map(|tx_receipt| {
        tracing::info!(
            "Report transaction complete {:#?}",
            tx_receipt.transaction_hash
        );
        hex::encode(tx_receipt.transaction_hash)
    })
    .map_err(|e| {
        tracing::error!("Failed to submit report {}", e);
        Error::from(e)
    })
}
