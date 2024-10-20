use alloy::node_bindings::{Anvil, AnvilInstance};
use anyhow::{anyhow, Result};
use sp1_lido_accounting_scripts::{
    beacon_state_reader::{BeaconStateReader, BeaconStateReaderEnum, StateId},
    consts::{self, NetworkInfo, WrappedNetwork},
    eth_client::{self, Contract, ProviderFactory, Sp1LidoAccountingReportContractWrapper},
    scripts::{self, shared as shared_logic},
    sp1_client_wrapper::{SP1ClientWrapper, SP1ClientWrapperImpl},
};

use lazy_static::lazy_static;
use sp1_lido_accounting_zk_shared::{
    eth_consensus_layer::{BeaconState, Hash256, Validator},
    io::eth_io::{ReportMetadataRust, ReportRust},
};
use sp1_sdk::ProverClient;
use std::env;
use test_utils::TestFiles;
mod test_utils;
use hex_literal::hex;

static NETWORK: &WrappedNetwork = &test_utils::NETWORK;

lazy_static! {
    static ref SP1_CLIENT: SP1ClientWrapperImpl = SP1ClientWrapperImpl::new(ProverClient::network(), consts::ELF);
    static ref LIDO_CREDS: Hash256 = NETWORK.get_config().lido_withdrawal_credentials.into();
}

fn eyre_to_anyhow(err: eyre::Error) -> anyhow::Error {
    anyhow!("Eyre error: {:#?}", err)
}

struct TestExecutor {
    bs_reader: BeaconStateReaderEnum,
    client: &'static SP1ClientWrapperImpl,
    test_files: test_utils::TestFiles,
    tamper_report: fn(ReportRust) -> ReportRust,
    tamper_metadata: fn(ReportMetadataRust) -> ReportMetadataRust,
}

impl TestExecutor {
    fn new(
        tamper_report: fn(ReportRust) -> ReportRust,
        tamper_metadata: fn(ReportMetadataRust) -> ReportMetadataRust,
    ) -> Self {
        let test_files = TestFiles::new_from_manifest_dir();
        Self {
            bs_reader: BeaconStateReaderEnum::new_from_env(NETWORK),
            client: &SP1_CLIENT,
            test_files,
            tamper_report,
            tamper_metadata,
        }
    }

    fn get_target_slot(&self) -> u64 {
        test_utils::CACHED_BEACON_STATE_SLOT
    }

    async fn start_anvil(&self, target_slot: u64) -> Result<AnvilInstance> {
        let finalized_bs = test_utils::read_latest_bs_at_or_before(&self.bs_reader, target_slot, test_utils::RETRIES)
            .await
            .map_err(eyre_to_anyhow)?;
        let fork_url =
            env::var("INTEGRATION_TEST_FORK_URL").expect("INTEGRATION_TEST_FORK_URL env var must be specified");
        let fork_block_number = finalized_bs.latest_execution_payload_header.block_number + 2;
        log::debug!(
            "Starting anvil: fork_block_number={}, fork_url={}",
            fork_block_number,
            fork_url
        );
        let anvil = Anvil::new()
            .fork(fork_url)
            .fork_block_number(fork_block_number)
            .try_spawn()?;
        Ok(anvil)
    }

    async fn deploy_contract(&self, network: &impl NetworkInfo, anvil: &AnvilInstance) -> Result<Contract> {
        let provider = ProviderFactory::create_provider(anvil.keys()[0].clone(), anvil.endpoint().parse()?);

        let deploy_bs: BeaconState = self
            .test_files
            .read_beacon_state(&StateId::Slot(test_utils::DEPLOY_SLOT))
            .await
            .map_err(eyre_to_anyhow)?;
        let deploy_params = scripts::deploy::prepare_deploy_params(self.client.vk_bytes(), &deploy_bs, network);

        log::info!("Deploying contract with parameters {:?}", deploy_params);
        let contract = Sp1LidoAccountingReportContractWrapper::deploy(provider, &deploy_params)
            .await
            .map_err(eyre_to_anyhow)?;
        log::info!("Deployed contract at {}", contract.address());
        Ok(contract)
    }

