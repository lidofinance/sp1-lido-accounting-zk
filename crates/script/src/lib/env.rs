use std::env;
use std::fmt::Debug;
use std::str::FromStr;

#[derive(Clone, Copy)]
pub struct EnvVarSpec {
    pub key: &'static str,
    pub sensitive: bool,
}

#[derive(Clone, Copy)]
pub struct EnvVarValue<TVal> {
    pub spec: &'static EnvVarSpec,
    pub value: TVal,
}

impl EnvVarSpec {
    pub fn default<TVal: FromStr>(&'static self, default: TVal) -> EnvVarValue<TVal> {
        let as_optional = self.optional();
        EnvVarValue {
            spec: as_optional.spec,
            value: as_optional.value.unwrap_or(default),
        }
    }

    pub fn optional<TVal: FromStr>(&'static self) -> EnvVarValue<Option<TVal>> {
        let value = match env::var(self.key) {
            Ok(val) => {
                let parsed = val
                    .parse()
                    .unwrap_or_else(|_e| panic!("Failed to parse env var {}", self.key));
                Some(parsed)
            }
            Err(e) => {
                tracing::debug!("Failed reading env var {}: {e:?}", self.key);
                None
            }
        };
        EnvVarValue { spec: self, value }
    }

    pub fn required<TVal: FromStr>(&'static self) -> EnvVarValue<TVal> {
        let raw_value = env::var(self.key).unwrap_or_else(|e| panic!("Failed to read env var {}: {e:?}", self.key));
        match raw_value.parse() {
            Ok(value) => EnvVarValue { spec: self, value },
            Err(_e) => {
                panic!("Failed to parse value {} for env var {}", raw_value, self.key)
            }
        }
    }

    pub fn map<TVal, Mapper>(&'static self, mapper: Mapper) -> EnvVarValue<TVal>
    where
        Mapper: Fn(&str) -> TVal,
    {
        let raw_value: String =
            env::var(self.key).unwrap_or_else(|e| panic!("Failed to read env var {}: {e:?}", self.key));
        let value = mapper(&raw_value);
        EnvVarValue { spec: self, value }
    }
}

impl<TVal: Debug> Debug for EnvVarValue<TVal> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if self.spec.sensitive {
            f.write_str("***")
        } else {
            f.write_fmt(format_args!("{:?}", self.value))
        }
    }
}

pub const LOG_FORMAT: EnvVarSpec = EnvVarSpec {
    key: "LOG_FORMAT",
    sensitive: false,
};
pub const DRY_RUN: EnvVarSpec = EnvVarSpec {
    key: "DRY_RUN",
    sensitive: false,
};
pub const REPORT_CYCLES: EnvVarSpec = EnvVarSpec {
    key: "REPORT_CYCLES",
    sensitive: false,
};
pub const SERVICE_BIND_TO_ADDR: EnvVarSpec = EnvVarSpec {
    key: "SERVICE_BIND_TO_ADDR",
    sensitive: false,
};
pub const INTERNAL_SCHEDULER: EnvVarSpec = EnvVarSpec {
    key: "INTERNAL_SCHEDULER",
    sensitive: false,
};
pub const INTERNAL_SCHEDULER_CRON: EnvVarSpec = EnvVarSpec {
    key: "INTERNAL_SCHEDULER_CRON",
    sensitive: false,
};
pub const INTERNAL_SCHEDULER_TZ: EnvVarSpec = EnvVarSpec {
    key: "INTERNAL_SCHEDULER_TZ",
    sensitive: false,
};

pub const SP1_FULFILLMENT_STRATEGY: EnvVarSpec = EnvVarSpec {
    key: "SP1_FULFILLMENT_STRATEGY",
    sensitive: false,
};
pub const SP1_SKIP_LOCAL_PROOF_VERIFICATION: EnvVarSpec = EnvVarSpec {
    key: "SP1_SKIP_LOCAL_PROOF_VERIFICATION",
    sensitive: false,
};
pub const NETWORK_PRIVATE_KEY: EnvVarSpec = EnvVarSpec {
    key: "NETWORK_PRIVATE_KEY",
    sensitive: true,
};
pub const NETWORK_RPC_URL: EnvVarSpec = EnvVarSpec {
    key: "NETWORK_RPC_URL",
    sensitive: true,
};
pub const BS_READER_MODE: EnvVarSpec = EnvVarSpec {
    key: "BS_READER_MODE",
    sensitive: false,
};
pub const BS_FILE_STORE: EnvVarSpec = EnvVarSpec {
    key: "BS_FILE_STORE",
    sensitive: false,
};

pub const EVM_CHAIN: EnvVarSpec = EnvVarSpec {
    key: "EVM_CHAIN",
    sensitive: false,
};
pub const EVM_CHAIN_ID: EnvVarSpec = EnvVarSpec {
    key: "EVM_CHAIN_ID",
    sensitive: false,
};
pub const PRIVATE_KEY: EnvVarSpec = EnvVarSpec {
    key: "PRIVATE_KEY",
    sensitive: true,
};
pub const CONTRACT_ADDRESS: EnvVarSpec = EnvVarSpec {
    key: "CONTRACT_ADDRESS",
    sensitive: false,
};
pub const HASH_CONSENSUS_ADDRESS: EnvVarSpec = EnvVarSpec {
    key: "HASH_CONSENSUS_ADDRESS",
    sensitive: false,
};
pub const WITHDRAWAL_VAULT_ADDRESS: EnvVarSpec = EnvVarSpec {
    key: "WITHDRAWAL_VAULT_ADDRESS",
    sensitive: false,
};
pub const LIDO_WIDTHRAWAL_CREDENTIALS: EnvVarSpec = EnvVarSpec {
    key: "LIDO_WIDTHRAWAL_CREDENTIALS",
    sensitive: false,
};

pub const EXECUTION_LAYER_RPC: EnvVarSpec = EnvVarSpec {
    key: "EXECUTION_LAYER_RPC",
    sensitive: true,
};
pub const CONSENSUS_LAYER_RPC: EnvVarSpec = EnvVarSpec {
    key: "CONSENSUS_LAYER_RPC",
    sensitive: true,
};
pub const BEACON_STATE_RPC: EnvVarSpec = EnvVarSpec {
    key: "BEACON_STATE_RPC",
    sensitive: true,
};
pub const PROMETHEUS_NAMESPACE: EnvVarSpec = EnvVarSpec {
    key: "PROMETHEUS_NAMESPACE",
    sensitive: false,
};
