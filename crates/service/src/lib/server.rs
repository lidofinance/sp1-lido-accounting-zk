use axum::{
    debug_handler,
    extract::{Json, Query},
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::{get, post},
    Extension, Router,
};
use serde::{Deserialize, Serialize};

use sp1_lido_accounting_scripts::{
    eth_client::Sp1LidoAccountingReportContract::Report, utils::read_env,
};
use sp1_lido_accounting_zk_shared::io::eth_io::{BeaconChainSlot, ReferenceSlot, ReportRust};
use std::{any::type_name_of_val, net::SocketAddr, sync::Arc, thread};
use tracing::{Instrument, Span};

use crate::common::{run_submit, AppState};

#[derive(Deserialize)]
struct RunReportParams {
    target_ref_slot: Option<u64>,
    previous_ref_slot: Option<u64>,
}

#[derive(Deserialize)]
struct GetReportParams {
    target_slot: Option<u64>,
}

pub fn launch(state: Arc<AppState>, parent_span: Span) -> thread::JoinHandle<()> {
    thread::Builder::new()
        .name("server".into())
        .spawn(move || {
            let rt = tokio::runtime::Runtime::new().unwrap();
            rt.block_on(run_server(state, parent_span));
        })
        .unwrap()
}

async fn run_server(state: Arc<AppState>, parent_span: Span) {
    // Build routes
    let app = Router::new()
        .route("/health", get(health))
        .route("/metrics", get(metrics_handler))
        .route("/run-report", post(run_report_handler))
        .route("/get-report", get(get_report_handler))
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

async fn metrics_handler(state: axum::extract::State<Arc<AppState>>) -> impl IntoResponse {
    match state.report_metrics() {
        Ok((buffer, format)) => Response::builder()
            .header("Content-Type", format)
            .body(buffer.into())
            .map(|response| (StatusCode::OK, response))
            .unwrap_or_else(|_| {
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "Failed to create response for metrics".into_response(),
                )
            }),
        Err(_) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            "Failed to collect metrics".into_response(),
        ),
    }
}

#[derive(Serialize, Deserialize)]
enum RunReportResponse {
    Success { tx_hash: String },
    Error { kind: String, message: String },
}

#[derive(Serialize, Deserialize)]
enum GetReportResponse {
    Success { report: ReportRust },
    Error { kind: String, message: String },
}

async fn run_report_handler(
    state: axum::extract::State<Arc<AppState>>,
    Query(params): Query<RunReportParams>,
    Extension(parent_span): Extension<Span>,
) -> (axum::http::StatusCode, axum::Json<RunReportResponse>) {
    async {
        state
            .script_runtime
            .metrics
            .metadata
            .run_report_counter
            .with_label_values(&["http"])
            .inc();

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
                let (kind, message, status_code) = match e {
                    crate::common::Error::AlreadyRunning => (
                        type_name_of_val(&e).to_string(),
                        e.to_string(),
                        StatusCode::TOO_MANY_REQUESTS,
                    ),
                    crate::common::Error::SubmitError(underlying) => (
                        type_name_of_val(&underlying).to_string(),
                        underlying.to_string(),
                        StatusCode::INTERNAL_SERVER_ERROR,
                    ),
                };
                let response_body = RunReportResponse::Error { kind, message };
                (status_code, Json(response_body))
            }
        }
    }
    .instrument(parent_span)
    .await
}

async fn get_report_handler(
    state: axum::extract::State<Arc<AppState>>,
    Query(params): Query<GetReportParams>,
    Extension(parent_span): Extension<Span>,
) -> (axum::http::StatusCode, axum::Json<GetReportResponse>) {
    async {
        let result = get_report_handler_impl(&state, params.target_slot).await;
        match result {
            Ok(report) => {
                let response_body = GetReportResponse::Success { report };
                (StatusCode::OK, Json(response_body))
            }
            Err(e) => {
                let response_body = GetReportResponse::Error {
                    kind: type_name_of_val(&e).to_string(),
                    message: e.to_string(),
                };
                (StatusCode::INTERNAL_SERVER_ERROR, Json(response_body))
            }
        }
    }
    .instrument(parent_span)
    .await
}

async fn get_report_handler_impl(
    state: &AppState,
    target_slot: Option<u64>,
) -> anyhow::Result<ReportRust> {
    let contract = &state.script_runtime.lido_infra.report_contract;
    let target_slot = if let Some(target_slot) = target_slot {
        target_slot
    } else {
        contract.get_latest_validator_state_slot().await?.0
    };

    let result = contract
        .get_report(ReferenceSlot(target_slot))
        .await
        .map_err(|e| anyhow::anyhow!("Error fetching latest report {e:?}"))?;
    Ok(result)
}
