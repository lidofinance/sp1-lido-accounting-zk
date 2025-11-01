use crate::beacon_state_reader::file::FileBasedBeaconStateReader;
use crate::beacon_state_reader::reqwest::{CachedReqwestBeaconStateReader, ReqwestBeaconStateReader};
use crate::beacon_state_reader::{self, BeaconStateReader, RefSlotResolver, StateId};
use crate::consts::{self, NetworkInfo, WrappedNetwork};
use crate::prometheus_metrics::{self, Metrics};
use crate::sp1_client_wrapper::SP1ClientWrapperImpl;
use crate::tracing::LogFormat;
use sp1_lido_accounting_zk_shared::eth_consensus_layer::{BeaconBlockHeader, BeaconState, Hash256};
use sp1_sdk::network::FulfillmentStrategy;
use sp1_sdk::ProverClient;

use crate::env::EnvVarValue;
use crate::eth_client::{
    DefaultProvider, EthELClient, ExecutionLayerClient, HashConsensusContract, HashConsensusContractWrapper,
    ProviderFactory, ReportContract, Sp1LidoAccountingReportContractWrapper,
};
use alloy::primitives::Address;

use std::collections::HashMap;
use std::env;
use std::path::PathBuf;
use std::sync::Arc;
use thiserror::Error;

use alloy::transports::http::reqwest::Url;

const DEFAULT_DRY_RUN: bool = true; // Fail close
const DEFAULT_REPORT_CYCLES: bool = false; // Reporting cycles causes higher load on the current server
const DEFAULT_PROMETHEUS_NAMESPACE: &str = "zk_accounting_sp1";

