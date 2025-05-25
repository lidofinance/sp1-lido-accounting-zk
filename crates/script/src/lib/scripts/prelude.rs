use crate::beacon_state_reader::file::FileBasedBeaconStateReader;
use crate::beacon_state_reader::reqwest::{CachedReqwestBeaconStateReader, ReqwestBeaconStateReader};
use crate::beacon_state_reader::{self, BeaconStateReader, RefSlotResolver, StateId};
use crate::consts::{self, NetworkInfo, WrappedNetwork};
use crate::sp1_client_wrapper::SP1ClientWrapperImpl;
use sp1_lido_accounting_zk_shared::eth_consensus_layer::{BeaconBlockHeader, BeaconState, Hash256};
use sp1_sdk::ProverClient;

use crate::eth_client::{
    DefaultProvider, EthELClient, ExecutionLayerClient, HashConsensusContract, HashConsensusContractWrapper,
    ProviderFactory, ReportContract, Sp1LidoAccountingReportContractWrapper,
};
use alloy::primitives::Address;

use std::env::{self, VarError};
use std::path::PathBuf;
use std::sync::Arc;
use thiserror::Error;

use alloy::transports::http::reqwest::Url;

const DEFAULT_DRY_RUN: bool = true; // Fail close

#[derive(Debug, Error)]
pub enum Error {
    #[error("Failed to read env var {0:?}")]
    FailedToReadEnvVar(VarError),