    async fn run_test(&self) -> Result<()> {
        sp1_sdk::utils::setup_logger();
        let lido_withdrawal_credentials: Hash256 = NETWORK.get_config().lido_withdrawal_credentials.into();

        let target_slot = self.get_target_slot();
        // // Anvil needs to be here in scope for the duration of the test, otherwise it terminates
        // // Hence creating it here (i.e. owner is this function) and passing down to deploy conract
        let anvil = self.start_anvil(target_slot).await?;
        let contract = self.deploy_contract(NETWORK, &anvil).await?;
        let previous_slot = contract.get_latest_report_slot().await?;

        let target_bh = self
            .bs_reader
            .read_beacon_block_header(&StateId::Slot(target_slot))
            .await?;
        let target_bs = self.bs_reader.read_beacon_state(&StateId::Slot(target_slot)).await?;
        // Should read old state from untampered reader, so the old state compute will match
        let old_bs = self.bs_reader.read_beacon_state(&StateId::Slot(previous_slot)).await?;
        log::info!("Preparing program input");
        let (_program_input, public_values) =
            shared_logic::prepare_program_input(&target_bs, &target_bh, &old_bs, &lido_withdrawal_credentials, false);
        log::info!("Reading proof");
        let proof = self.test_files.read_proof("fixture.json").map_err(eyre_to_anyhow)?;

        log::info!("Sending report");
        let result = contract
            .submit_report_data(
                target_bs.slot,
                (self.tamper_report)(public_values.report),
                (self.tamper_metadata)(public_values.metadata),
                proof.proof,
                proof.public_values,
            )
            .await;

        match result {
            Err(eth_client::Error::Rejection(err)) => {
                log::info!("As expected, contract rejected {:#?}", err);
                Ok(())
            }
            Err(eth_client::Error::VerifierRejection(err)) => {
                log::info!("As expected, verifier rejected {:#?}", err);
                Ok(())
            }
            Err(other_err) => Err(anyhow!(
                "Submission failed due to technical reasons - inconclusive outcome {:#?}",
                other_err
            )),
            Ok(_txhash) => Err(anyhow!("Report accepted")),
        }
    }
}

fn validator_indices<P>(bs: &BeaconState, positions: &[usize], predicate: P) -> Vec<usize>
where
    P: Fn(&Validator) -> bool,
{
    let filtered_validator_indices: Vec<usize> = bs
        .validators
        .iter()
        .enumerate()
        .filter_map(
            |(index, validator)| {
                if predicate(validator) {
                    Some(index)
                } else {
                    None
                }
            },
        )
        .collect();
    positions.iter().map(|idx| filtered_validator_indices[*idx]).collect()
}

fn is_lido(validator: &Validator) -> bool {
    validator.withdrawal_credentials == *LIDO_CREDS
}

fn is_non_lido(validator: &Validator) -> bool {
    !is_lido(validator)
}

/*
General idea here is that a valid proof is used with a mismatching report

Report:
* Different slot
* Different cl balance
* Different deposited validator count
* Different exited validator count

Metatada:
* Different slot
* Different epoch
* Different Lido withdrawal credentials
* Different beacon block hash
* Different old state - slot
* Different old state - hash
* Different new state - slot
* Different new state - hash
*/

fn id<T>(val: T) -> T {
    val
}

#[tokio::test(flavor = "multi_thread")]
async fn report_tampering_report_slot() -> Result<()> {
    let executor = TestExecutor::new(
        |report| {
            let mut new_report = report.clone();
            new_report.slot = 1234567890;
            new_report
        },
        id,
    );

    executor.run_test().await
}

