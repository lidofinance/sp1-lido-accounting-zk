use alloy::sol_types::SolError;
use alloy::transports::http::reqwest::Url;
use alloy::{network::EthereumWallet, primitives::Address};
use alloy_primitives::U256;
use sp1_lido_accounting_zk_shared::io::eth_io::{
    ContractDeployParametersRust, LidoValidatorStateRust, ReportMetadataRust, ReportRust,
};
use std::env;
use Sp1LidoAccountingReportContract::Sp1LidoAccountingReportContractInstance;

use alloy::{providers::ProviderBuilder, signers::local::PrivateKeySigner, sol};
use eyre::Result;
use k256;
use thiserror::Error;

sol!(
    #[allow(missing_docs)]
    #[sol(rpc)]
    #[derive(Debug)]
    Sp1LidoAccountingReportContract,
    "../contracts/out/Sp1LidoAccountingReportContract.sol/Sp1LidoAccountingReportContract.json",
);

#[derive(Debug)]
pub enum RejectionError {
    NoBlockRootFound(Sp1LidoAccountingReportContract::NoBlockRootFound),
    TimestampOutOfRange(Sp1LidoAccountingReportContract::TimestampOutOfRange),
    CustomError(Vec<u8>),
}

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

    #[error("Failed to submit report")]
    ReportSubmissionFailure,
    #[error("Failed to parse URL")]
    FailedToParseUrl,

    #[error("Rejected report")]
    RejectionError(RejectionError),
}

impl Error {
    pub fn parse_rejection(error_data: Vec<u8>) -> Self {
        let rejection_error = {
            if let Some(selector) = error_data.get(0..4) {
                let mut fixed_selector: [u8; 4] = [0u8; 4];
                fixed_selector.copy_from_slice(selector);
                match fixed_selector {
                    Sp1LidoAccountingReportContract::NoBlockRootFound::SELECTOR => {
                        if let Ok(decoded) =
                            Sp1LidoAccountingReportContract::NoBlockRootFound::abi_decode(error_data.as_slice(), true)
                        {
                            RejectionError::NoBlockRootFound(decoded)
                        } else {
                            RejectionError::CustomError(error_data)
                        }
                    }
                    Sp1LidoAccountingReportContract::TimestampOutOfRange::SELECTOR => {
                        if let Ok(decoded) = Sp1LidoAccountingReportContract::TimestampOutOfRange::abi_decode(
                            error_data.as_slice(),
                            true,
                        ) {
                            RejectionError::TimestampOutOfRange(decoded)
                        } else {
                            RejectionError::CustomError(error_data)
                        }
                    }
                    _ => RejectionError::CustomError(error_data),
                }
            } else {
                RejectionError::CustomError(error_data)
            }
        };
        Error::RejectionError(rejection_error)
    }
}

impl From<ReportRust> for Sp1LidoAccountingReportContract::Report {
    fn from(value: ReportRust) -> Self {
        Sp1LidoAccountingReportContract::Report {
            slot: U256::from(value.slot),
            deposited_lido_validators: U256::from(value.deposited_lido_validators),
            exited_lido_validators: U256::from(value.exited_lido_validators),
            lido_cl_balance: U256::from(value.lido_cl_balance),
        }
    }
}

impl From<LidoValidatorStateRust> for Sp1LidoAccountingReportContract::LidoValidatorState {
    fn from(value: LidoValidatorStateRust) -> Self {
        Sp1LidoAccountingReportContract::LidoValidatorState {
            slot: U256::from(value.slot),
            merkle_root: value.merkle_root.into(),
        }
    }
}

impl From<ReportMetadataRust> for Sp1LidoAccountingReportContract::ReportMetadata {
    fn from(value: ReportMetadataRust) -> Self {
        Sp1LidoAccountingReportContract::ReportMetadata {
            slot: U256::from(value.slot),
            epoch: U256::from(value.epoch),
            lido_withdrawal_credentials: value.lido_withdrawal_credentials.into(),
            beacon_block_hash: value.beacon_block_hash.into(),
            old_state: value.state_for_previous_report.into(),
            new_state: value.new_state.into(),
        }
    }
}

