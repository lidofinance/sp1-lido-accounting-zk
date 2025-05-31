use common::{setup_prometheus, AppState};

use sp1_lido_accounting_scripts::{scripts, tracing as tracing_config};
use std::{str::FromStr, sync::Arc};
use tokio::sync::Mutex;

mod common;
mod scheduler;
mod server;

#[derive(PartialEq)]
enum LogFormat {
    Plain,
    Json,
}

impl FromStr for LogFormat {
    type Err = ();

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_ascii_lowercase().as_str() {
            "plain" => Ok(LogFormat::Plain),
            "json" => Ok(LogFormat::Json),
            _ => Err(()),
        }
    }
}

fn read_env<T: FromStr>(env_var: &str, default: T) -> T {
    if let Ok(str) = std::env::var(env_var) {
        if let Ok(value) = T::from_str(&str) {
            value
        } else {
            default
        }
    } else {
        default
    }
}

pub async fn service_main() {
    let log_format = read_env("LOG_FORMAT", LogFormat::Plain);

    let log_config = tracing_config::LoggingConfig::default()
        .with_thread_names(true)
        .use_json(log_format == LogFormat::Json);
    // logging setup
    tracing_config::setup_logger(log_config);

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
