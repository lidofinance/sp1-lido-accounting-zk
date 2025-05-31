use axum::{
    extract::{Json, Query},
    http::StatusCode,
    response::Response,
    routing::{get, post},
    Router,
};
use chrono::Utc;
use cron::Schedule;
use prometheus::{Encoder, IntCounter, Registry, TextEncoder};
use serde::{Deserialize, Serialize};
use sp1_lido_accounting_scripts::{
    scripts::{self, prelude::ScriptRuntime},
    tracing as tracing_config,
};
use sp1_lido_accounting_zk_shared::io::eth_io::ReferenceSlot;
use std::{any::type_name_of_val, env, net::SocketAddr, sync::Arc, thread};
use tokio::sync::Mutex;
use tokio::time::Duration;
use tracing::Level;

#[derive(Deserialize)]
struct RunReportParams {
    target_ref_slot: Option<u64>,
    previous_ref_slot: Option<u64>,
}

struct PrometheusCounters {
    run_report_counter: IntCounter,
    scheduler_report_counter: IntCounter,
}

struct AppState {
    registry: Registry,
    metric_reporters: PrometheusCounters,
    script_runtime: scripts::prelude::ScriptRuntime,
    submit_flags: scripts::submit::Flags,
}

fn read_bool(env_var: &str, default: bool) -> bool {
    match std::env::var(env_var) {
        Ok(val) => val.to_ascii_lowercase().as_str() == "true",
        Err(_) => default,
    }
}

fn log_app_state_settings(
    runtime: &scripts::prelude::ScriptRuntime,
    flags: &scripts::submit::Flags,
) {
    tracing::event!(
        Level::INFO,
        env_vars = ?runtime.env_vars,
        "Script runtime parameters",
    );
    tracing::event!(
        Level::INFO,
        submit_flags = ?flags,
        "Script flags",
    );
}

async fn scheduler_loop(state: Arc<Mutex<AppState>>, schedule: Schedule, timezone: chrono_tz::Tz) {
    let upcoming = schedule.upcoming(timezone);

    for next in upcoming {
        let now = Utc::now().with_timezone(&timezone);
        let duration = next - now;
        let sleep_duration = duration.to_std().unwrap_or(Duration::from_secs(0));
        tracing::info!(
            "Next run at {} ({} seconds)",
            next,
            sleep_duration.as_secs()
        );

        tokio::time::sleep(sleep_duration).await;
        submit_report(Arc::clone(&state)).await;
    }
}

async fn submit_report(state: Arc<Mutex<AppState>>) {
    let st = state.lock().await;
    st.metric_reporters.scheduler_report_counter.inc();
    let result = run_submit(&st.script_runtime, &st.submit_flags, None, None).await;
    match result {
        Ok(tx_hash) => tracing::info!("Successfully submitted report, txhash: {}", tx_hash),
        Err(e) => tracing::error!("Failed to submit report: {e:?}"),
    }
}

fn launch_scheduler(state: Arc<Mutex<AppState>>) {
    let enabled = read_bool("INTERNAL_SCHEDULER", false);

    if !enabled {
        tracing::info!("Scheduler disabled");
        return;
    }

    tracing::debug!("Scheduler enabled, reading schedule expression");
    // Read cron expression
    let schedule = env::var("INTERNAL_SCHEDULER_CRON")
        .unwrap_or_else(|e| panic!("Failed to read INTERNAL_SCHEDULER_CRON: {e:?}"))
        .parse()
        .unwrap_or_else(|e| panic!("Failed to parse INTERNAL_SCHEDULER_CRON: {e:?}"));

    let tz: chrono_tz::Tz = env::var("INTERNAL_SCHEDULER_TZ")
        .unwrap_or_else(|e| {
            tracing::warn!(
                "Failed to read INTERNAL_SCHEDULER_TZ env var - assuming UTC. Error: {e:?}"
            );
            "UTC".to_owned()
        })
        .parse()
        .unwrap_or_else(|e| panic!("Failed to parse INTERNAL_SCHEDULER_TZ: {e:?}"));

    tracing::info!(
        "Scheduler enabled. Using timezone {} and schedule: {}",
        tz,
        schedule
    );

    // Spawn scheduler thread
    thread::Builder::new()
        .name("scheduler-thread".into())
        .spawn(move || {
            let rt = tokio::runtime::Runtime::new().unwrap();
            rt.block_on(scheduler_loop(state, schedule, tz));
        })
        .unwrap();
}

