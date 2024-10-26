use alloy::network::Ethereum;
use alloy::network::EthereumWallet;
use alloy::primitives::Address;
use alloy::providers::ProviderBuilder;
use alloy::signers::local::PrivateKeySigner;
use alloy::sol;
use alloy::transports::http::reqwest::Url;
use alloy::transports::Transport;
use alloy_primitives::U256;
use serde::{Deserialize, Serialize};

use core::clone::Clone;
use core::fmt;
use eyre::Result;
use k256;
use ISP1VerifierGateway::ISP1VerifierGatewayErrors;

use sp1_lido_accounting_zk_shared::io::eth_io::{LidoValidatorStateRust, ReportMetadataRust, ReportRust};
use sp1_lido_accounting_zk_shared::io::serde_utils::serde_hex_as_string;
use thiserror::Error;
use Sp1LidoAccountingReportContract::Sp1LidoAccountingReportContractErrors;
use Sp1LidoAccountingReportContract::Sp1LidoAccountingReportContractInstance;

sol!(
    #[allow(missing_docs)]
    #[sol(rpc)]
    #[derive(Debug)]
    Sp1LidoAccountingReportContract,
    "../contracts/out/Sp1LidoAccountingReportContract.sol/Sp1LidoAccountingReportContract.json",
);

sol!(
    #[allow(missing_docs)]
    #[sol(rpc)]
    #[derive(Debug)]
    ISP1VerifierGateway,
    "../contracts/out/ISP1VerifierGateway.sol/ISP1VerifierGateway.json",
);

#[derive(Debug, Error)]
pub enum Error {
    #[error("Contract rejected: {0:#?}")]
    Rejection(Sp1LidoAccountingReportContractErrors),

    #[error("Sp1 verifier gateway rejected: {0:#?}")]
    VerifierRejection(ISP1VerifierGatewayErrors),

    #[error("Custom rejection: {0:#?}")]
    CustomRejection(String),

    #[error("Report for slot {0} not found")]
    ReportNotFound(u64),

    #[error("Other alloy error {0:#?}")]
    AlloyError(alloy::contract::Error),
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

#[derive(PartialEq, Eq, Clone, Debug, Serialize, Deserialize)]
pub struct ContractDeployParametersRust {
    pub network: String,
    #[serde(with = "serde_hex_as_string::FixedHexStringProtocol::<20>")]
    pub verifier: [u8; 20],
    #[serde(with = "serde_hex_as_string::FixedHexStringProtocol::<32>")]
    pub vkey: [u8; 32],
    #[serde(with = "serde_hex_as_string::FixedHexStringProtocol::<32>")]
    pub withdrawal_credentials: [u8; 32],
    pub genesis_timestamp: u64,
    pub initial_validator_state: LidoValidatorStateRust,
}

impl fmt::Display for ContractDeployParametersRust {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ContractDeployParametersRust")
            .field("network", &self.network)
            .field("verifier", &hex::encode(self.verifier))
            .field("vkey", &hex::encode(self.vkey))
            .field("withdrawal_credentials", &hex::encode(self.withdrawal_credentials))
            .field("genesis_timestamp", &self.genesis_timestamp)
            .field("initial_validator_state", &self.initial_validator_state)
            .finish()
    }
}

pub struct Sp1LidoAccountingReportContractWrapper<P, T: Transport + Clone>
where
    P: alloy::providers::Provider<T, Ethereum>,
{
    contract: Sp1LidoAccountingReportContractInstance<T, P>,
}

