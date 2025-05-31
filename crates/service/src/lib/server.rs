use axum::{
    extract::{Json, Query},
    http::StatusCode,
    response::Response,
    routing::{get, post},
    Extension, Router,
};
use prometheus::{Encoder, TextEncoder};
use serde::{Deserialize, Serialize};

use sp1_lido_accounting_scripts::utils::read_env;
use sp1_lido_accounting_zk_shared::io::eth_io::ReferenceSlot;
use std::{any::type_name_of_val, net::SocketAddr, sync::Arc, thread};
use tokio::sync::Mutex;
use tower_http::trace::{DefaultMakeSpan, TraceLayer};
use tracing::Span;

use crate::common::{run_submit, AppState};

#[derive(Deserialize)]
struct RunReportParams {
    target_ref_slot: Option<u64>,
    previous_ref_slot: Option<u64>,
}

pub fn launch(state: Arc<Mutex<AppState>>, parent_span: Span) -> thread::JoinHandle<()> {
    thread::Builder::new()
        .name("server".into())
        .spawn(move || {
            let rt = tokio::runtime::Runtime::new().unwrap();
            rt.block_on(run_server(state, parent_span));
        })
        .unwrap()
}

async fn run_server(state: Arc<Mutex<AppState>>, parent_span: Span) {
    // Build routes
    let app = Router::new()
        .route("/health", get(health))
        .route("/metrics", get(metrics))
        .route("/run-report", post(run_report_handler))
        .layer(Extension(parent_span.clone()))
        .with_state(state);

    let addr = read_env(
        "SERVICE_BIND_TO_ADDR",
        SocketAddr::from(([0, 0, 0, 0], 8080)),
    );
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
    Extension(parent_span): Extension<Span>,
) -> (axum::http::StatusCode, axum::Json<RunReportResponse>) {
    let _entered = parent_span.enter();
    let state = state_extractor.lock().await;
    state.metric_reporters.run_report_counter.inc();

    let result = run_submit(
        &state,
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
