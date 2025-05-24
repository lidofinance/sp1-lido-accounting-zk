use crate::beacon_state_reader::{BeaconStateReader, BeaconStateReaderEnum};
use crate::consts::{self, NetworkInfo, WrappedNetwork};
use crate::sp1_client_wrapper::SP1ClientWrapperImpl;
use sp1_sdk::ProverClient;

use crate::eth_client::{
    DefaultProvider, EthELClient, ExecutionLayerClient, HashConsensusContract, HashConsensusContractWrapper,
    ProviderFactory, ReportContract, Sp1LidoAccountingReportContractWrapper,
};
use alloy::primitives::Address;

use std::env;
use std::sync::Arc;
use thiserror::Error;

use alloy::transports::http::reqwest::Url;

#[derive(Debug, Error)]
pub enum Error {
    #[error("Failed to read env var")]
    FailedToReadEnvVar(String),

    #[error("Failed to parse URL")]
    FailedToParseUrl,
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

        let network = consts::read_network(&chain);
        let client = SP1ClientWrapperImpl::new(ProverClient::from_env(), consts::ELF);
        let bs_reader = BeaconStateReaderEnum::new_from_env(&network);
        let provider = Arc::new(ProviderFactory::create_provider_decode_key(private_key, endpoint));
        let report_contract = Sp1LidoAccountingReportContractWrapper::new(Arc::clone(&provider), address);
        let hash_consensus_contract = HashConsensusContractWrapper::new(Arc::clone(&provider), address);
        let eth_client = ExecutionLayerClient::new(Arc::clone(&provider));

        Ok(Self::new(
            network,
            provider,
            client,
            bs_reader,
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
}