pub struct Sp1LidoAccountingReportContractWrapper<P>
where
    P: alloy::providers::Provider,
{
    contract: Sp1LidoAccountingReportContractInstance<alloy::transports::BoxTransport, P>,
}

impl<P> Sp1LidoAccountingReportContractWrapper<P>
where
    P: alloy::providers::Provider,
{
    pub fn new(provider: P, contract_address: Address) -> Self {
        let contract: Sp1LidoAccountingReportContractInstance<alloy::transports::BoxTransport, P> =
            Sp1LidoAccountingReportContract::new(contract_address, provider);
        Sp1LidoAccountingReportContractWrapper { contract }
    }

    pub async fn deploy(provider: P, constructor_args: &ContractDeployParametersRust) -> Result<Self> {
        // Deploy the `Counter` contract.
        let validator_state_solidity: Sp1LidoAccountingReportContract::LidoValidatorState =
            Sp1LidoAccountingReportContract::LidoValidatorState {
                slot: U256::from(constructor_args.initial_validator_state.slot),
                merkle_root: constructor_args.initial_validator_state.merkle_root.into(),
            };
        let contract: Sp1LidoAccountingReportContractInstance<alloy::transports::BoxTransport, P> =
            Sp1LidoAccountingReportContract::deploy(
                provider,
                constructor_args.verifier.into(),
                constructor_args.vkey.into(),
                constructor_args.withdrawal_credentials.into(),
                U256::from(constructor_args.genesis_timestamp),
                validator_state_solidity,
            )
            .await?;
        Ok(Sp1LidoAccountingReportContractWrapper { contract })
    }

    pub async fn submit_report_data(
        &self,
        slot: u64,
        report: ReportRust,
        metadata: ReportMetadataRust,
        proof: Vec<u8>,
        public_values: Vec<u8>,
    ) -> Result<Sp1LidoAccountingReportContract::submitReportDataReturn, Error> {
        let report_solidity: Sp1LidoAccountingReportContract::Report = report.into();
        let metadata_solidity: Sp1LidoAccountingReportContract::ReportMetadata = metadata.into();

        let result = self
            .contract
            .submitReportData(
                U256::from(slot),
                report_solidity,
                metadata_solidity,
                proof.into(),
                public_values.into(),
            )
            .call()
            .await
            .map_err(|_e| Error::ReportSubmissionFailure)?;
        Ok(result)
    }
}

pub struct ProviderFactory {}
impl ProviderFactory {
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

    pub fn create(
        endpoint: Url,
        private_key: k256::SecretKey,
    ) -> std::result::Result<
        impl alloy::providers::Provider<alloy::transports::http::Http<alloy::transports::http::Client>> + Clone,
        Error,
    > {
        let signer: PrivateKeySigner = PrivateKeySigner::from(private_key);
        let wallet: EthereumWallet = EthereumWallet::from(signer);
        let provider = ProviderBuilder::new()
            .with_recommended_fillers()
            .wallet(wallet)
            .on_http(endpoint);
        Ok(provider)
    }

    pub fn create_from_env() -> Result<
        impl alloy::providers::Provider<alloy::transports::http::Http<alloy::transports::http::Client>> + Clone,
        Error,
    > {
        let raw_endpoint: String = env::var("EXECUTION_LAYER_RPC")
            .map_err(|_e| Error::FailedToReadEnvVar("EXECUTION_LAYER_RPC".to_owned()))?;
        let endpoint: Url = raw_endpoint.parse().map_err(|_e| Error::FailedToParseUrl)?;
        let private_key = env::var("PRIVATE_KEY").expect("Failed to read PRIVATE_KEY env var");
        let key = Self::decode_key(&private_key)?;
        Self::create(endpoint, key)
    }
}
