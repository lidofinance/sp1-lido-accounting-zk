use alloy::network::Ethereum;
use alloy::primitives::Address;
use alloy_primitives::U256;
use sp1_lido_accounting_zk_shared::io::eth_io::{
    ContractDeployParametersRust, LidoValidatorStateRust, ReportMetadataRust, ReportRust,
};
use ISP1VerifierGateway::ISP1VerifierGatewayErrors;
use Sp1LidoAccountingReportContract::Sp1LidoAccountingReportContractErrors;
use Sp1LidoAccountingReportContract::Sp1LidoAccountingReportContractInstance;

use alloy::sol;
use alloy::transports::Transport;
use core::clone::Clone;
use eyre::Result;
use thiserror::Error;

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
    #[error("Contract rejected")]
    Rejection(Sp1LidoAccountingReportContractErrors),

    #[error("Sp1 verifier gateway rejected")]
    VerifierRejection(ISP1VerifierGatewayErrors),

    #[error("Other alloy error")]
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

    fn map_alloy_error(&self, error: alloy::contract::Error) -> Error {
        if let alloy::contract::Error::TransportError(alloy::transports::RpcError::ErrorResp(ref error_payload)) = error
        {
            None.or(error_payload
                .as_decoded_error::<Sp1LidoAccountingReportContractErrors>(true)
                .map(Error::Rejection))
                .or(error_payload
                    .as_decoded_error::<ISP1VerifierGatewayErrors>(true)
                    .map(Error::VerifierRejection))
                .unwrap_or(Error::AlloyError(error))
        } else {
            Error::AlloyError(error)
        }
    }
}
