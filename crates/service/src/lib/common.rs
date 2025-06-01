use prometheus::Registry;
use sp1_lido_accounting_scripts::scripts::{self};
use sp1_lido_accounting_zk_shared::io::eth_io::ReferenceSlot;

pub struct AppState {
    pub registry: Registry,
    pub metric_reporters: prometheus_metrics::Metrics,
    pub env_vars: scripts::prelude::EnvVars,
    pub script_runtime: scripts::prelude::ScriptRuntime,
    pub submit_flags: scripts::submit::Flags,
}

impl AppState {
    pub fn log_config_full(&self) {
        tracing::info!(
            env_vars = ?self.env_vars.for_logging(false),
            "Env vars",
        );
        tracing::debug!(
            submit_flags = ?self.submit_flags,
            "Script flags",
        );
    }

    pub fn log_config_important(&self) {
        tracing::info!(
            env_vars = ?self.env_vars.for_logging(true),
            "Env vars",
        );
        tracing::info!(
            submit_flags = ?self.submit_flags,
            "Script flags",
        );
    }
}

pub mod prometheus_metrics {
    use prometheus::{
        Counter, Gauge, GaugeVec, Histogram, HistogramOpts, IntCounter, Opts, Registry,
    };

    pub struct Metrics {
        pub metadata: Metadata,
        report: Report,
        services: Services,
        execution: Execution,
    }

    pub struct Metadata {
        pub network_chain: GaugeVec,
        pub app_build_info: GaugeVec,

        pub run_report_counter: IntCounter,
        pub scheduler_report_counter: IntCounter,
    }

    pub struct Report {
        pub refslot: Gauge,
        pub refslot_epoch: Gauge,
        pub old_slot: Gauge,
        pub timestamp: Gauge,

        pub num_validators: Gauge,
        pub num_lido_validators: Gauge,
        // NOTE: not a typo, cl balance reported in Gwei (10^9 wei), withdrawal vault in Wei.
        pub cl_balance_gwei: Gauge,
        pub withdrawal_vault_balance_wei: Gauge,
        pub state_new_validators: Gauge,
        pub state_changed_validators: Gauge,
    }

    pub struct Service {
        pub call_count: Counter,
        pub retry_count: Gauge,
        pub execution_time_seconds: Histogram,
        pub status_code: Counter,
    }

    pub struct Services {
        pub eth_client: Service,
        pub prover: Service,
        pub beacon_state_client: Service,
    }

    pub struct Execution {
        pub execution_time_seconds: Gauge,
        pub sp1_cycle_count: Gauge,
        pub outcome: Gauge,
    }

    fn register_counter(registry: &Registry, namespace: &str, name: &str, help: &str) -> Counter {
        let opts = Opts::new(name, help).namespace(namespace.to_string());
        let counter = Counter::with_opts(opts).unwrap();
        registry.register(Box::new(counter.clone())).unwrap();
        counter
    }

    fn register_int_counter(
        registry: &Registry,
        namespace: &str,
        name: &str,
        help: &str,
    ) -> IntCounter {
        let opts = Opts::new(name, help).namespace(namespace.to_string());
        let counter = IntCounter::with_opts(opts).unwrap();
        registry.register(Box::new(counter.clone())).unwrap();
        counter
    }

    fn register_gauge(registry: &Registry, namespace: &str, name: &str, help: &str) -> Gauge {
        let opts = Opts::new(name, help).namespace(namespace.to_string());
        let gauge = Gauge::with_opts(opts).unwrap();
        registry.register(Box::new(gauge.clone())).unwrap();
        gauge
    }

    fn register_gauge_vec(
        registry: &Registry,
        namespace: &str,
        name: &str,
        help: &str,
        labels: &[&str],
    ) -> GaugeVec {
        let opts = Opts::new(name, help).namespace(namespace.to_string());
        let gauge = GaugeVec::new(opts, labels).unwrap();
        registry.register(Box::new(gauge.clone())).unwrap();
        gauge
    }

