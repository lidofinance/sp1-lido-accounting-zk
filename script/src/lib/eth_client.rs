use alloy::eips::BlockId;
use alloy::eips::RpcBlockHash;
use alloy::network::Ethereum;
use alloy::network::EthereumWallet;
use alloy::primitives::Address;
use alloy::providers::fillers::RecommendedFillers;
use alloy::providers::ProviderBuilder;
use alloy::signers::local::PrivateKeySigner;
use alloy::sol;
use alloy::transports::http::reqwest::Url;
use alloy_primitives::U256;
use serde::{Deserialize, Serialize};
use sp1_lido_accounting_zk_shared::eth_consensus_layer::Hash256;
use sp1_lido_accounting_zk_shared::io::eth_io::{BeaconChainSlot, ReferenceSlot};
use sp1_lido_accounting_zk_shared::io::program_io::WithdrawalVaultData;

use core::clone::Clone;
use core::fmt;
use eyre::Result;
use k256;
use std::fmt::Debug;
use std::sync::Arc;

use sp1_lido_accounting_zk_shared::io::eth_io::{LidoValidatorStateRust, ReportRust};
use sp1_lido_accounting_zk_shared::io::serde_utils::serde_hex_as_string;
use thiserror::Error;
use HashConsensus::HashConsensusInstance;
use Sp1LidoAccountingReportContract::Sp1LidoAccountingReportContractErrors;
use Sp1LidoAccountingReportContract::Sp1LidoAccountingReportContractInstance;

sol!(
    #[allow(missing_docs)]
    #[sol(rpc)]
    #[derive(Debug)]
    Sp1LidoAccountingReportContract,
    "../contracts/out/Sp1LidoAccountingReportContract.sol/Sp1LidoAccountingReportContract.json",
);

sol! {
    #[sol(rpc)]
    interface HashConsensus {
        function getCurrentFrame() external view returns (
            uint256 refSlot,
            uint256 reportProcessingDeadlineSlot
        );
    }
}

#[derive(Debug, Error)]
pub enum Error {
    #[error("Contract rejected: {0:#?}")]
    Rejection(Sp1LidoAccountingReportContractErrors),

    #[error("Custom rejection: {0:#?}")]
    CustomRejection(String),

    #[error("Report for slot {0} not found")]
    ReportNotFound(ReferenceSlot),

    #[error("Other alloy error {0:#?}")]
    AlloyError(alloy::contract::Error),
}

#[derive(Debug, Error)]
pub enum RPCError {
    #[error(transparent)]
    Error(#[from] alloy::transports::RpcError<alloy::transports::TransportErrorKind>),
}

fn map_rpc_error(error: alloy::transports::RpcError<alloy::transports::TransportErrorKind>) -> RPCError {
    error.into()
}

impl From<ReportRust> for Sp1LidoAccountingReportContract::Report {
    fn from(value: ReportRust) -> Self {
        Sp1LidoAccountingReportContract::Report {
            reference_slot: value.reference_slot.into(),
            deposited_lido_validators: U256::from(value.deposited_lido_validators),
            exited_lido_validators: U256::from(value.exited_lido_validators),
            lido_cl_balance: U256::from(value.lido_cl_balance),
            lido_withdrawal_vault_balance: U256::from(value.lido_withdrawal_vault_balance),
        }
    }
}

impl From<LidoValidatorStateRust> for Sp1LidoAccountingReportContract::LidoValidatorState {
    fn from(value: LidoValidatorStateRust) -> Self {
        Sp1LidoAccountingReportContract::LidoValidatorState {
            slot: value.slot.into(),
            merkle_root: value.merkle_root.into(),
        }
    }
}

#[derive(PartialEq, Eq, Clone, Serialize, Deserialize)]
pub struct ContractDeployParametersRust {
    pub network: String,
    #[serde(with = "serde_hex_as_string::FixedHexStringProtocol::<20>")]
    pub verifier: [u8; 20],
    #[serde(with = "serde_hex_as_string::FixedHexStringProtocol::<32>")]
    pub vkey: [u8; 32],
    #[serde(with = "serde_hex_as_string::FixedHexStringProtocol::<32>")]
    pub withdrawal_credentials: [u8; 32],
    #[serde(with = "serde_hex_as_string::FixedHexStringProtocol::<20>")]
    pub withdrawal_vault_address: [u8; 20],
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
            .field("withdrawal_vault_address", &hex::encode(self.withdrawal_vault_address))
            .field("genesis_timestamp", &self.genesis_timestamp)
            .field("initial_validator_state", &self.initial_validator_state)
            .finish()
    }
}