impl<P, T: Transport + Clone> Sp1LidoAccountingReportContractWrapper<P, T>
where
    P: alloy::providers::Provider<T, Ethereum>,
{
    pub fn new(provider: P, contract_address: Address) -> Self {
        let contract = Sp1LidoAccountingReportContract::new(contract_address, provider);
        Sp1LidoAccountingReportContractWrapper { contract }
    }

    pub async fn deploy(provider: P, constructor_args: &ContractDeployParametersRust) -> Result<Self> {
        // Deploy the `Counter` contract.
        let validator_state_solidity: Sp1LidoAccountingReportContract::LidoValidatorState =
            Sp1LidoAccountingReportContract::LidoValidatorState {
                slot: U256::from(constructor_args.initial_validator_state.slot),
                merkle_root: constructor_args.initial_validator_state.merkle_root.into(),
            };
        let contract = Sp1LidoAccountingReportContract::deploy(
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

    pub fn address(&self) -> &Address {
        self.contract.address()
    }

    pub async fn submit_report_data(
        &self,
        slot: u64,
        report: ReportRust,
        metadata: ReportMetadataRust,
        proof: Vec<u8>,
        public_values: Vec<u8>,
    ) -> Result<alloy_primitives::TxHash, Error> {
        let report_solidity: Sp1LidoAccountingReportContract::Report = report.into();
        let metadata_solidity: Sp1LidoAccountingReportContract::ReportMetadata = metadata.into();

        let tx_builder = self.contract.submitReportData(
            U256::from(slot),
            report_solidity,
            metadata_solidity,
            proof.into(),
            public_values.into(),
        );

        let tx = tx_builder
            .send()
            .await
            .map_err(|e: alloy::contract::Error| self.map_alloy_error(e))?;

        log::info!("Waiting for report transaction");
        let tx_result = tx.watch().await.expect("Failed to wait for confirmation");
        Ok(tx_result)
    }

    pub async fn get_latest_report_slot(&self) -> Result<u64, Error> {
        let latest_report_response = self
            .contract
            .getLatestLidoValidatorStateSlot()
            .call()
            .await
            .map_err(|e: alloy::contract::Error| self.map_alloy_error(e))?;
        let latest_report_slot = latest_report_response._0;
        Ok(latest_report_slot.to::<u64>())
    }

    pub async fn get_report(&self, slot: u64) -> Result<ReportRust, Error> {
        let report_response = self
            .contract
            .getReport(U256::from(slot))
            .call()
            .await
            .map_err(|e: alloy::contract::Error| self.map_alloy_error(e))?;

        if !report_response.success {
            return Err(Error::ReportNotFound(slot));
        }

        let report: ReportRust = ReportRust {
            slot,
            deposited_lido_validators: report_response.totalDepositedValidators.to(),
            exited_lido_validators: report_response.totalExitedValidators.to(),
            lido_cl_balance: report_response.clBalanceGwei.to(),
        };
        Ok(report)
    }

    fn map_alloy_error(&self, error: alloy::contract::Error) -> Error {
        if let alloy::contract::Error::TransportError(alloy::transports::RpcError::ErrorResp(ref error_payload)) = error
        {
            if let Some(contract_error) = error_payload.as_decoded_error::<Sp1LidoAccountingReportContractErrors>(true)
            {
                Error::Rejection(contract_error)
            } else if let Some(verifier_error) = error_payload.as_decoded_error::<ISP1VerifierGatewayErrors>(true) {
                Error::VerifierRejection(verifier_error)
            } else if error_payload.message.contains("execution reverted") {
                Error::CustomRejection(error_payload.message.clone())
            } else {
                Error::AlloyError(error)
            }
        } else {
            Error::AlloyError(error)
        }
    }
}

#[derive(Debug, Error)]
pub enum ProviderError {
    #[error("Failed to convert string to hex")]
    FromHexError,
    #[error("Failed to parse private key")]
    ParsePrivateKeyError,
    #[error("Failed to deserialize private key")]
    DeserializePrivateKeyError,
}

pub type DefaultProvider = alloy::providers::fillers::FillProvider<
    alloy::providers::fillers::JoinFill<
        alloy::providers::fillers::JoinFill<
            alloy::providers::fillers::JoinFill<
                alloy::providers::fillers::JoinFill<alloy::providers::Identity, alloy::providers::fillers::GasFiller>,
                alloy::providers::fillers::NonceFiller,
            >,
            alloy::providers::fillers::ChainIdFiller,
        >,
        alloy::providers::fillers::WalletFiller<EthereumWallet>,
    >,
    alloy::providers::RootProvider<alloy::transports::http::Http<reqwest::Client>>,
    alloy::transports::http::Http<reqwest::Client>,
    alloy::network::Ethereum,
>;

pub type Contract =
    Sp1LidoAccountingReportContractWrapper<DefaultProvider, alloy::transports::http::Http<reqwest::Client>>;
pub struct ProviderFactory {}
impl ProviderFactory {
    fn decode_key(private_key_raw: &str) -> Result<k256::SecretKey, ProviderError> {
        let key_str = private_key_raw
            .split("0x")
            .last()
            .ok_or(ProviderError::ParsePrivateKeyError)?
            .trim();
        let key_hex = hex::decode(key_str).map_err(|_e| ProviderError::FromHexError)?;
        let key = k256::SecretKey::from_bytes((&key_hex[..]).into())
            .map_err(|_e| ProviderError::DeserializePrivateKeyError)?;
        Ok(key)
    }

    pub fn create_provider(key: k256::SecretKey, endpoint: Url) -> DefaultProvider {
        let signer: PrivateKeySigner = PrivateKeySigner::from(key);
        let wallet: EthereumWallet = EthereumWallet::from(signer);
        ProviderBuilder::new()
            .with_recommended_fillers()
            .wallet(wallet)
            .on_http(endpoint)
    }

    pub fn create_provider_decode_key(key_str: String, endpoint: Url) -> DefaultProvider {
        let key = Self::decode_key(&key_str).expect("Failed to decode private key");
        Self::create_provider(key, endpoint)
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use crate::utils;

    use super::*;
    use hex_literal::hex;

    fn default_params() -> ContractDeployParametersRust {
        ContractDeployParametersRust {
            network: "anvil-sepolia".to_owned(),
            verifier: hex!("3b6041173b80e77f038f3f2c0f9744f04837185e"),
            vkey: hex!("00da6bb9e019268e8f2494fc5dbcda36d7c1c854ca2682df448f761cf47887f4"),
            withdrawal_credentials: hex!("010000000000000000000000de7318afa67ead6d6bbc8224dfce5ed6e4b86d76"),
            genesis_timestamp: 1655733600,
            initial_validator_state: LidoValidatorStateRust {
                slot: 5832096,
                merkle_root: hex!("918070ce0cb66881d6839965371f79a600bc26b50a363a17ac00a1b295f89113"),
            },
        }
    }

    #[test]
    fn deployment_parameters_serde() {
        let params = default_params();

        let serialized = serde_json::to_string_pretty(&params).expect("Failed to serialize");
        let deserialized: ContractDeployParametersRust =
            serde_json::from_str(&serialized).expect("Failed to deserialize");

        assert_eq!(params, deserialized);
    }

    #[test]
    fn deployment_parameters_from_file() {
        let deploy_args_file =
            PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/data/deploy/anvil-sepolia-5832096-deploy.json");
        let deploy_params: ContractDeployParametersRust =
            utils::read_json(deploy_args_file).expect("Failed to read deployment args");

        assert_eq!(deploy_params, default_params());
    }
}
