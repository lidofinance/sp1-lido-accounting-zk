use prometheus::{IntCounter, Registry};
use sp1_lido_accounting_scripts::scripts::{self};
use sp1_lido_accounting_zk_shared::io::eth_io::ReferenceSlot;
use tracing::Level;

pub struct AppState {
    pub registry: Registry,
    pub metric_reporters: PrometheusCounters,
    pub script_runtime: scripts::prelude::ScriptRuntime,
    pub submit_flags: scripts::submit::Flags,
}

impl AppState {
    pub fn log_config(&self) {
        tracing::event!(
            Level::INFO,
            env_vars = ?self.script_runtime.env_vars,
            "Script runtime parameters",
        );
        tracing::event!(
            Level::INFO,
            submit_flags = ?self.submit_flags,
            "Script flags",
        );
    }
}

pub struct PrometheusCounters {
    pub run_report_counter: IntCounter,
    pub scheduler_report_counter: IntCounter,
}

pub fn setup_prometheus() -> (Registry, PrometheusCounters) {
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

pub async fn run_submit(
    state: &AppState,
    refslot: Option<ReferenceSlot>,
    previous_slot: Option<ReferenceSlot>,
) -> Result<String, anyhow::Error> {
    state.log_config();
    scripts::submit::run(
        &state.script_runtime,
        refslot,
        previous_slot,
        &state.submit_flags,
    )
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
