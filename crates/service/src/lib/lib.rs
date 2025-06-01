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

    let env_vars = state.script_runtime.env_vars.as_ref();

    // Everything on this span will be appended to all messages
    let main_span = tracing::info_span!(
        "span:main",
        chain = env_vars.map(|v| v.evm_chain.value.clone()),
        chain_id = env_vars.map(|v| v.evm_chain_id.value.clone()),
        prover = env_vars.map(|v| v.sp1_prover.value.clone()),
        dry_run = dry_run,
    );
    let scheduler_span = main_span.clone();
    let service_span = main_span.clone();

    let _entered = main_span.entered();

    state.log_config();

    let shared_state = Arc::new(Mutex::new(state));

    let maybe_scheduler_thread = scheduler::launch(Arc::clone(&shared_state), scheduler_span);
    let server_thread = server::launch(Arc::clone(&shared_state), service_span);

    if let Some(scheduler_thread) = maybe_scheduler_thread {
        scheduler_thread.join().unwrap();
    }
    server_thread.join().unwrap();
    _entered.exit();
}
