use crate::beacon_state_reader::BeaconStateReaderEnum;
use crate::consts::{self, WrappedNetwork};
use crate::sp1_client_wrapper::SP1ClientWrapperImpl;
use sp1_sdk::ProverClient;

use crate::eth_client::{Contract, DefaultProvider, ProviderFactory, Sp1LidoAccountingReportContractWrapper};
use alloy::primitives::Address;

use std::env;
use thiserror::Error;

use alloy::transports::http::reqwest::Url;

#[derive(Debug, Error)]
pub enum Error {
    #[error("Failed to read env var")]
    FailedToReadEnvVar(String),

    #[error("Failed to parse URL")]
    FailedToParseUrl,
}

pub fn initialize() -> (WrappedNetwork, SP1ClientWrapperImpl, BeaconStateReaderEnum) {
    let chain = env::var("EVM_CHAIN").expect("Couldn't read EVM_CHAIN env var");
    let network = consts::read_network(&chain);
    let client = SP1ClientWrapperImpl::new(ProverClient::new(), consts::ELF);
    let bs_reader = BeaconStateReaderEnum::new_from_env(&network);

    (network, client, bs_reader)
}

pub fn initialize_provider() -> DefaultProvider {
    let raw_endpoint: String = env::var("EXECUTION_LAYER_RPC").expect("Couldn't read EXECUTION_LAYER_RPC env var");
    let endpoint: Url = raw_endpoint.parse().expect("Couldn't parse endpoint URL");
    let private_key = env::var("PRIVATE_KEY").expect("Failed to read PRIVATE_KEY env var");
    ProviderFactory::create_provider_decode_key(private_key, endpoint)
}

pub fn initialize_contract() -> Contract {
    let address: Address = env::var("CONTRACT_ADDRESS")
        .expect("Failed to read CONTRACT_ADDRESS env var")
        .parse()
        .expect("Failed to parse CONTRACT_ADDRESS into Address");
    let provider = initialize_provider();
    Sp1LidoAccountingReportContractWrapper::new(provider, address)
}
