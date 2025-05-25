use axum::{
    extract::{Json, Query},
    http::StatusCode,
    response::Response,
    routing::{get, post},
    Router,
};
use prometheus::{Counter, Encoder, Registry, TextEncoder};
use serde::{Deserialize, Serialize};
use sp1_lido_accounting_scripts::{
    scripts::{self},
    tracing as tracing_config,
};
use sp1_lido_accounting_zk_shared::io::eth_io::ReferenceSlot;
use std::{any::type_name_of_val, net::SocketAddr, sync::Arc};
use tokio::sync::Mutex;
use tracing::Level;

#[derive(Deserialize)]
struct RunReportParams {
    target_ref_slot: Option<u64>,
    previous_ref_slot: Option<u64>,
}

struct AppState {
    registry: Registry,
    run_report_counter: Counter,
    script_runtime: scripts::prelude::ScriptRuntime,
    submit_flags: scripts::submit::Flags,
}

fn read_bool(env_var: &str, default: bool) -> bool {
    match std::env::var(env_var) {
        Ok(val) => val.to_ascii_lowercase().as_str() == "true",
        Err(_) => default,
    }
}

fn log_app_state_settings(state: &AppState) {
    tracing::event!(
        Level::INFO,
        env_vars = ?state.script_runtime.env_vars,
        "Script runtime parameters",
    );
    tracing::event!(
        Level::INFO,
        submit_flags = ?state.submit_flags,
        "Script flags",
    );
}

#[tokio::main]
async fn main() {
    // logging setup
    tracing_config::setup_logger(tracing_config::LoggingConfig::default().use_json(true));

    // Prometheus setup
    let registry = Registry::new();
    let run_report_counter = Counter::new("run_report_total", "Total run report requests").unwrap();
    registry
        .register(Box::new(run_report_counter.clone()))
        .unwrap();

    // Initialize runtime
    let script_runtime = scripts::prelude::ScriptRuntime::init_from_env()
        .expect("Failed to initialize script runtime");
    let dry_run = script_runtime.is_dry_run();

    tracing::info!(dry_run = dry_run, "DRY_RUN: {}", dry_run);

    let state = AppState {
        registry,
        run_report_counter,
        script_runtime,
        submit_flags: scripts::submit::Flags {
            verify: false,
            store_proof: false,
            store_input: false,
            dry_run,
        },
    };
    log_app_state_settings(&state);

    let shared_state = Arc::new(Mutex::new(state));

    // Build routes
    let app = Router::new()
        .route("/health", get(health))
        .route("/metrics", get(metrics))
        .route("/run-report", post(run_report))
        .with_state(shared_state);

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

async fn run_report(
    state: axum::extract::State<Arc<Mutex<AppState>>>,
    Query(params): Query<RunReportParams>,
) -> (axum::http::StatusCode, axum::Json<RunReportResponse>) {
    let state = state.lock().await;
    state.run_report_counter.inc();

    log_app_state_settings(&state);

    let submit_result = scripts::submit::run(
        &state.script_runtime,
        params.target_ref_slot.map(ReferenceSlot),
        params.previous_ref_slot.map(ReferenceSlot),
        &state.submit_flags,
    )
    .await;

    match submit_result {
        Ok(tx_hash) => {
            let tx_hash_str = hex::encode(tx_hash);
            tracing::info!("Report transaction complete {}", tx_hash_str);
            let response_body = RunReportResponse::Success {
                tx_hash: tx_hash_str,
            };
            (StatusCode::OK, Json(response_body))
        }
        Err(e) => {
            tracing::error!("Failed to submit report {}", e);
            let response_body = RunReportResponse::Error {
                kind: type_name_of_val(&e).to_string(),
                message: e.to_string(),
            };
            (StatusCode::INTERNAL_SERVER_ERROR, Json(response_body))
        }
    }
}
