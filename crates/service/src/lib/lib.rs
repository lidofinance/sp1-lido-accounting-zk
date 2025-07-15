use common::AppState;

use prometheus::Registry;
use sp1_lido_accounting_scripts::{
    prometheus_metrics::Registar,
    scripts::{self, prelude::EnvVars},
    tracing as tracing_config,
    utils::read_env,
};
use std::sync::Arc;

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

    let env_vars = EnvVars::init_from_env_or_crash();

    // Prometheus setup
    let registry = Registry::new();

    // Initialize script runtime
    let script_runtime = scripts::prelude::ScriptRuntime::init(&env_vars)
        .unwrap_or_else(|e| panic!("Failed to initialize script runtime {e:?}"));
    script_runtime
        .metrics
        .register_on(&registry)
        .unwrap_or_else(|e| panic!("Failed to create metrics {e:?}"));
    let dry_run = script_runtime.flags.dry_run;
    let report_cycles = script_runtime.flags.report_cycles;

    tracing::info!(dry_run = dry_run, "DRY_RUN: {}", dry_run);

    let state = AppState {
        registry,
        env_vars,
        script_runtime,
        submit_flags: scripts::submit::Flags {
            verify_input: true,
            verify_proof: false,
            dry_run,
            report_cycles,
        },
        run_lock: Arc::new(tokio::sync::Mutex::new(())),
    };

    let env_vars_ref = &state.env_vars;

    // Everything on this span will be appended to all messages
    let main_span = tracing::info_span!(
        "main",
        chain = env_vars_ref.evm_chain.value.clone(),
        chain_id = env_vars_ref.evm_chain_id.value.clone(),
        prover = "network",
        dry_run = dry_run,
    );
    let scheduler_span = main_span.clone();
    let service_span = main_span.clone();

    let _entered = main_span.entered();

    state.log_config_full();

    state
        .script_runtime
        .metrics
        .metadata
        .network_chain
        .with_label_values(&[&env_vars_ref.evm_chain.value])
        .set(1.0);
    state
        .script_runtime
        .metrics
        .metadata
        .app_build_info
        .with_label_values(&[
            env!("CARGO_PKG_VERSION"),
            env!("VERGEN_GIT_SHA"),
            env!("VERGEN_BUILD_TIMESTAMP"),
        ])
        .set(1.0);

    let shared_state = Arc::new(state);

    let maybe_scheduler_thread = scheduler::launch(Arc::clone(&shared_state), scheduler_span);
    let server_thread = server::launch(Arc::clone(&shared_state), service_span);

    if let Some(scheduler_thread) = maybe_scheduler_thread {
        scheduler_thread.join().unwrap();
    }
    server_thread.join().unwrap();
    _entered.exit();
}