    #[error("Failed to read network from env var: {0:?}")]
    FailedToParseNetwork(#[from] consts::NetworkParseError),

    #[error("Failed to create beacon state reader: {0:?}")]
    FailedToCreateBeaconState(#[from] beacon_state_reader::Error),

    #[error("Failed to parse URL {0}")]
    FailedToParseUrl(String),

    #[error("Setting {name}: unknown value {value}")]
    UnknownSetting { name: String, value: String },
}

impl From<VarError> for Error {
    fn from(err: VarError) -> Self {
        Error::FailedToReadEnvVar(err)
    }
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
                let reqwest_reader = ReqwestBeaconStateReader::new(&rpc_endpoint, &bs_endpoint);
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
}

pub mod env_vars {
    use std::env;
    use std::fmt::Debug;

    #[derive(Clone, Copy)]
    pub struct EnvVarValue<TVal> {
        pub name: &'static str,
        pub sensitive: bool,
        pub value: TVal,
    }

    impl<TVal: Debug> Debug for EnvVarValue<TVal> {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            let value_print = if self.sensitive {
                "***".to_string()
            } else {
                format!("{:?}", self.value)
            };
            f.debug_struct("EnvVarValue")
                .field("name", &self.name)
                .field("value", &value_print)
                .finish()
        }
    }

    #[derive(Debug, Clone)]
    pub struct EnvVars {
        pub evm_chain: EnvVarValue<String>,
        pub bs_reader_mode: EnvVarValue<String>,
        pub execution_layer_rpc: EnvVarValue<String>,
        pub consensus_layer_rpc: EnvVarValue<String>,
        pub beacon_state_rpc: EnvVarValue<String>,
        pub contract_address: EnvVarValue<String>,
        pub hash_consensus_address: EnvVarValue<String>,
        pub withdrawal_credentials: EnvVarValue<String>,
        pub withdrawal_vault_address: EnvVarValue<String>,

        pub sp1_prover: EnvVarValue<String>,
        pub sp1_verifier: EnvVarValue<String>,
        pub network_rpc_url: EnvVarValue<Option<String>>,

        pub dry_run: EnvVarValue<Option<String>>,
        // sensitive
        pub ethereum_private_key: EnvVarValue<String>,
        pub network_private_key: EnvVarValue<String>,
    }

    impl EnvVars {
        fn optional(key: &'static str, sensitive: bool) -> EnvVarValue<Option<String>> {
            let value = match env::var(key) {
                Ok(value) => Some(value),
                Err(_) => None,
            };
            EnvVarValue {
                name: key,
                sensitive,
                value,
            }
        }

        fn required(key: &'static str, sensitive: bool) -> EnvVarValue<String> {
            let value = env::var(key).unwrap_or_else(|e| panic!("Failed to read env var {key}: {e:?}"));
            EnvVarValue {
                name: key,
                sensitive,
                value,
            }
        }

        pub fn init_from_env() -> Self {
            Self {
                evm_chain: Self::required("EVM_CHAIN", false),
                bs_reader_mode: Self::required("BS_READER_MODE", false),
                execution_layer_rpc: Self::required("EXECUTION_LAYER_RPC", false),
                consensus_layer_rpc: Self::required("CONSENSUS_LAYER_RPC", false),
                beacon_state_rpc: Self::required("BEACON_STATE_RPC", false),
                contract_address: Self::required("CONTRACT_ADDRESS", false),
                hash_consensus_address: Self::required("HASH_CONSENSUS_ADDRESS", false),
                withdrawal_credentials: Self::required("LIDO_WIDTHRAWAL_CREDENTIALS", false),
                withdrawal_vault_address: Self::required("WITHDRAWAL_VAULT_ADDRESS", false),
                sp1_prover: Self::required("SP1_PROVER", false),
                sp1_verifier: Self::required("SP1_VERIFIER_ADDRESS", false),
                network_rpc_url: Self::optional("NETWORK_RPC_URL", false),
                dry_run: Self::optional("DRY_RUN", false),
                ethereum_private_key: Self::required("PRIVATE_KEY", true),
                network_private_key: Self::required("NETWORK_PRIVATE_KEY", true),
            }
        }
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
    pub env_vars: Option<env_vars::EnvVars>,
}

impl ScriptRuntime {
    pub fn new(
        eth_infra: EthInfrastructure,
        sp1_infra: Sp1Infrastructure,
        lido_infra: LidoInfrastructure,
        lido_settings: LidoSettings,
        sp1_settings: Sp1Settings,
        env_vars: Option<env_vars::EnvVars>,
    ) -> Self {
        Self {
            eth_infra,
            sp1_infra,
            lido_infra,
            lido_settings,
            sp1_settings,
            env_vars,
        }
    }

    pub fn init(env_vars: env_vars::EnvVars) -> Result<Self, Error> {
        let endpoint: Url = env_vars
            .execution_layer_rpc
            .value
            .clone()
            .parse()
            .expect("Couldn't parse endpoint URL");
        let private_key = env_vars.ethereum_private_key.value.clone();
        let contract_address: Address = env_vars
            .contract_address
            .value
            .clone()
            .parse()
            .expect("Failed to parse CONTRACT_ADDRESS into Address");
        let hash_consensus_address = env_vars
            .hash_consensus_address
            .value
            .clone()
            .parse()
            .expect("Failed to parse HASH_CONSENSUS_ADDRESS into Address");
        let network = env_vars.evm_chain.value.clone().parse::<WrappedNetwork>()?;
        let withdrawal_credentials = env_vars
            .withdrawal_credentials
            .value
            .clone()
            .parse()
            .expect("Failed to parse LIDO_WIDTHRAWAL_CREDENTIALS into Hash256");
        let withdrawal_vault_address = env_vars
            .withdrawal_vault_address
            .value
            .clone()
            .parse()
            .expect("Failed to parse LIDO_WIDTHRAWAL_CREDENTIALS into Address");
        let verifier_address = env_vars
            .sp1_verifier
            .value
            .clone()
            .parse()
            .expect("Failed to parse VERIFIER_ADDRESS into Address");

        let sp1_client = SP1ClientWrapperImpl::new(ProverClient::from_env());
        let beacon_state_reader = BeaconStateReaderEnum::new_from_env(&network)?;
        let provider = Arc::new(ProviderFactory::create_provider_decode_key(private_key, endpoint));
        let report_contract = Sp1LidoAccountingReportContractWrapper::new(Arc::clone(&provider), contract_address);
        let hash_consensus_contract = HashConsensusContractWrapper::new(Arc::clone(&provider), hash_consensus_address);
        let eth_client = ExecutionLayerClient::new(Arc::clone(&provider));
        let lido_settings = LidoSettings {
            withdrawal_credentials,
            contract_address,
            withdrawal_vault_address,
            hash_consensus_address,
        };
        let sp1_settings = Sp1Settings { verifier_address };

        Ok(Self::new(
            EthInfrastructure {
                network,
                provider,
                eth_client,
                beacon_state_reader,
            },
            Sp1Infrastructure { sp1_client },
            LidoInfrastructure {
                report_contract,
                hash_consensus_contract,
            },
            lido_settings,
            sp1_settings,
            Some(env_vars),
        ))
    }

    pub fn init_from_env() -> Result<Self, Error> {
        let env_vars = env_vars::EnvVars::init_from_env();
        Self::init(env_vars)
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
        if let Some(env_vars) = &self.env_vars {
            match &env_vars.dry_run.value {
                Some(v) => v
                    .parse()
                    .unwrap_or_else(|e| panic!("Couldn't parse DRY_RUN value {v}: {e:?}")),
                None => DEFAULT_DRY_RUN,
            }
        } else {
            DEFAULT_DRY_RUN
        }
    }
}
