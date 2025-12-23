use alloy::eips::BlockId;
use alloy::eips::RpcBlockHash;
use alloy::network::Ethereum;
use alloy::network::EthereumWallet;
use alloy::primitives::Address;
use alloy::providers::fillers::RecommendedFillers;
use alloy::providers::ProviderBuilder;
use alloy::rpc::types::TransactionReceipt;
use alloy::signers::local::PrivateKeySigner;
use alloy::sol;
use alloy::transports::http::reqwest::Url;
use alloy_primitives::U256;
use serde::{Deserialize, Serialize};
use sp1_lido_accounting_zk_shared::eth_consensus_layer::Hash256;
use sp1_lido_accounting_zk_shared::io::eth_io;
use sp1_lido_accounting_zk_shared::io::eth_io::{BeaconChainSlot, ReferenceSlot};
use sp1_lido_accounting_zk_shared::io::program_io::WithdrawalVaultData;
use tracing::Instrument;

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

use crate::prometheus_metrics;

sol!(
    #[allow(missing_docs)]
    #[sol(rpc)]
    #[derive(Debug)]
    Sp1LidoAccountingReportContract,
    "../../contracts/out/Sp1LidoAccountingReportContract.sol/Sp1LidoAccountingReportContract.json",
);

sol! {
    #[sol(rpc)]
    interface HashConsensus {
        #[derive(Debug)]
        function getCurrentFrame() external view returns (
            uint256 refSlot,
            uint256 reportProcessingDeadlineSlot
        );
    }
}

#[derive(Debug, Error)]
pub enum ContractError {
    #[error("Contract rejected: {0:#?}")]
    Rejection(Sp1LidoAccountingReportContractErrors),

    #[error("Custom rejection: {0:#?}")]
    CustomRejection(String),

    #[error("Report for slot {0} not found")]
    ReportNotFound(ReferenceSlot),

    #[error("Other alloy error {0:#?}")]
    OtherAlloyError(alloy::contract::Error),