#[derive(Debug, Error)]
pub enum Error {
    #[error("Failed to read env var {0:?}")]
    FailedToReadEnvVar(#[from] std::env::VarError),

    #[error("Failed to read network from env var: {0:?}")]
    FailedToParseNetwork(#[from] consts::NetworkParseError),

    #[error("Failed to create beacon state reader: {0:?}")]
    FailedToCreateBeaconState(#[from] beacon_state_reader::InitializationError),

    #[error("Setting {name}: unknown value {value}")]
    UnknownSetting { name: String, value: String },

    #[error("Failed to create EL provider")]
    ELProviderError(#[from] crate::eth_client::ProviderError),
}

pub enum BeaconStateReaderEnum {
    File(FileBasedBeaconStateReader),
    RPC(ReqwestBeaconStateReader),
    RPCCached(CachedReqwestBeaconStateReader),
}

impl BeaconStateReaderEnum {
    pub fn new_from_env(
        network: &impl NetworkInfo,
        metric_reporter: Arc<prometheus_metrics::Service>,
    ) -> Result<BeaconStateReaderEnum, Error> {
        let bs_reader_mode_var = env::var("BS_READER_MODE")?;

        match bs_reader_mode_var.to_lowercase().as_str() {
            "file" => {
                let file_store_location = env::var("BS_FILE_STORE")?;
                let file_store = PathBuf::from(file_store_location).join(network.as_str());
                let file_reader = FileBasedBeaconStateReader::new(&file_store, Arc::clone(&metric_reporter))?;
                Ok(BeaconStateReaderEnum::File(file_reader))
            }
            "rpc" => {
                let rpc_endpoint = env::var("CONSENSUS_LAYER_RPC")?;
                let bs_endpoint = env::var("BEACON_STATE_RPC")?;
                let reqwest_reader =
                    ReqwestBeaconStateReader::new(&rpc_endpoint, &bs_endpoint, Arc::clone(&metric_reporter))?;
                Ok(BeaconStateReaderEnum::RPC(reqwest_reader))
            }
            "rpc_cached" => {
                let file_store_location = env::var("BS_FILE_STORE")?;
                let rpc_endpoint = env::var("CONSENSUS_LAYER_RPC")?;
                let bs_endpoint = env::var("BEACON_STATE_RPC")?;
                let file_store = PathBuf::from(file_store_location).join(network.as_str());
                let cached_reader = CachedReqwestBeaconStateReader::new(
                    &rpc_endpoint,
                    &bs_endpoint,
                    &file_store,
                    &[],
                    Arc::clone(&metric_reporter),
                )?;
                Ok(BeaconStateReaderEnum::RPCCached(cached_reader))
            }
            unknown_value => Err(Error::UnknownSetting {
                name: "BS_READER_MODE".to_string(),
                value: unknown_value.to_string(),
            }),
        }
    }
}

impl BeaconStateReader for BeaconStateReaderEnum {
    async fn read_beacon_state(&self, state_id: &StateId) -> anyhow::Result<BeaconState> {
        match self {
            Self::File(reader) => reader.read_beacon_state(state_id).await,
            Self::RPC(reader) => reader.read_beacon_state(state_id).await,
            Self::RPCCached(reader) => reader.read_beacon_state(state_id).await,
        }
    }

    async fn read_beacon_block_header(&self, state_id: &StateId) -> anyhow::Result<BeaconBlockHeader> {
        match self {
            Self::File(reader) => reader.read_beacon_block_header(state_id).await,
            Self::RPC(reader) => reader.read_beacon_block_header(state_id).await,
            Self::RPCCached(reader) => reader.read_beacon_block_header(state_id).await,
        }
    }
}

impl RefSlotResolver for BeaconStateReaderEnum {
    async fn find_bc_slot_for_refslot(
        &self,
        target_slot: sp1_lido_accounting_zk_shared::io::eth_io::ReferenceSlot,
    ) -> anyhow::Result<sp1_lido_accounting_zk_shared::io::eth_io::BeaconChainSlot> {
        match self {
            Self::File(_) => {
                panic!("File-based BS reader does not support resolving ref slot to beacon chain slot")
            }
            Self::RPC(reader) => reader.find_bc_slot_for_refslot(target_slot).await,
            Self::RPCCached(reader) => reader.find_bc_slot_for_refslot(target_slot).await,
        }
    }

    async fn is_finalized_slot(
        &self,
        target_slot: sp1_lido_accounting_zk_shared::io::eth_io::BeaconChainSlot,
    ) -> anyhow::Result<bool> {
        match self {
            Self::File(_) => {
                panic!("File-based BS reader does not support checking slot finality")
            }
            Self::RPC(reader) => reader.is_finalized_slot(target_slot).await,
            Self::RPCCached(reader) => reader.is_finalized_slot(target_slot).await,
        }
    }
}
#[derive(Debug, Clone)]
pub struct EnvVars {
    pub log_format: EnvVarValue<LogFormat>,
    pub dry_run: EnvVarValue<bool>,
    pub report_cycles: EnvVarValue<bool>,
    pub service_bind_to_addr: EnvVarValue<String>,
    pub internal_scheduler: EnvVarValue<String>,
    pub internal_scheduler_cron: EnvVarValue<String>,
    pub internal_scheduler_tz: EnvVarValue<String>,
    pub strategy: EnvVarValue<FulfillmentStrategy>,
    pub network_private_key: EnvVarValue<String>,
    pub network_rpc_url: EnvVarValue<Option<Url>>,
    pub bs_reader_mode: EnvVarValue<String>,
    pub bs_file_store: EnvVarValue<String>,
    pub evm_chain: EnvVarValue<String>,
    pub evm_chain_id: EnvVarValue<String>,
    pub private_key: EnvVarValue<String>,
    pub contract_address: EnvVarValue<Address>,
    pub hash_consensus_address: EnvVarValue<Address>,
    pub withdrawal_vault_address: EnvVarValue<Address>,
    pub lido_widthrawal_credentials: EnvVarValue<Hash256>,

    pub execution_layer_rpc: EnvVarValue<Url>,
    pub consensus_layer_rpc: EnvVarValue<Url>,
    pub beacon_state_rpc: EnvVarValue<Url>,

    pub prometheus_namespace: EnvVarValue<String>,
}

impl EnvVars {
    pub fn init_from_env_or_crash() -> Self {
        Self {
            log_format: crate::env::LOG_FORMAT.required(),
            dry_run: crate::env::DRY_RUN.default(DEFAULT_DRY_RUN),
            report_cycles: crate::env::REPORT_CYCLES.default(DEFAULT_REPORT_CYCLES),
            service_bind_to_addr: crate::env::SERVICE_BIND_TO_ADDR.required(),
            internal_scheduler: crate::env::INTERNAL_SCHEDULER.required(),
            internal_scheduler_cron: crate::env::INTERNAL_SCHEDULER_CRON.required(),
            internal_scheduler_tz: crate::env::INTERNAL_SCHEDULER_TZ.required(),
            strategy: crate::env::SP1_FULFILLMENT_STRATEGY.map(|raw| match FulfillmentStrategy::from_str_name(raw) {
                Some(val) => val,
                None => panic!("Couldn't parse SP1_FULFILLMENT_STRATEGY: {raw}"),
            }),
            network_private_key: crate::env::NETWORK_PRIVATE_KEY.required(),
            network_rpc_url: crate::env::NETWORK_RPC_URL.optional(),
            bs_reader_mode: crate::env::BS_READER_MODE.required(),
            bs_file_store: crate::env::BS_FILE_STORE.required(),
            evm_chain: crate::env::EVM_CHAIN.required(),
            evm_chain_id: crate::env::EVM_CHAIN_ID.required(),
            private_key: crate::env::PRIVATE_KEY.required(),
            contract_address: crate::env::CONTRACT_ADDRESS.required(),
            hash_consensus_address: crate::env::HASH_CONSENSUS_ADDRESS.required(),
            withdrawal_vault_address: crate::env::WITHDRAWAL_VAULT_ADDRESS.required(),
            lido_widthrawal_credentials: crate::env::LIDO_WIDTHRAWAL_CREDENTIALS.required(),
            execution_layer_rpc: crate::env::EXECUTION_LAYER_RPC.required(),
            consensus_layer_rpc: crate::env::CONSENSUS_LAYER_RPC.required(),
            beacon_state_rpc: crate::env::BEACON_STATE_RPC.required(),
            prometheus_namespace: crate::env::PROMETHEUS_NAMESPACE.default(DEFAULT_PROMETHEUS_NAMESPACE.to_owned()),
        }
    }

    pub fn for_logging(&self, only_important: bool) -> HashMap<&'static str, String> {
        let mut result = HashMap::new();

        // Always log these
        result.insert("log_format", format!("{:?}", self.log_format.value));
        result.insert("dry_run", self.dry_run.value.to_string());
        result.insert("evm_chain", self.evm_chain.value.clone());
        result.insert("evm_chain_id", self.evm_chain_id.value.clone());
        result.insert("contract_address", format!("{:?}", self.contract_address.value));
        result.insert(
            "withdrawal_vault_address",
            format!("{:?}", self.withdrawal_vault_address.value),
        );
        result.insert(
            "hash_consensus_address",
            format!("{:?}", self.hash_consensus_address.value),
        );
        result.insert(
            "lido_widthrawal_credentials",
            format!("{:?}", self.lido_widthrawal_credentials.value),
        );

        if !only_important {
            result.insert("dry_run", format!("{:?}", self.dry_run));
            result.insert("report_cycles", format!("{:?}", self.report_cycles));
            result.insert("internal_scheduler", format!("{:?}", self.internal_scheduler));
            result.insert("internal_scheduler_cron", format!("{:?}", self.internal_scheduler_cron));
            result.insert("internal_scheduler_tz", format!("{:?}", self.internal_scheduler_tz));
            result.insert("fulfillment_strategy", format!("{:?}", self.strategy.value));
            result.insert("network_private_key", format!("{:?}", self.network_private_key));
            result.insert("network_rpc_url", format!("{:?}", self.network_rpc_url));
            result.insert("bs_reader_mode", format!("{:?}", self.bs_reader_mode));
            result.insert("bs_file_store", format!("{:?}", self.bs_file_store));
            result.insert("private_key", format!("{:?}", self.private_key));
            result.insert("execution_layer_rpc", format!("{:?}", self.execution_layer_rpc));
            result.insert("consensus_layer_rpc", format!("{:?}", self.consensus_layer_rpc));
            result.insert("beacon_state_rpc", format!("{:?}", self.beacon_state_rpc));
            result.insert("prometheus_namespace", format!("{:?}", self.prometheus_namespace));
        }

        result
    }
}

pub struct LidoSettings {
    pub withdrawal_credentials: Hash256,
    pub contract_address: Address,
    pub withdrawal_vault_address: Address,
    pub hash_consensus_address: Address,
}

pub struct EthInfrastructure {
    pub network: WrappedNetwork,
    pub provider: Arc<DefaultProvider>,
    pub eth_client: EthELClient,
    pub beacon_state_reader: Arc<BeaconStateReaderEnum>,
}

pub struct Sp1Infrastructure {
    pub sp1_client: Arc<SP1ClientWrapperImpl>,
}

pub struct LidoInfrastructure {
    pub report_contract: ReportContract,
    pub hash_consensus_contract: HashConsensusContract,
}

pub struct ScriptRuntime {
    pub eth_infra: EthInfrastructure,
    pub sp1_infra: Sp1Infrastructure,
    pub lido_infra: LidoInfrastructure,
    pub lido_settings: LidoSettings,
    pub metrics: Arc<Metrics>,
    pub flags: Flags,
}

pub struct Flags {
    pub dry_run: bool,
    pub report_cycles: bool,
}

impl ScriptRuntime {
    pub fn new(
        eth_infra: EthInfrastructure,
        sp1_infra: Sp1Infrastructure,
        lido_infra: LidoInfrastructure,
        lido_settings: LidoSettings,
        metrics: Arc<Metrics>,
        flags: Flags,
    ) -> Self {
        Self {
            eth_infra,
            sp1_infra,
            lido_infra,
            lido_settings,
            metrics,
            flags,
        }
    }

    pub fn init(env_vars: &EnvVars) -> Result<Self, Error> {
        let provider = Arc::new(ProviderFactory::create_provider_decode_key(
            env_vars.private_key.value.clone(),
            env_vars.execution_layer_rpc.value.clone(),
        )?);

        let metrics = Arc::new(Metrics::new(&env_vars.prometheus_namespace.value));

        let network = env_vars.evm_chain.value.clone().parse::<WrappedNetwork>()?;
        let beacon_state_reader = Arc::new(BeaconStateReaderEnum::new_from_env(
            &network,
            Arc::clone(&metrics.services.beacon_state_client),
        )?);

        let sp1_client = Arc::new(SP1ClientWrapperImpl::new(
            ProverClient::builder().network().build(),
            env_vars.strategy.value,
            Arc::clone(&metrics.services.sp1_client),
        ));

        let result = Self::new(
            EthInfrastructure {
                network,
                provider: Arc::clone(&provider),
                eth_client: ExecutionLayerClient::new(Arc::clone(&provider), Arc::clone(&metrics.services.eth_client)),
                beacon_state_reader,
            },
            Sp1Infrastructure { sp1_client },
            LidoInfrastructure {
                report_contract: Sp1LidoAccountingReportContractWrapper::new(
                    Arc::clone(&provider),
                    env_vars.contract_address.value,
                ),
                hash_consensus_contract: HashConsensusContractWrapper::new(
                    Arc::clone(&provider),
                    env_vars.hash_consensus_address.value,
                    Arc::clone(&metrics.services.hash_consensus),
                ),
            },
            LidoSettings {
                withdrawal_credentials: env_vars.lido_widthrawal_credentials.value,
                contract_address: env_vars.contract_address.value,
                withdrawal_vault_address: env_vars.withdrawal_vault_address.value,
                hash_consensus_address: env_vars.hash_consensus_address.value,
            },
            metrics,
            Flags {
                dry_run: env_vars.dry_run.value,
                report_cycles: env_vars.report_cycles.value,
            },
        );
        Ok(result)
    }

    pub fn bs_reader(&self) -> Arc<impl BeaconStateReader> {
        self.eth_infra.beacon_state_reader.clone()
    }

    pub fn ref_slot_resolver(&self) -> Arc<impl RefSlotResolver> {
        self.eth_infra.beacon_state_reader.clone()
    }

    pub fn network(&self) -> &impl NetworkInfo {
        &self.eth_infra.network
    }

    pub fn is_dry_run(&self) -> bool {
        self.flags.dry_run
    }
}
