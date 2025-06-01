use anyhow;
use prometheus::{Counter, Gauge, GaugeVec, Histogram, HistogramOpts, IntCounter, IntCounterVec, Opts, Registry};

pub trait Registar {
    fn register_on(&self, registry: &Registry) -> anyhow::Result<()>;
}

pub struct Metrics {
    pub metadata: Metadata,
    pub report: Report,
    pub services: Services,
    pub execution: Execution,
}

impl Registar for Metrics {
    fn register_on(&self, registry: &Registry) -> anyhow::Result<()> {
        self.metadata.register_on(registry)?;
        self.report.register_on(registry)?;
        self.services.register_on(registry)?;
        self.execution.register_on(registry)?;
        Ok(())
    }
}

pub struct Metadata {
    pub network_chain: GaugeVec,
    pub app_build_info: GaugeVec,
    pub run_report_counter: IntCounterVec,
}

impl Registar for Metadata {
    fn register_on(&self, registry: &Registry) -> anyhow::Result<()> {
        registry.register(Box::new(self.network_chain.clone()))?;
        registry.register(Box::new(self.app_build_info.clone()))?;
        registry.register(Box::new(self.run_report_counter.clone()))?;
        Ok(())
    }
}

pub struct Report {
    pub refslot: Gauge,
    pub refslot_epoch: Gauge,
    pub old_slot: Gauge,
    pub timestamp: Gauge,
    pub num_validators: Gauge,
    pub num_lido_validators: Gauge,
    pub cl_balance_gwei: Gauge,
    pub withdrawal_vault_balance_wei: Gauge,
    pub state_new_validators: Gauge,
    pub state_changed_validators: Gauge,
}

impl Registar for Report {
    fn register_on(&self, registry: &Registry) -> anyhow::Result<()> {
        registry.register(Box::new(self.refslot.clone()))?;
        registry.register(Box::new(self.refslot_epoch.clone()))?;
        registry.register(Box::new(self.old_slot.clone()))?;
        registry.register(Box::new(self.timestamp.clone()))?;
        registry.register(Box::new(self.num_validators.clone()))?;
        registry.register(Box::new(self.num_lido_validators.clone()))?;
        registry.register(Box::new(self.cl_balance_gwei.clone()))?;
        registry.register(Box::new(self.withdrawal_vault_balance_wei.clone()))?;
        registry.register(Box::new(self.state_new_validators.clone()))?;
        registry.register(Box::new(self.state_changed_validators.clone()))?;
        Ok(())
    }
}

pub struct Service {
    pub call_count: Counter,
    pub retry_count: Gauge,
    pub execution_time_seconds: Histogram,
    pub status_code: Counter,
}

impl Registar for Service {
    fn register_on(&self, registry: &Registry) -> anyhow::Result<()> {
        registry.register(Box::new(self.call_count.clone()))?;
        registry.register(Box::new(self.retry_count.clone()))?;
        registry.register(Box::new(self.execution_time_seconds.clone()))?;
        registry.register(Box::new(self.status_code.clone()))?;
        Ok(())
    }
}

pub struct Services {
    pub eth_client: Service,
    pub prover: Service,
    pub beacon_state_client: Service,
}

impl Registar for Services {
    fn register_on(&self, registry: &Registry) -> anyhow::Result<()> {
        self.eth_client.register_on(registry)?;
        self.prover.register_on(registry)?;
        self.beacon_state_client.register_on(registry)?;
        Ok(())
    }
}

pub struct Execution {
    pub execution_time_seconds: Gauge,
    pub sp1_cycle_count: Gauge,
    pub outcome: Gauge,
}

impl Registar for Execution {
    fn register_on(&self, registry: &Registry) -> anyhow::Result<()> {
        registry.register(Box::new(self.execution_time_seconds.clone()))?;
        registry.register(Box::new(self.sp1_cycle_count.clone()))?;
        registry.register(Box::new(self.outcome.clone()))?;
        Ok(())
    }
}

pub fn register_counter(namespace: &str, name: &str, help: &str) -> Counter {
    let opts = Opts::new(name, help).namespace(namespace.to_string());
    Counter::with_opts(opts).unwrap()
}

