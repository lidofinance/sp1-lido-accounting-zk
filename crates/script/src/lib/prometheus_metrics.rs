use anyhow;
use lazy_static::lazy_static;
use prometheus::{
    core::{Atomic, AtomicU64, GenericCounter, GenericCounterVec, GenericGauge, GenericGaugeVec},
    Counter, Gauge, GaugeVec, Histogram, HistogramOpts, HistogramVec, IntCounterVec, IntGauge, Opts, Registry,
};

pub mod outcome {
    pub const REJECTION: &str = "rejection";
    pub const SUCCESS: &str = "success";
    pub const ERROR: &str = "error";
}

pub mod services {
    pub mod hash_consensus {
        pub const GET_REFSLOT: &str = "get_refslot";
    }
}

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

pub type UIntGauge = GenericGauge<AtomicU64>;
pub type UIntGaugeVec = GenericGaugeVec<AtomicU64>;
pub type UIntCounterVec = GenericCounterVec<AtomicU64>;

pub struct Report {
    pub refslot: UIntGauge,
    pub refslot_epoch: UIntGauge,
    pub old_slot: UIntGauge,
    pub timestamp: IntGauge,
    pub num_validators: UIntGauge,
    pub num_lido_validators: UIntGauge,
    pub cl_balance_gwei: UIntGauge,
    pub withdrawal_vault_balance_gwei: UIntGauge,
    pub state_new_validators: UIntGauge,
    pub state_changed_validators: UIntGauge,
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
        registry.register(Box::new(self.withdrawal_vault_balance_gwei.clone()))?;
        registry.register(Box::new(self.state_new_validators.clone()))?;
        registry.register(Box::new(self.state_changed_validators.clone()))?;
        Ok(())
    }
}

#[derive(Clone)]
pub struct Service {
    pub call_count: UIntCounterVec,
    pub retry_count: UIntGaugeVec,
    pub execution_time_seconds: HistogramVec,
    pub status: UIntCounterVec,
}

impl Registar for Service {
    fn register_on(&self, registry: &Registry) -> anyhow::Result<()> {
        registry.register(Box::new(self.call_count.clone()))?;
        registry.register(Box::new(self.retry_count.clone()))?;
        registry.register(Box::new(self.execution_time_seconds.clone()))?;
        registry.register(Box::new(self.status.clone()))?;
        Ok(())
    }
}

pub struct Services {
    pub eth_client: Service,
    pub prover: Service,
    pub beacon_state_client: Service,
    pub hash_consensus: Service,
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
    pub execution_time_seconds: Histogram,
    pub sp1_cycle_count: UIntGauge,
    pub outcome: UIntCounterVec,
}

impl Registar for Execution {
    fn register_on(&self, registry: &Registry) -> anyhow::Result<()> {
        registry.register(Box::new(self.execution_time_seconds.clone()))?;
        registry.register(Box::new(self.sp1_cycle_count.clone()))?;
        registry.register(Box::new(self.outcome.clone()))?;
        Ok(())
    }
}

fn gauge<TVal: Atomic>(namespace: &str, name: &str, help: &str) -> GenericGauge<TVal> {
    let opts = Opts::new(name, help).namespace(namespace.to_string());
    GenericGauge::with_opts(opts).unwrap()
}

fn gauge_vec<TVal: Atomic>(namespace: &str, name: &str, help: &str, labels: &[&str]) -> GenericGaugeVec<TVal> {
    let opts = Opts::new(name, help).namespace(namespace.to_string());
    GenericGaugeVec::new(opts, labels).unwrap()
}

fn counter<TVal: Atomic>(namespace: &str, name: &str, help: &str) -> GenericCounter<TVal> {
    let opts = Opts::new(name, help).namespace(namespace.to_string());
    GenericCounter::with_opts(opts).unwrap()
}

fn counter_vec<TVal: Atomic>(namespace: &str, name: &str, help: &str, labels: &[&str]) -> GenericCounterVec<TVal> {
    let opts = Opts::new(name, help).namespace(namespace.to_string());
    GenericCounterVec::new(opts, labels).unwrap()
}

fn histogram(namespace: &str, name: &str, help: &str) -> Histogram {
    let opts = HistogramOpts::new(name, help).namespace(namespace.to_string());
    Histogram::with_opts(opts).unwrap()
}

fn histogram_vec(namespace: &str, name: &str, help: &str, labels: &[&str]) -> HistogramVec {
    let opts = HistogramOpts::new(name, help).namespace(namespace.to_string());
    HistogramVec::new(opts, labels).unwrap()
}

impl Metrics {
    pub fn new(namespace: &str) -> Self {
        let metadata = Metadata {
            network_chain: gauge_vec(
                namespace,
                "metadata__network_chain",
                "Network Chain ID",
                &["chain_name"],
            ),
            app_build_info: gauge_vec(
                namespace,
                "metadata__app_build_info",
                "Application Build Info",
                &["version", "git_sha", "git_branch", "build_timestamp", "target"],
            ),

            run_report_counter: counter_vec(namespace, "metadata__report_runs", "Number of report runs", &["caller"]),
        };

        let report = Report {
            refslot: gauge(namespace, "report__refslot", "Current refslot"),
            refslot_epoch: gauge(namespace, "report__refslot_epoch", "Epoch of refslot"),
            old_slot: gauge(namespace, "report__old_slot", "Oldest slot"),
            timestamp: gauge(namespace, "report__timestamp", "Timestamp"),

            num_validators: gauge(namespace, "report__num_validators", "Number of validators"),
            num_lido_validators: gauge(namespace, "report__num_lido_validators", "Number of Lido validators"),
            cl_balance_gwei: gauge(namespace, "report__cl_balance_gwei", "CL balance in Gwei"),
            withdrawal_vault_balance_gwei: gauge(
                namespace,
                "report__withdrawal_vault_balance_wei",
                "Withdrawal vault balance in Wei",
            ),
            state_new_validators: gauge(namespace, "report__state_new_validators", "New validators"),
            state_changed_validators: gauge(namespace, "report__state_changed_validators", "Changed validators"),
        };

        fn build_service_metrics(namespace: &str, component: &str) -> Service {
            Service {
                call_count: counter_vec(
                    namespace,
                    &format!("external__{component}__call_count"),
                    "Total call count",
                    &["operation"],
                ),
                retry_count: gauge_vec(
                    namespace,
                    &format!("external__{component}__retry_count"),
                    "Retry count",
                    &["operation"],
                ),
                execution_time_seconds: histogram_vec(
                    namespace,
                    &format!("{component}_execution_time_seconds"),
                    "Execution time in seconds",
                    &["operation"],
                ),
                status: counter_vec(
                    namespace,
                    &format!("external__{component}__status"),
                    "Status codes",
                    &["operation", "status"],
                ),
            }
        }

        let services = Services {
            eth_client: build_service_metrics(namespace, "eth_client"),
            prover: build_service_metrics(namespace, "prover"),
            beacon_state_client: build_service_metrics(namespace, "beacon_state_client"),
            hash_consensus: build_service_metrics(namespace, "beacon_state_client"),
        };

        let execution = Execution {
            execution_time_seconds: histogram(namespace, "execution__execution_time_seconds", "Total execution time"),
            sp1_cycle_count: gauge(namespace, "execution__sp1_cycle_count", "SP1 cycle count"),
            outcome: counter_vec(
                namespace,
                "execution__execution_outcome",
                "Execution outcome",
                &[outcome::ERROR, outcome::SUCCESS, outcome::REJECTION],
            ),
        };

        Metrics {
            metadata,
            report,
            services,
            execution,
        }
    }
}
