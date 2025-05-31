use std::sync::Once;

use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt, EnvFilter, Layer, Registry};

static INIT: Once = Once::new();

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
    use_json: bool,
    is_test: bool,
    with_thread_names: bool,
}

impl LoggingConfig {
    pub fn use_json(mut self, value: bool) -> Self {
        self.use_json = value;
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
            use_json: false,
            is_test: cfg!(test),
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

        let common_layer = tracing_subscriber::fmt::layer()
            .with_target(false)
            .with_thread_names(config.with_thread_names);

        let fmt_layer = if config.use_json {
            common_layer.json().with_span_list(false).flatten_event(true).boxed()
        } else {
            common_layer.compact().boxed()
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
