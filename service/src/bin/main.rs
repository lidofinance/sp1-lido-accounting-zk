use axum::{
    extract::{Json, Query},
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::{get, post},
    Router,
};
use prometheus::{Encoder, TextEncoder, Registry, Counter};
use serde::Deserialize;
use std::{net::SocketAddr, sync::Arc};
use tokio::sync::Mutex;

#[derive(Deserialize)]
struct RunReportParams {
    target_ref_slot: Option<u64>,
    previous_ref_slot: Option<u64>,
}

struct AppState {
    registry: Registry,
    run_report_counter: Counter,
}

#[tokio::main]
async fn main() {
    // Prometheus setup
    let registry = Registry::new();
    let run_report_counter = Counter::new("run_report_total", "Total run report requests").unwrap();
    registry.register(Box::new(run_report_counter.clone())).unwrap();

    let state = Arc::new(Mutex::new(AppState {
        registry,
        run_report_counter,
    }));

    // Build routes
    let app = Router::new()
        .route("/health", get(health))
        .route("/metrics", get(metrics))
        .route("/run-report", post(run_report))
        .with_state(state);

    let addr = SocketAddr::from(([0, 0, 0, 0], 8080));
    println!("Listening on {}", addr);
    axum::Server::bind(&addr)
        .serve(app.into_make_service())
        .await
        .unwrap();
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

async fn run_report(
    state: axum::extract::State<Arc<Mutex<AppState>>>,
    Query(params): Query<RunReportParams>,
) -> impl IntoResponse {
    let mut state = state.lock().await;
    state.run_report_counter.inc();

    // Here you would run your report logic using params.target_ref_slot and params.previous_ref_slot
    let response = serde_json::json!({
        "status": "report started",
        "target_ref_slot": params.target_ref_slot,
        "previous_ref_slot": params.previous_ref_slot,
    });

    (StatusCode::OK, Json(response))
}