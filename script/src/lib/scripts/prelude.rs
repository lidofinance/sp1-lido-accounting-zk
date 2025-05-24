use crate::beacon_state_reader::file::FileBasedBeaconStateReader;
use crate::beacon_state_reader::reqwest::{CachedReqwestBeaconStateReader, ReqwestBeaconStateReader};
use crate::beacon_state_reader::{BeaconStateReader, StateId};
use crate::consts::{self, Network, NetworkConfig, NetworkInfo, WrappedNetwork};
use crate::sp1_client_wrapper::SP1ClientWrapperImpl;
use sp1_lido_accounting_zk_shared::eth_consensus_layer::{BeaconBlockHeader, BeaconState};
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

#[derive(Debug, Error)]
pub enum Error {
    #[error("Failed to read env var {0:?}")]
    FailedToReadEnvVar(VarError),

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
                Ok(BeaconStateReaderEnum::File(FileBasedBeaconStateReader::new(
                    &file_store,
                )))
            }
            "rpc" => {
                let rpc_endpoint = env::var("CONSENSUS_LAYER_RPC")?;
                let bs_endpoint = env::var("BEACON_STATE_RPC")?;
                Ok(BeaconStateReaderEnum::RPC(ReqwestBeaconStateReader::new(
                    &rpc_endpoint,
                    &bs_endpoint,
                )))
            }
            "rpc_cached" => {
                let file_store_location = env::var("BS_FILE_STORE")?;
                let rpc_endpoint = env::var("CONSENSUS_LAYER_RPC")?;
                let bs_endpoint = env::var("BEACON_STATE_RPC")?;
                let file_store = PathBuf::from(file_store_location).join(network.as_str());
                Ok(BeaconStateReaderEnum::RPCCached(CachedReqwestBeaconStateReader::new(
                    &rpc_endpoint,
                    &bs_endpoint,
                    &file_store,
                )))
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

pub struct ScriptRuntime {
    network: WrappedNetwork,
    pub provider: Arc<DefaultProvider>,
    pub sp1_client: SP1ClientWrapperImpl,
    pub beacon_state_reader: BeaconStateReaderEnum,
    pub eth_client: EthELClient,
    pub report_contract: ReportContract,
    pub hash_consensus_contract: HashConsensusContract,
}

impl ScriptRuntime {
    pub fn new(
        network: WrappedNetwork,
        provider: Arc<DefaultProvider>,
        client: SP1ClientWrapperImpl,
        beacon_state_reader: BeaconStateReaderEnum,
        eth_client: EthELClient,
        report_contract: ReportContract,
        hash_consensus_contract: HashConsensusContract,
    ) -> Self {
        Self {
            network,
            provider,
            sp1_client: client,
            beacon_state_reader,
            eth_client,
            report_contract,
            hash_consensus_contract,
        }
    }

    pub fn init_from_env() -> Result<Self, Error> {
        let chain = env::var("EVM_CHAIN").expect("Couldn't read EVM_CHAIN env var");
        let raw_endpoint: String = env::var("EXECUTION_LAYER_RPC").expect("Couldn't read EXECUTION_LAYER_RPC env var");
        let endpoint: Url = raw_endpoint.parse().expect("Couldn't parse endpoint URL");
        let private_key = env::var("PRIVATE_KEY").expect("Failed to read PRIVATE_KEY env var");
        let address: Address = env::var("CONTRACT_ADDRESS")
            .expect("Failed to read CONTRACT_ADDRESS env var")
            .parse()
            .expect("Failed to parse CONTRACT_ADDRESS into Address");

        let network = read_network(&chain);
        let client = SP1ClientWrapperImpl::new(ProverClient::from_env(), consts::ELF);
        let beacon_state_reader = BeaconStateReaderEnum::new_from_env(&network)?;
        let provider = Arc::new(ProviderFactory::create_provider_decode_key(private_key, endpoint));
        let report_contract = Sp1LidoAccountingReportContractWrapper::new(Arc::clone(&provider), address);
        let hash_consensus_contract = HashConsensusContractWrapper::new(Arc::clone(&provider), address);
        let eth_client = ExecutionLayerClient::new(Arc::clone(&provider));

        Ok(Self::new(
            network,
            provider,
            client,
            beacon_state_reader,
            eth_client,
            report_contract,
            hash_consensus_contract,
        ))
    }

    pub fn bs_reader(&self) -> &impl BeaconStateReader {
        &self.beacon_state_reader
    }

    pub fn network(&self) -> &impl NetworkInfo {
        &self.network
    }

    pub fn network_config(&self) -> NetworkConfig {
        self.network.get_config()
    }
}

pub fn read_network(val: &str) -> WrappedNetwork {
    let is_anvil = val.starts_with("anvil");
    let base_network: &str = if is_anvil {
        let mut parts = val.splitn(2, '-');
        parts.nth(1).unwrap()
    } else {
        val
    };

    let network = match base_network {
        "mainnet" => Network::Mainnet,
        "sepolia" => Network::Sepolia,
        "holesky" => Network::Holesky,
        _ => panic!("Unknown network"),
    };

    if is_anvil {
        WrappedNetwork::Anvil(network)
    } else {
        WrappedNetwork::Id(network)
    }
}