#[tokio::test(flavor = "multi_thread")]
async fn report_tampering_report_cl_balance() -> Result<()> {
    let executor = TestExecutor::new(
        |report| {
            let mut new_report = report.clone();
            new_report.lido_cl_balance += 50;
            new_report
        },
        id,
    );

    executor.run_test().await
}

#[tokio::test(flavor = "multi_thread")]
async fn report_tampering_report_deposited_count() -> Result<()> {
    let executor = TestExecutor::new(
        |report| {
            let mut new_report = report.clone();
            new_report.deposited_lido_validators += 1;
            new_report
        },
        id,
    );

    executor.run_test().await
}

#[tokio::test(flavor = "multi_thread")]
async fn report_tampering_report_exited_count() -> Result<()> {
    let executor = TestExecutor::new(
        |report| {
            let mut new_report = report.clone();
            new_report.exited_lido_validators += 1;
            new_report
        },
        id,
    );

    executor.run_test().await
}

#[tokio::test(flavor = "multi_thread")]
async fn report_tampering_metadata_slot() -> Result<()> {
    let executor = TestExecutor::new(id, |metadata| {
        let mut new_metadata = metadata.clone();
        new_metadata.slot = 1234567890;
        new_metadata
    });

    executor.run_test().await
}

#[tokio::test(flavor = "multi_thread")]
async fn report_tampering_metadata_epoch() -> Result<()> {
    let executor = TestExecutor::new(id, |metadata| {
        let mut new_metadata = metadata.clone();
        new_metadata.epoch = 9876543;
        new_metadata
    });

    executor.run_test().await
}

#[tokio::test(flavor = "multi_thread")]
async fn report_tampering_metadata_withdrawal_credentials() -> Result<()> {
    let executor = TestExecutor::new(id, |metadata| {
        let mut new_metadata = metadata.clone();
        new_metadata.lido_withdrawal_credentials =
            hex!("010000000000000000000000abcdefabcdefabcdefabcdefabcdefabcdefabcd");
        new_metadata
    });

    executor.run_test().await
}

#[tokio::test(flavor = "multi_thread")]
async fn report_tampering_metadata_beacon_block_hash() -> Result<()> {
    let executor = TestExecutor::new(id, |metadata| {
        let mut new_metadata = metadata.clone();
        new_metadata.beacon_block_hash = hex!("123456789000000000000000abcdefabcdefabcdefabcdefabcdefabcdefabcd");
        new_metadata
    });

    executor.run_test().await
}

#[tokio::test(flavor = "multi_thread")]
async fn report_tampering_metadata_old_state_slot() -> Result<()> {
    let executor = TestExecutor::new(id, |metadata| {
        let mut new_metadata = metadata.clone();
        new_metadata.state_for_previous_report.slot = 1234567890;
        new_metadata
    });

    executor.run_test().await
}

#[tokio::test(flavor = "multi_thread")]
async fn report_tampering_metadata_old_state_merkle_root() -> Result<()> {
    let executor = TestExecutor::new(id, |metadata| {
        let mut new_metadata = metadata.clone();
        new_metadata.state_for_previous_report.merkle_root =
            hex!("1234567890000000000000000000000000000000000000000000000000000000");
        new_metadata
    });

    executor.run_test().await
}

#[tokio::test(flavor = "multi_thread")]
async fn report_tampering_metadata_new_state_slot() -> Result<()> {
    let executor = TestExecutor::new(id, |metadata| {
        let mut new_metadata = metadata.clone();
        new_metadata.new_state.slot = 1234567890;
        new_metadata
    });

    executor.run_test().await
}

#[tokio::test(flavor = "multi_thread")]
async fn report_tampering_metadata_new_state_merkle_root() -> Result<()> {
    let executor = TestExecutor::new(id, |metadata| {
        let mut new_metadata = metadata.clone();
        new_metadata.new_state.merkle_root = hex!("1234567890000000000000000000000000000000000000000000000000000000");
        new_metadata
    });

    executor.run_test().await
}
