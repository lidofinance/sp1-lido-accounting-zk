use std::sync::Once;

use derive_more::FromStr;
use json_subscriber;
use tracing_subscriber::{layer::Layer, registry::Registry, util::SubscriberInitExt};
use tracing_subscriber::{layer::SubscriberExt, EnvFilter};

static INIT: Once = Once::new();

#[derive(Debug, Clone, PartialEq, FromStr)]
pub enum LogFormat {
    Plain,
    Json,
}

fn append_sp1_directives(env_filter: EnvFilter) -> EnvFilter {
    env_filter
        .add_directive("hyper=off".parse().unwrap())
        .add_directive("p3_keccak_air=off".parse().unwrap())
        .add_directive("p3_fri=off".parse().unwrap())
        .add_directive("p3_dft=off".parse().unwrap())
        .add_directive("p3_challenger=off".parse().unwrap())
        .add_directive("sp1_cuda=off".parse().unwrap())
}

pub struct LoggingConfig {
    apply_sp1_suppressions: bool,
    format: LogFormat,
    is_test: bool,
    with_thread_names: bool,
}

impl LoggingConfig {
    pub fn default_for_test() -> Self {
        Self {
            apply_sp1_suppressions: true,
            format: LogFormat::Plain,
            is_test: true,
            with_thread_names: false,
        }
    }

    pub fn use_format(mut self, value: LogFormat) -> Self {
        self.format = value;
        self
    }
    pub fn is_test(mut self, value: bool) -> Self {
        self.is_test = value;
        self
    }
    pub fn with_thread_names(mut self, value: bool) -> Self {
        self.with_thread_names = value;
        self
    }
}

impl Default for LoggingConfig {
    fn default() -> Self {
        Self {
            apply_sp1_suppressions: true,
            format: LogFormat::Plain,
            is_test: false,
            with_thread_names: false,
        }
    }
}

pub fn setup_logger(config: LoggingConfig) {
    INIT.call_once(|| {
        let mut env_filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("off"));
        if config.apply_sp1_suppressions {
            env_filter = append_sp1_directives(env_filter);
        }

        let fmt_layer = match config.format {
            LogFormat::Json => json_subscriber::layer()
                .with_target(true)
                .with_thread_names(config.with_thread_names)
                .with_current_span(false)
                .with_span_list(false)
                .flatten_span_list_on_top_level(true)
                .flatten_event(true)
                .boxed(),
            LogFormat::Plain => tracing_subscriber::fmt::layer()
                .compact()
                .with_target(true)
                .with_thread_names(config.with_thread_names)
                .boxed(),
        };

        let test_layer = if config.is_test {
            Some(tracing_subscriber::fmt::layer().compact().with_test_writer())
        } else {
            None
        };

        let registry = Registry::default().with(env_filter).with(fmt_layer).with(test_layer);
        registry.init();
    });
}