fn setup_prometheus() -> (Registry, PrometheusCounters) {
    let registry = Registry::new();
    let prometheus_counters = PrometheusCounters {
        run_report_counter: IntCounter::new(
            "run_report_total",
            "Total requests to /run-report endpoint",
        )
        .unwrap(),
        scheduler_report_counter: IntCounter::new(
            "scheduler_report_counter",
            "Total report attempts from scheduler",
        )
        .unwrap(),
    };
    registry
        .register(Box::new(prometheus_counters.run_report_counter.clone()))
        .unwrap();
    registry
        .register(Box::new(
            prometheus_counters.scheduler_report_counter.clone(),
        ))
        .unwrap();

    (registry, prometheus_counters)
}

#[tokio::main]
async fn main() {
    // logging setup
    tracing_config::setup_logger(
        tracing_config::LoggingConfig::default()
            .use_json(true)
            .with_thread_names(true),
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
            store_proof: false,
            store_input: false,
            dry_run,
        },
    };
    log_app_state_settings(&state.script_runtime, &state.submit_flags);

    let shared_state = Arc::new(Mutex::new(state));

    launch_scheduler(Arc::clone(&shared_state));
    launch_server(Arc::clone(&shared_state)).await;
}

async fn launch_server(state: Arc<Mutex<AppState>>) {
    // Build routes
    let app = Router::new()
        .route("/health", get(health))
        .route("/metrics", get(metrics))
        .route("/run-report", post(run_report_handler))
        .with_state(state);

    let addr = SocketAddr::from(([0, 0, 0, 0], 8080));
    let listener = tokio::net::TcpListener::bind(addr).await.unwrap();
    tracing::info!("Starting service at {:?}", addr);
    axum::serve(listener, app).await.unwrap();
}

async fn health() -> &'static str {
    "ok"
}

async fn metrics(state: axum::extract::State<Arc<Mutex<AppState>>>) -> Response {
    let state = state.lock().await;
    let mut buffer = Vec::new();
    let encoder = TextEncoder::new();
    let mf = state.registry.gather();
    encoder.encode(&mf, &mut buffer).unwrap();
    Response::builder()
        .header("Content-Type", encoder.format_type())
        .body(buffer.into())
        .unwrap()
}

#[derive(Serialize, Deserialize)]
enum RunReportResponse {
    Success { tx_hash: String },
    Error { kind: String, message: String },
}

async fn run_report_handler(
    state_extractor: axum::extract::State<Arc<Mutex<AppState>>>,
    Query(params): Query<RunReportParams>,
) -> (axum::http::StatusCode, axum::Json<RunReportResponse>) {
    let state = state_extractor.lock().await;
    state.metric_reporters.run_report_counter.inc();

    let result = run_submit(
        &state.script_runtime,
        &state.submit_flags,
        params.target_ref_slot.map(ReferenceSlot),
        params.previous_ref_slot.map(ReferenceSlot),
    )
    .await;

    match result {
        Ok(tx_hash) => {
            let response_body = RunReportResponse::Success { tx_hash };
            (StatusCode::OK, Json(response_body))
        }
        Err(e) => {
            let response_body = RunReportResponse::Error {
                kind: type_name_of_val(&e).to_string(),
                message: e.to_string(),
            };
            (StatusCode::INTERNAL_SERVER_ERROR, Json(response_body))
        }
    }
}

async fn run_submit(
    runtime: &ScriptRuntime,
    submit_flags: &scripts::submit::Flags,
    refslot: Option<ReferenceSlot>,
    previous_slot: Option<ReferenceSlot>,
) -> Result<String, anyhow::Error> {
    log_app_state_settings(runtime, submit_flags);
    scripts::submit::run(runtime, refslot, previous_slot, submit_flags)
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
