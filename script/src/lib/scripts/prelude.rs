use crate::beacon_state_reader::BeaconStateReaderEnum;
use crate::consts::{self, Network};
use crate::sp1_client_wrapper::SP1ClientWrapper;
use sp1_sdk::ProverClient;

use crate::eth_client::Sp1LidoAccountingReportContractWrapper;
use alloy::primitives::Address;

use std::env;
use thiserror::Error;

use alloy::network::EthereumWallet;
use alloy::transports::http::reqwest::Url;

use alloy::{providers::ProviderBuilder, signers::local::PrivateKeySigner};
use eyre::Result;
use k256;

#[derive(Debug, Error)]
pub enum Error {
    #[error("Failed to convert string to hex")]
    FromHexError,
    #[error("Failed to parse private key")]
    ParsePrivateKeyError,
    #[error("Failed to deserialize private key")]
    DeserializePrivateKeyError,
    #[error("Failed to read env var")]
    FailedToReadEnvVar(String),

    #[error("Failed to parse URL")]
    FailedToParseUrl,
}

pub fn initialize() -> (Network, SP1ClientWrapper, BeaconStateReaderEnum) {
    let chain = env::var("EVM_CHAIN").expect("Couldn't read EVM_CHAIN env var");
    let network = Network::from_str(&chain).unwrap();
    let client = SP1ClientWrapper::new(ProverClient::network(), consts::ELF);
    let bs_reader = BeaconStateReaderEnum::new_from_env(&network);

    (network, client, bs_reader)
}

fn decode_key(private_key_raw: &str) -> Result<k256::SecretKey, Error> {
    let key_str = private_key_raw
        .split("0x")
        .last()
        .ok_or(Error::ParsePrivateKeyError)?
        .trim();
    let key_hex = hex::decode(key_str).map_err(|_e| Error::FromHexError)?;
    let key = k256::SecretKey::from_bytes((&key_hex[..]).into()).map_err(|_e| Error::DeserializePrivateKeyError)?;
    Ok(key)
}

pub type Contract = Sp1LidoAccountingReportContractWrapper<
    alloy::providers::fillers::FillProvider<
        alloy::providers::fillers::JoinFill<
            alloy::providers::fillers::JoinFill<
                alloy::providers::fillers::JoinFill<
                    alloy::providers::fillers::JoinFill<
                        alloy::providers::Identity,
                        alloy::providers::fillers::GasFiller,
                    >,
                    alloy::providers::fillers::NonceFiller,
                >,
                alloy::providers::fillers::ChainIdFiller,
            >,
            alloy::providers::fillers::WalletFiller<EthereumWallet>,
        >,
        alloy::providers::RootProvider<alloy::transports::http::Http<reqwest::Client>>,
        alloy::transports::http::Http<reqwest::Client>,
        alloy::network::Ethereum,
    >,
    alloy::transports::http::Http<reqwest::Client>,
>;

// TODO: simplify return type
pub fn initialize_contract() -> Contract {
    let raw_endpoint: String = env::var("EXECUTION_LAYER_RPC").expect("Couldn't read EXECUTION_LAYER_RPC env var");
    let endpoint: Url = raw_endpoint.parse().expect("Couldn't parse endpoint URL");
    let private_key = env::var("PRIVATE_KEY").expect("Failed to read PRIVATE_KEY env var");
    let key = decode_key(&private_key).expect("Failed to decode private key");
    let signer: PrivateKeySigner = PrivateKeySigner::from(key);
    let wallet: EthereumWallet = EthereumWallet::from(signer);
    let provider = ProviderBuilder::new()
        .with_recommended_fillers()
        .wallet(wallet)
        .on_http(endpoint);

    let address: Address = env::var("CONTRACT_ADDRESS")
        .expect("Failed to read CONTRACT_ADDRESS env var")
        .parse()
        .expect("Failed to parse CONTRACT_ADDRESS into URL");
    Sp1LidoAccountingReportContractWrapper::new(provider, address)
}