    #[error("Transaction error {0:#?}")]
    TransactionError(#[from] alloy::providers::PendingTransactionError),

    #[error("{0:#?}")]
    EthIOConversionError(#[from] eth_io::Error),
}

impl From<alloy::contract::Error> for ContractError {
    fn from(error: alloy::contract::Error) -> Self {
        if let alloy::contract::Error::TransportError(alloy::transports::RpcError::ErrorResp(ref error_payload)) = error
        {
            if let Some(contract_error) =
                error_payload.as_decoded_interface_error::<Sp1LidoAccountingReportContractErrors>()
            {
                ContractError::Rejection(contract_error)
            } else if error_payload.message.contains("execution reverted") {
                ContractError::CustomRejection(error_payload.message.to_string())
            } else {
                ContractError::OtherAlloyError(error)
            }
        } else {
            ContractError::OtherAlloyError(error)
        }
    }
}

#[derive(Debug, Error)]
pub enum RPCError {
    #[error(transparent)]
    Error(#[from] alloy::transports::RpcError<alloy::transports::TransportErrorKind>),
}

impl TryFrom<ReportRust> for Sp1LidoAccountingReportContract::Report {
    type Error = ContractError;
    fn try_from(value: ReportRust) -> Result<Self, Self::Error> {
        let result = Sp1LidoAccountingReportContract::Report {
            reference_slot: value.reference_slot.try_into()?,
            deposited_lido_validators: U256::from(value.deposited_lido_validators),
            exited_lido_validators: U256::from(value.exited_lido_validators),
            lido_cl_balance: U256::from(value.lido_cl_balance),
            lido_withdrawal_vault_balance: U256::from(value.lido_withdrawal_vault_balance),
        };
        Ok(result)
    }
}

impl TryFrom<LidoValidatorStateRust> for Sp1LidoAccountingReportContract::LidoValidatorState {
    type Error = ContractError;
    fn try_from(value: LidoValidatorStateRust) -> Result<Self, Self::Error> {
        let result = Sp1LidoAccountingReportContract::LidoValidatorState {
            slot: value.slot.try_into()?,
            merkle_root: value.merkle_root.into(),
        };
        Ok(result)
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
    #[serde(with = "serde_hex_as_string::FixedHexStringProtocol::<20>")]
    pub admin: [u8; 20],
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
        write!(f, "{self}") // just use display
    }
}

pub struct Sp1LidoAccountingReportContractWrapper<P>
where
    P: alloy::providers::Provider<Ethereum> + std::clone::Clone,
{
    contract: Sp1LidoAccountingReportContractInstance<Arc<P>>,
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
                slot: constructor_args.initial_validator_state.slot.try_into()?,
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
            constructor_args.admin.into(),
        )
        .await?;
        Ok(Sp1LidoAccountingReportContractWrapper { contract })
    }

    pub fn address(&self) -> &Address {
        self.contract.address()
    }

    async fn submit_report_data_impl(
        &self,
        proof: Vec<u8>,
        public_values: Vec<u8>,
    ) -> Result<TransactionReceipt, ContractError> {
        let tx_builder = self.contract.submitReportData(proof.into(), public_values.into());
        // Optional preflight call to surface revert reasons before sending a tx.
        // This mirrors what we send on-chain, so if it already reverts we can fail fast.
        if std::env::var("SKIP_PREFLIGHT_CALL").is_err() {
            let preflight = tx_builder.call().await;
            if let Err(err) = preflight {
                tracing::error!("Preflight call for submitReportData reverted: {err:?}");
                return Err(err.into());
            }
        }

        tracing::info!("Submitting report transaction");
        let tx = tx_builder
            .send()
            .instrument(tracing::info_span!("send_tx"))
            .await
            .inspect(|val| tracing::debug!("Submitted transaction {}", val.tx_hash()))
            .inspect_err(|err| tracing::error!("Failed to submit transaction {err:?}"))?;

        tracing::info!("Waiting for report transaction");
        let tx_result = tx
            .get_receipt()
            .instrument(tracing::info_span!("get_receipt"))
            .await
            .inspect(|val| {
                if val.status() {
                    tracing::info!("Transaction completed {:#?}", val.transaction_hash)
                } else {
                    tracing::error!("Transaction reverted {:#?}", val.transaction_hash)
                }
            })
            .inspect_err(|err| tracing::error!("Transaction failed {err:?}"))?;

        // Short-circuit on on-chain revert so callers see an error, not Ok(receipt)
        if !tx_result.status() {
            tracing::debug!(
                "Receipt status=0, decoding revert for tx {}",
                tx_result.transaction_hash
            );
            let call_result = tx_builder.call().await;

            return match call_result {
                Ok(_) => Err(ContractError::CustomRejection(format!(
                    "Transaction reverted without reason: {:#?}",
                    tx_result.transaction_hash
                ))),
                Err(e) => Err(e.into()),
            };
        }

        Ok(tx_result)
    }

    pub async fn submit_report_data(
        &self,
        proof: Vec<u8>,
        public_values: Vec<u8>,
    ) -> Result<TransactionReceipt, ContractError> {
        let tracing_span = tracing::info_span!("submit_report_data");
        self.submit_report_data_impl(proof, public_values)
            .instrument(tracing_span)
            .await
    }

    pub async fn get_latest_validator_state_slot(&self) -> Result<BeaconChainSlot, ContractError> {
        tracing::info!("Getting latest validator state slot");
        let latest_report_response = self
            .contract
            .getLatestLidoValidatorStateSlot()
            .call()
            .await
            .inspect(|val| tracing::info!("Obtained latest validator state slot {:#?}", val))
            .inspect_err(|err| tracing::error!("Failed to read latest validator state slot {err:?}"))?;
        Ok(BeaconChainSlot(latest_report_response.to::<u64>()))
    }

    pub async fn get_report(&self, slot: ReferenceSlot) -> Result<ReportRust, ContractError> {
        let report_response = self
            .contract
            .getReport(slot.try_into()?)
            .call()
            .await
            .inspect(|_val| tracing::debug!(slot=?slot, "Obtained report for slot {slot:?}"))
            .inspect_err(|err| tracing::error!(slot=?slot, "Failed to read report for slot {slot:?}: {err:?}"))?;

        if !report_response.success {
            tracing::warn!(slot=?slot, "Report for slot {slot:?} not found");
            return Err(ContractError::ReportNotFound(slot));
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
}

pub struct HashConsensusContractWrapper<P>
where
    P: alloy::providers::Provider<Ethereum>,
{
    contract: HashConsensusInstance<Arc<P>>,
    metric_reporter: Arc<prometheus_metrics::Service>,
}

impl<P> HashConsensusContractWrapper<P>
where
    P: alloy::providers::Provider<Ethereum>,
{
    pub fn new(provider: Arc<P>, contract_address: Address, metric_reporter: Arc<prometheus_metrics::Service>) -> Self {
        let contract = HashConsensusInstance::new(contract_address, Arc::clone(&provider));
        HashConsensusContractWrapper {
            contract,
            metric_reporter,
        }
    }

    async fn get_refslot_impl(&self) -> Result<(ReferenceSlot, ReferenceSlot), ContractError> {
        tracing::info!(
            hash_consensus_address = hex::encode(self.contract.address()),
            "Reading current refslot from HashConsensus"
        );
        let result: HashConsensus::getCurrentFrameReturn = self.contract.getCurrentFrame().call().await?;

        Ok((
            result.refSlot.try_into()?,
            result.reportProcessingDeadlineSlot.try_into()?,
        ))
    }

    pub async fn get_refslot(&self) -> Result<(ReferenceSlot, ReferenceSlot), ContractError> {
        self.metric_reporter
            .run_with_metrics_and_logs_async(prometheus_metrics::services::hash_consensus::GET_REFSLOT, || {
                self.get_refslot_impl()
            })
            .await
    }
}

pub struct ExecutionLayerClient<P>
where
    P: alloy::providers::Provider<Ethereum>,
{
    provider: Arc<P>,
    metric_reporter: Arc<prometheus_metrics::Service>,
}

impl<P> ExecutionLayerClient<P>
where
    P: alloy::providers::Provider<Ethereum>,
{
    pub fn new(provider: Arc<P>, metric_reporter: Arc<prometheus_metrics::Service>) -> Self {
        Self {
            provider,
            metric_reporter,
        }
    }

    async fn get_withdrawal_vault_data_impl(
        &self,
        address: Address,
        block_hash: Hash256,
    ) -> Result<WithdrawalVaultData, RPCError> {
        tracing::info!(
            widthrawal_vault_address = hex::encode(address),
            "Reading balance proof for address 0x{address:#?} at block 0x{block_hash:#?}",
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
            })?;
        Ok(response)
    }

    pub async fn get_withdrawal_vault_data(
        &self,
        address: Address,
        block_hash: Hash256,
    ) -> Result<WithdrawalVaultData, RPCError> {
        self.metric_reporter
            .run_with_metrics_and_logs_async(
                prometheus_metrics::services::eth_client::GET_WITHDRAWAL_VAULT_DATA,
                || self.get_withdrawal_vault_data_impl(address, block_hash),
            )
            .await
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
        ProviderBuilder::new().wallet(wallet).connect_http(endpoint)
    }

    pub fn create_provider_decode_key(key_str: String, endpoint: Url) -> Result<DefaultProvider, ProviderError> {
        let key = Self::decode_key(&key_str)?;
        Ok(Self::create_provider(key, endpoint))
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use crate::utils;

    use super::*;
    use hex_literal::hex;
    use sp1_lido_accounting_zk_shared::io::eth_io::BeaconChainSlot;

    fn default_params() -> ContractDeployParametersRust {
        ContractDeployParametersRust {
            network: "fusaka".to_owned(),
            verifier: hex!("17435cce3d1b4fa2e5f8a08ed921d57c6762a180"),
            vkey: hex!("00cacf583f09b87b96201653eec2d8c946616c026cb4e369106008e9b4001d9c"),
            withdrawal_credentials: hex!("010000000000000000000000f0179dec45a37423ead4fad5fcb136197872ead9"),
            withdrawal_vault_address: hex!("b4b46bdaa835f8e4b4d8e208b6559cd267851051"),
            genesis_timestamp: 1760348240,
            initial_validator_state: LidoValidatorStateRust {
                slot: BeaconChainSlot(1200),
                merkle_root: hex!("d9d0aaed20248d7eb129b77c92a2ef72b9701d1c7cc136297fcb96aa652f15f6"),
            },
            admin: hex!("8943545177806ed17b9f23f0a21ee5948ecaa776"),
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
        let deploy_args_file = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../data/deploy/fusaka-deploy.json");
        let deploy_params: ContractDeployParametersRust =
            utils::read_json(deploy_args_file).expect("Failed to read deployment args");

        assert_eq!(deploy_params, default_params());
    }
}
