use crate::beacon_state_reader::file::FileBasedBeaconStateReader;
use crate::beacon_state_reader::reqwest::{CachedReqwestBeaconStateReader, ReqwestBeaconStateReader};
use crate::beacon_state_reader::{self, BeaconStateReader, RefSlotResolver, StateId};
use crate::consts::{self, NetworkInfo, WrappedNetwork};
use crate::prometheus_metrics::Metrics;
use crate::sp1_client_wrapper::SP1ClientWrapperImpl;
use crate::tracing::LogFormat;
use sp1_lido_accounting_zk_shared::eth_consensus_layer::{BeaconBlockHeader, BeaconState, Hash256};
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
    pub fn new_from_env(network: &impl NetworkInfo) -> Result<BeaconStateReaderEnum, Error> {
        let bs_reader_mode_var = env::var("BS_READER_MODE")?;

        match bs_reader_mode_var.to_lowercase().as_str() {
            "file" => {
                let file_store_location = env::var("BS_FILE_STORE")?;
                let file_store = PathBuf::from(file_store_location).join(network.as_str());
                let file_reader = FileBasedBeaconStateReader::new(&file_store)?;
                Ok(BeaconStateReaderEnum::File(file_reader))
            }
            "rpc" => {
                let rpc_endpoint = env::var("CONSENSUS_LAYER_RPC")?;
                let bs_endpoint = env::var("BEACON_STATE_RPC")?;
                let reqwest_reader = ReqwestBeaconStateReader::new(&rpc_endpoint, &bs_endpoint)?;
                Ok(BeaconStateReaderEnum::RPC(reqwest_reader))
            }
            "rpc_cached" => {
                let file_store_location = env::var("BS_FILE_STORE")?;
                let rpc_endpoint = env::var("CONSENSUS_LAYER_RPC")?;
                let bs_endpoint = env::var("BEACON_STATE_RPC")?;
                let file_store = PathBuf::from(file_store_location).join(network.as_str());
                let cached_reader = CachedReqwestBeaconStateReader::new(&rpc_endpoint, &bs_endpoint, &file_store)?;
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
    pub service_bind_to_addr: EnvVarValue<String>,
    pub internal_scheduler: EnvVarValue<String>,
    pub internal_scheduler_cron: EnvVarValue<String>,
    pub internal_scheduler_tz: EnvVarValue<String>,
    pub sp1_prover: EnvVarValue<String>,
    pub network_private_key: EnvVarValue<String>,
    pub network_rpc_url: EnvVarValue<Option<Url>>,
    pub sp1_verifier_address: EnvVarValue<Address>,
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
            service_bind_to_addr: crate::env::SERVICE_BIND_TO_ADDR.required(),
            internal_scheduler: crate::env::INTERNAL_SCHEDULER.required(),
            internal_scheduler_cron: crate::env::INTERNAL_SCHEDULER_CRON.required(),
            internal_scheduler_tz: crate::env::INTERNAL_SCHEDULER_TZ.required(),
            sp1_prover: crate::env::SP1_PROVER.required(),
            network_private_key: crate::env::NETWORK_PRIVATE_KEY.required(),
            network_rpc_url: crate::env::NETWORK_RPC_URL.optional(),
            sp1_verifier_address: crate::env::SP1_VERIFIER_ADDRESS.required(),
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
            result.insert("service_bind_to_addr", self.service_bind_to_addr.value.clone());
            result.insert("internal_scheduler", self.internal_scheduler.value.clone());
            result.insert("internal_scheduler_cron", self.internal_scheduler_cron.value.clone());
            result.insert("internal_scheduler_tz", self.internal_scheduler_tz.value.clone());
            result.insert("sp1_prover", self.sp1_prover.value.clone());
            result.insert("network_private_key", "<sensitive>".to_string());
            result.insert("network_rpc_url", format!("{:?}", self.network_rpc_url.value));
            result.insert("sp1_verifier_address", format!("{:?}", self.sp1_verifier_address.value));
            result.insert("bs_reader_mode", self.bs_reader_mode.value.clone());
            result.insert("bs_file_store", self.bs_file_store.value.clone());
            result.insert("private_key", "<sensitive>".to_string());
            result.insert("execution_layer_rpc", self.execution_layer_rpc.value.to_string());
            result.insert("consensus_layer_rpc", self.consensus_layer_rpc.value.to_string());
            result.insert("beacon_state_rpc", self.beacon_state_rpc.value.to_string());
            result.insert("prometheus_namespace", self.prometheus_namespace.value.to_string());
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

pub struct Sp1Settings {
    pub verifier_address: Address,
}

pub struct EthInfrastructure {
    pub network: WrappedNetwork,
    pub provider: Arc<DefaultProvider>,
    pub eth_client: EthELClient,
    pub beacon_state_reader: BeaconStateReaderEnum,
}

pub struct Sp1Infrastructure {
    pub sp1_client: SP1ClientWrapperImpl,
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
    pub sp1_settings: Sp1Settings,
    pub metrics: Metrics,
    pub dry_run: bool,
}

impl ScriptRuntime {
    pub fn new(
        eth_infra: EthInfrastructure,
        sp1_infra: Sp1Infrastructure,
        lido_infra: LidoInfrastructure,
        lido_settings: LidoSettings,
        sp1_settings: Sp1Settings,
        metrics: Metrics,
        dry_run: bool,
    ) -> Self {
        Self {
            eth_infra,
            sp1_infra,
            lido_infra,
            lido_settings,
            sp1_settings,
            metrics,
            dry_run,
        }
    }

    pub fn init(env_vars: &EnvVars) -> Result<Self, Error> {
        let provider = Arc::new(ProviderFactory::create_provider_decode_key(
            env_vars.private_key.value.clone(),
            env_vars.execution_layer_rpc.value.clone(),
        )?);
        let network = env_vars.evm_chain.value.clone().parse::<WrappedNetwork>()?;
        let beacon_state_reader = BeaconStateReaderEnum::new_from_env(&network)?;

        let metrics = Metrics::new(&env_vars.prometheus_namespace.value);

        let result = Self::new(
            EthInfrastructure {
                network,
                provider: Arc::clone(&provider),
                eth_client: ExecutionLayerClient::new(Arc::clone(&provider)),
                beacon_state_reader,
            },
            Sp1Infrastructure {
                sp1_client: SP1ClientWrapperImpl::new(ProverClient::from_env()),
            },
            LidoInfrastructure {
                report_contract: Sp1LidoAccountingReportContractWrapper::new(
                    Arc::clone(&provider),
                    env_vars.contract_address.value,
                ),
                hash_consensus_contract: HashConsensusContractWrapper::new(
                    Arc::clone(&provider),
                    env_vars.hash_consensus_address.value,
                    metrics.services.hash_consensus.clone(),
                ),
            },
            LidoSettings {
                withdrawal_credentials: env_vars.lido_widthrawal_credentials.value,
                contract_address: env_vars.contract_address.value,
                withdrawal_vault_address: env_vars.withdrawal_vault_address.value,
                hash_consensus_address: env_vars.hash_consensus_address.value,
            },
            Sp1Settings {
                verifier_address: env_vars.sp1_verifier_address.value,
            },
            metrics,
            env_vars.dry_run.value,
        );
        Ok(result)
    }

    pub fn bs_reader(&self) -> &impl BeaconStateReader {
        &self.eth_infra.beacon_state_reader
    }

    pub fn ref_slot_resolver(&self) -> &impl RefSlotResolver {
        &self.eth_infra.beacon_state_reader
    }

    pub fn network(&self) -> &impl NetworkInfo {
        &self.eth_infra.network
    }

    pub fn is_dry_run(&self) -> bool {
        self.dry_run
    }
}
