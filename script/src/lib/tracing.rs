use std::sync::Once;

use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt, EnvFilter, Registry};

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
}

impl LoggingConfig {
    pub fn use_json(mut self, value: bool) -> Self {
        self.use_json = value;
        self
    }
}

impl Default for LoggingConfig {
    fn default() -> Self {
        Self {
            apply_sp1_suppressions: true,
            use_json: false,
        }
    }
}

pub fn setup_logger(config: LoggingConfig) {
    INIT.call_once(|| {
        let mut env_filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("off"));
        if config.apply_sp1_suppressions {
            env_filter = append_sp1_directives(env_filter);
        }

        let registry = Registry::default().with(env_filter);

        if config.use_json {
            registry
                .with(
                    tracing_subscriber::fmt::layer()
                        .json()
                        .flatten_event(true)
                        .with_target(false)
                        .with_span_list(false),
                )
                .init();
        } else {
            // Forest format: ForestLayer::default()
            registry.with(tracing_subscriber::fmt::layer().compact()).init();
        };
    });
}