    fn register_histogram(
        registry: &Registry,
        namespace: &str,
        name: &str,
        help: &str,
    ) -> Histogram {
        let opts = HistogramOpts::new(name, help).namespace(namespace.to_string());
        let histogram = Histogram::with_opts(opts).unwrap();
        registry.register(Box::new(histogram.clone())).unwrap();
        histogram
    }

    pub fn setup_prometheus(namespace: &str) -> (Registry, Metrics) {
        let registry = Registry::new();

        let metadata = Metadata {
            network_chain: register_gauge_vec(
                &registry,
                namespace,
                "network_chain",
                "Network Chain ID",
                &["chain_name"],
            ),
            app_build_info: register_gauge_vec(
                &registry,
                namespace,
                "app_build_info",
                "Application Build Info",
                &[
                    "version",
                    "git_sha",
                    "git_branch",
                    "build_timestamp",
                    "target",
                ],
            ),

            run_report_counter: register_int_counter(
                &registry,
                namespace,
                "run_report_total",
                "Number of report runs",
            ),
            scheduler_report_counter: register_int_counter(
                &registry,
                namespace,
                "scheduler_report_total",
                "Number of scheduler reports",
            ),
        };

        let report = Report {
            refslot: register_gauge(&registry, namespace, "refslot", "Current refslot"),
            refslot_epoch: register_gauge(
                &registry,
                namespace,
                "refslot_epoch",
                "Epoch of refslot",
            ),
            old_slot: register_gauge(&registry, namespace, "old_slot", "Oldest slot"),
            timestamp: register_gauge(&registry, namespace, "timestamp", "Timestamp"),

            num_validators: register_gauge(
                &registry,
                namespace,
                "num_validators",
                "Number of validators",
            ),
            num_lido_validators: register_gauge(
                &registry,
                namespace,
                "num_lido_validators",
                "Number of Lido validators",
            ),
            cl_balance_gwei: register_gauge(
                &registry,
                namespace,
                "cl_balance_gwei",
                "CL balance in Gwei",
            ),
            withdrawal_vault_balance_wei: register_gauge(
                &registry,
                namespace,
                "withdrawal_vault_balance_wei",
                "Withdrawal vault balance in Wei",
            ),
            state_new_validators: register_gauge(
                &registry,
                namespace,
                "state_new_validators",
                "New validators",
            ),
            state_changed_validators: register_gauge(
                &registry,
                namespace,
                "state_changed_validators",
                "Changed validators",
            ),
        };

        fn build_service_metrics(registry: &Registry, namespace: &str, component: &str) -> Service {
            Service {
                call_count: register_counter(
                    registry,
                    namespace,
                    &format!("{component}_call_count"),
                    "Total call count",
                ),
                retry_count: register_gauge(
                    registry,
                    namespace,
                    &format!("{component}_retry_count"),
                    "Retry count",
                ),
                execution_time_seconds: register_histogram(
                    registry,
                    namespace,
                    &format!("{component}_execution_time_seconds"),
                    "Execution time in seconds",
                ),
                status_code: register_counter(
                    registry,
                    namespace,
                    &format!("{component}_status_code"),
                    "Status codes",
                ),
            }
        }

        let services = Services {
            eth_client: build_service_metrics(&registry, namespace, "eth_client"),
            prover: build_service_metrics(&registry, namespace, "prover"),
            beacon_state_client: build_service_metrics(&registry, namespace, "beacon_state_client"),
        };

        let execution = Execution {
            execution_time_seconds: register_gauge(
                &registry,
                namespace,
                "execution_time_seconds",
                "Total execution time",
            ),
            sp1_cycle_count: register_gauge(
                &registry,
                namespace,
                "sp1_cycle_count",
                "SP1 cycle count",
            ),
            outcome: register_gauge(
                &registry,
                namespace,
                "execution_outcome",
                "Execution outcome",
            ),
        };

        let metrics = Metrics {
            metadata,
            report,
            services,
            execution,
        };

        (registry, metrics)
    }
}

pub async fn run_submit(
    state: &AppState,
    refslot: Option<ReferenceSlot>,
    previous_slot: Option<ReferenceSlot>,
) -> Result<String, anyhow::Error> {
    state.log_config_important();
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