pub fn register_int_counter(namespace: &str, name: &str, help: &str) -> IntCounter {
    let opts = Opts::new(name, help).namespace(namespace.to_string());
    IntCounter::with_opts(opts).unwrap()
}

pub fn register_int_counter_vec(namespace: &str, name: &str, help: &str, labels: &[&str]) -> IntCounterVec {
    let opts = Opts::new(name, help).namespace(namespace.to_string());
    IntCounterVec::new(opts, labels).unwrap()
}

pub fn register_gauge(namespace: &str, name: &str, help: &str) -> Gauge {
    let opts = Opts::new(name, help).namespace(namespace.to_string());
    Gauge::with_opts(opts).unwrap()
}

pub fn register_gauge_vec(namespace: &str, name: &str, help: &str, labels: &[&str]) -> GaugeVec {
    let opts = Opts::new(name, help).namespace(namespace.to_string());
    GaugeVec::new(opts, labels).unwrap()
}

pub fn register_histogram(namespace: &str, name: &str, help: &str) -> Histogram {
    let opts = HistogramOpts::new(name, help).namespace(namespace.to_string());
    Histogram::with_opts(opts).unwrap()
}

impl Metrics {
    pub fn new(namespace: &str) -> Self {
        let metadata = Metadata {
            network_chain: register_gauge_vec(
                namespace,
                "metadata__network_chain",
                "Network Chain ID",
                &["chain_name"],
            ),
            app_build_info: register_gauge_vec(
                namespace,
                "metadata__app_build_info",
                "Application Build Info",
                &["version", "git_sha", "git_branch", "build_timestamp", "target"],
            ),

            run_report_counter: register_int_counter_vec(
                namespace,
                "metadata__report_runs",
                "Number of report runs",
                &["caller"],
            ),
        };

        let report = Report {
            refslot: register_gauge(namespace, "report__refslot", "Current refslot"),
            refslot_epoch: register_gauge(namespace, "report__refslot_epoch", "Epoch of refslot"),
            old_slot: register_gauge(namespace, "report__old_slot", "Oldest slot"),
            timestamp: register_gauge(namespace, "report__timestamp", "Timestamp"),

            num_validators: register_gauge(namespace, "report__num_validators", "Number of validators"),
            num_lido_validators: register_gauge(namespace, "report__num_lido_validators", "Number of Lido validators"),
            cl_balance_gwei: register_gauge(namespace, "report__cl_balance_gwei", "CL balance in Gwei"),
            withdrawal_vault_balance_wei: register_gauge(
                namespace,
                "report__withdrawal_vault_balance_wei",
                "Withdrawal vault balance in Wei",
            ),
            state_new_validators: register_gauge(namespace, "report__state_new_validators", "New validators"),
            state_changed_validators: register_gauge(
                namespace,
                "report__state_changed_validators",
                "Changed validators",
            ),
        };

        fn build_service_metrics(namespace: &str, component: &str) -> Service {
            Service {
                call_count: register_counter(
                    namespace,
                    &format!("external__{component}__call_count"),
                    "Total call count",
                ),
                retry_count: register_gauge(namespace, &format!("external__{component}__retry_count"), "Retry count"),
                execution_time_seconds: register_histogram(
                    namespace,
                    &format!("{component}_execution_time_seconds"),
                    "Execution time in seconds",
                ),
                status_code: register_counter(
                    namespace,
                    &format!("external__{component}__status_code"),
                    "Status codes",
                ),
            }
        }

        let services = Services {
            eth_client: build_service_metrics(namespace, "eth_client"),
            prover: build_service_metrics(namespace, "prover"),
            beacon_state_client: build_service_metrics(namespace, "beacon_state_client"),
        };

        let execution = Execution {
            execution_time_seconds: register_gauge(
                namespace,
                "execution__execution_time_seconds",
                "Total execution time",
            ),
            sp1_cycle_count: register_gauge(namespace, "execution__sp1_cycle_count", "SP1 cycle count"),
            outcome: register_gauge(namespace, "execution__execution_outcome", "Execution outcome"),
        };

        Metrics {
            metadata,
            report,
            services,
            execution,
        }
    }
}
