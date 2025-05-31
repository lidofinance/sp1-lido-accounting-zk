use common::{setup_prometheus, AppState};

use sp1_lido_accounting_scripts::{scripts, tracing as tracing_config, utils::read_env};
use std::sync::Arc;
use tokio::sync::Mutex;

mod common;
mod scheduler;
mod server;

pub async fn service_main() {
    // logging setup
    tracing_config::setup_logger(
        tracing_config::LoggingConfig::default()
            .with_thread_names(true)
            .use_format(read_env("LOG_FORMAT", tracing_config::LogFormat::Plain)),
    );

    // Prometheus setup
    let (registry, metric_reporters) = setup_prometheus();

    // Initialize script runtime
    let script_runtime = scripts::prelude::ScriptRuntime::init_from_env()
        .expect("Failed to initialize script runtime");
    let dry_run = script_runtime.is_dry_run();

    tracing::info!(dry_run = dry_run, "DRY_RUN: {}", dry_run);

    let state = AppState {
        registry,
        metric_reporters,
        script_runtime,
        submit_flags: scripts::submit::Flags {
            verify: false,
            dry_run,
        },
    };
    state.log_config();

    let shared_state = Arc::new(Mutex::new(state));

    scheduler::launch(Arc::clone(&shared_state));
    server::launch(Arc::clone(&shared_state)).await;
}