impl Debug for ContractDeployParametersRust {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self) // just use display
    }
}

pub struct Sp1LidoAccountingReportContractWrapper<P>
where
    P: alloy::providers::Provider<Ethereum> + std::clone::Clone,
{
    contract: Sp1LidoAccountingReportContractInstance<(), Arc<P>>,
}

impl<P> Sp1LidoAccountingReportContractWrapper<P>
where
    P: alloy::providers::Provider<Ethereum> + std::clone::Clone,
{
    pub fn new(provider: Arc<P>, contract_address: Address) -> Self {
        let contract = Sp1LidoAccountingReportContract::new(contract_address, Arc::clone(&provider));
        Sp1LidoAccountingReportContractWrapper { contract }
    }

    pub async fn deploy(provider: Arc<P>, constructor_args: &ContractDeployParametersRust) -> Result<Self> {
        // Deploy the `Counter` contract.
        let validator_state_solidity: Sp1LidoAccountingReportContract::LidoValidatorState =
            Sp1LidoAccountingReportContract::LidoValidatorState {
                slot: constructor_args.initial_validator_state.slot.into(),
                merkle_root: constructor_args.initial_validator_state.merkle_root.into(),
            };
        let contract = Sp1LidoAccountingReportContract::deploy(
            provider.clone(),
            constructor_args.verifier.into(),
            constructor_args.vkey.into(),
            constructor_args.withdrawal_credentials.into(),
            constructor_args.withdrawal_vault_address.into(),
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
        proof: Vec<u8>,
        public_values: Vec<u8>,
    ) -> Result<alloy_primitives::TxHash, Error> {
        let tx_builder = self.contract.submitReportData(proof.into(), public_values.into());

        let tx = tx_builder
            .send()
            .await
            .map_err(|e: alloy::contract::Error| self.map_contract_error(e))?;

        tracing::info!("Waiting for report transaction");
        let tx_result = tx.watch().await.expect("Failed to wait for confirmation");
        Ok(tx_result)
    }

    pub async fn get_latest_validator_state_slot(&self) -> Result<BeaconChainSlot, Error> {
        let latest_report_response = self
            .contract
            .getLatestLidoValidatorStateSlot()
            .call()
            .await
            .map_err(|e: alloy::contract::Error| self.map_contract_error(e))?;
        let latest_report_slot = latest_report_response._0;
        Ok(BeaconChainSlot(latest_report_slot.to::<u64>()))
    }

    pub async fn get_report(&self, slot: ReferenceSlot) -> Result<ReportRust, Error> {
        let report_response = self
            .contract
            .getReport(slot.into())
            .call()
            .await
            .map_err(|e: alloy::contract::Error| self.map_contract_error(e))?;

        if !report_response.success {
            return Err(Error::ReportNotFound(slot));
        }

        let report: ReportRust = ReportRust {
            reference_slot: slot,
            deposited_lido_validators: report_response.totalDepositedValidators.to(),
            exited_lido_validators: report_response.totalExitedValidators.to(),
            lido_cl_balance: report_response.clBalanceGwei.to(),
            lido_withdrawal_vault_balance: report_response.withdrawalVaultBalanceWei.to(),
        };
        Ok(report)
    }

    fn map_contract_error(&self, error: alloy::contract::Error) -> Error {
        if let alloy::contract::Error::TransportError(alloy::transports::RpcError::ErrorResp(ref error_payload)) = error
        {
            if let Some(contract_error) =
                error_payload.as_decoded_interface_error::<Sp1LidoAccountingReportContractErrors>()
            {
                Error::Rejection(contract_error)
            } else if error_payload.message.contains("execution reverted") {
                Error::CustomRejection(error_payload.message.to_string())
            } else {
                Error::AlloyError(error)
            }
        } else {
            Error::AlloyError(error)
        }
    }
}

pub struct HashConsensusContractWrapper<P>
where
    P: alloy::providers::Provider<Ethereum>,
{
    contract: HashConsensusInstance<(), Arc<P>>,
}

impl<P> HashConsensusContractWrapper<P>
where
    P: alloy::providers::Provider<Ethereum>,
{
    pub fn new(provider: Arc<P>, contract_address: Address) -> Self {
        let contract = HashConsensusInstance::new(contract_address, Arc::clone(&provider));
        HashConsensusContractWrapper { contract }
    }

    pub async fn get_refslot(&self) -> Result<(ReferenceSlot, ReferenceSlot), alloy::contract::Error> {
        let result: HashConsensus::getCurrentFrameReturn = self.contract.getCurrentFrame().call().await?;
        Ok((result.refSlot.into(), result.reportProcessingDeadlineSlot.into()))
    }
}

pub struct ExecutionLayerClient<P>
where
    P: alloy::providers::Provider<Ethereum>,
{
    provider: Arc<P>,
}

impl<P> ExecutionLayerClient<P>
where
    P: alloy::providers::Provider<Ethereum>,
{
    pub fn new(provider: Arc<P>) -> Self {
        Self { provider }
    }

    pub async fn get_withdrawal_vault_data(
        &self,
        address: Address,
        block_hash: Hash256,
    ) -> Result<WithdrawalVaultData, RPCError> {
        tracing::info!(
            "Reading balance proof for address 0x{} at block 0x{}",
            hex::encode(address),
            hex::encode(block_hash)
        );

        let block_hash: RpcBlockHash = RpcBlockHash::from_hash(block_hash.0.into(), Some(true));
        let response = self
            .provider
            .get_proof(address, vec![])
            .block_id(BlockId::Hash(block_hash))
            // .block_id(BlockId::Number(alloy::eips::BlockNumberOrTag::Latest))
            .await
            .map(|resp| {
                let proof_as_vecs = resp.account_proof.iter().map(|val| val.to_vec()).collect();
                WithdrawalVaultData {
                    vault_address: address,
                    balance: resp.balance,
                    account_proof: proof_as_vecs,
                }
            })
            .map_err(map_rpc_error)?;

        Ok(response)
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
            alloy::providers::Identity,
            <Ethereum as RecommendedFillers>::RecommendedFillers,
        >,
        alloy::providers::fillers::WalletFiller<EthereumWallet>,
    >,
    alloy::providers::RootProvider,
>;

pub type ReportContract = Sp1LidoAccountingReportContractWrapper<DefaultProvider>;
pub type HashConsensusContract = HashConsensusContractWrapper<DefaultProvider>;

pub type EthELClient = ExecutionLayerClient<DefaultProvider>;
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
        ProviderBuilder::new().wallet(wallet).on_http(endpoint)
    }

    pub fn create_provider_decode_key(key_str: String, endpoint: Url) -> DefaultProvider {
        let key = Self::decode_key(&key_str).expect("Failed to decode private key");
        Self::create_provider(key, endpoint)
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use crate::{consts, utils};

    use super::*;
    use hex_literal::hex;
    use sp1_lido_accounting_zk_shared::io::eth_io::BeaconChainSlot;

    fn default_params() -> ContractDeployParametersRust {
        ContractDeployParametersRust {
            network: "anvil-sepolia".to_owned(),
            verifier: hex!("e00a3cbfc45241b33c0a44c78e26168cbc55ec63"),
            vkey: hex!("00a13852b52626b0cc77128e2935361ed27c3ba6e97ffa92a9faaa62f0720643"),
            withdrawal_credentials: hex!("010000000000000000000000de7318afa67ead6d6bbc8224dfce5ed6e4b86d76"),
            withdrawal_vault_address: consts::lido_withdrawal_vault::SEPOLIA,
            genesis_timestamp: 1655733600,
            initial_validator_state: LidoValidatorStateRust {
                slot: BeaconChainSlot(7643456),
                merkle_root: hex!("5d22a84a06f79d4b9f4d94769190a9f5afb077607f5084b781c1d996c4bd3c16"),
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
            PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/data/deploy/anvil-sepolia-7643456-deploy.json");
        let deploy_params: ContractDeployParametersRust =
            utils::read_json(deploy_args_file).expect("Failed to read deployment args");

        assert_eq!(deploy_params, default_params());
    }
}
