mod test_utils;

use alloy_sol_types::SolType;
use sp1_lido_accounting_scripts::{
    beacon_state_reader::{BeaconStateReader, StateId},
    eth_client,
    proof_storage::StoredProof,
    scripts::shared as shared_logic,
    sp1_client_wrapper::SP1ClientWrapper,
};

use hex_literal::hex;
use sp1_lido_accounting_zk_shared::{
    eth_consensus_layer::Hash256,
    io::{
        eth_io::{PublicValuesRust, PublicValuesSolidity, ReportMetadataRust, ReportRust},
        program_io::WithdrawalVaultData,
    },
};
use sp1_sdk::HashableKey;
use test_utils::env::IntegrationTestEnvironment;
use thiserror::Error;

const STORED_PROOF_FILE_NAME: &str = "fixture.json";

#[derive(Debug, Error)]
enum ExecutorError {
    #[error("Contract rejected: {0:#?}")]
    Contract(eth_client::Error),
    #[error("Failed to launch anvil: {0:#?}")]
    AnvilLaunch(alloy::node_bindings::NodeError),
    #[error("Eyre error: {0:#?}")]
    Eyre(eyre::Error),
    #[error("Anyhow error: {0:#?}")]
    Anyhow(anyhow::Error),
}

type Result<T> = std::result::Result<T, ExecutorError>;
type TestExecutorResult = Result<alloy_primitives::TxHash>;

impl From<eth_client::Error> for ExecutorError {
    fn from(value: eth_client::Error) -> Self {
        ExecutorError::Contract(value)
    }
}

impl From<alloy::node_bindings::NodeError> for ExecutorError {
    fn from(value: alloy::node_bindings::NodeError) -> Self {
        ExecutorError::AnvilLaunch(value)
    }
}

impl From<eyre::Error> for ExecutorError {
    fn from(value: eyre::Error) -> Self {
        ExecutorError::Eyre(value)
    }
}

impl From<anyhow::Error> for ExecutorError {
    fn from(value: anyhow::Error) -> Self {
        ExecutorError::Anyhow(value)
    }
}

struct TestExecutor<M: Fn(PublicValuesRust) -> PublicValuesRust> {
    env: IntegrationTestEnvironment,
    tamper_public_values: M,
}

impl<M: Fn(PublicValuesRust) -> PublicValuesRust> TestExecutor<M> {
    async fn new(tamper_public_values: M) -> Result<Self> {
        let env = IntegrationTestEnvironment::default().await?;
        let instance = Self {
            env,
            tamper_public_values,
        };
        Ok(instance)
    }

    fn get_stored_proof(&self) -> Result<StoredProof> {
        let proof = self.env.test_files.read_proof(STORED_PROOF_FILE_NAME)?;
        Ok(proof)
    }

    async fn run_test(&self) -> TestExecutorResult {
        sp1_sdk::utils::setup_logger();
        let lido_withdrawal_credentials: Hash256 = self.env.network_config().lido_withdrawal_credentials.into();
        let stored_proof = self.get_stored_proof()?;

        let reference_slot = stored_proof.report.reference_slot;
        let bc_slot = stored_proof.metadata.bc_slot;

        let previous_slot = self
            .env
            .script_runtime
            .report_contract
            .get_latest_validator_state_slot()
            .await?;

        let target_bh = self
            .env
            .script_runtime
            .bs_reader()
            .read_beacon_block_header(&StateId::Slot(bc_slot))
            .await?;
        let target_bs = self
            .env
            .script_runtime
            .bs_reader()
            .read_beacon_state(&StateId::Slot(bc_slot))
            .await?;
        // Should read old state from untampered reader, so the old state compute will match
        let old_bs = self
            .env
            .script_runtime
            .bs_reader()
            .read_beacon_state(&StateId::Slot(previous_slot))
            .await?;
        tracing::info!("Preparing program input");

        let withdrawal_vault_data = WithdrawalVaultData {
            balance: stored_proof.metadata.withdrawal_vault_data.balance,
            vault_address: stored_proof.metadata.withdrawal_vault_data.vault_address,
            account_proof: vec![vec![0u8, 1u8, 2u8, 3u8]], // proof is unused in this scenario
        };

        let (_program_input, public_values) = shared_logic::prepare_program_input(
            reference_slot,
            &target_bs,
            &target_bh,
            &old_bs,
            &lido_withdrawal_credentials,
            withdrawal_vault_data,
            false,
        );
        tracing::info!("Reading proof");

        let tampered_public_values = (self.tamper_public_values)(public_values);

        let pub_vals_solidity: PublicValuesSolidity = tampered_public_values.into();
        let public_values_bytes: Vec<u8> = PublicValuesSolidity::abi_encode(&pub_vals_solidity);

        tracing::info!("Sending report");
        let result = self
            .env
            .script_runtime
            .report_contract
            .submit_report_data(stored_proof.proof, public_values_bytes)
            .await?;

        Ok(result)
    }
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

Withdrawal credentials:
* Different address
* Actual proof, tampered balance
*/

fn id<T>(val: T) -> T {
    val
}

fn wrap_report_mapper(mapper: fn(ReportRust) -> ReportRust) -> impl Fn(PublicValuesRust) -> PublicValuesRust {
    move |pub_vals| {
        let mut new_pub_values = pub_vals.clone();
        new_pub_values.report = (mapper)(new_pub_values.report);
        new_pub_values
    }
}

fn wrap_metadata_mapper(
    mapper: fn(ReportMetadataRust) -> ReportMetadataRust,
) -> impl Fn(PublicValuesRust) -> PublicValuesRust {
    move |pub_vals| {
        let mut new_pub_values = pub_vals.clone();
        new_pub_values.metadata = (mapper)(new_pub_values.metadata);
        new_pub_values
    }
}

fn assert_rejects(result: TestExecutorResult) -> Result<()> {
    match result {
        Err(ExecutorError::Contract(eth_client::Error::Rejection(err))) => {
            tracing::info!("As expected, contract rejected {:#?}", err);
            Ok(())
        }
        Err(ExecutorError::Contract(eth_client::Error::CustomRejection(err))) => {
            tracing::info!("As expected, verifier rejected {:#?}", err);
            Ok(())
        }
        Err(other_err) => Err(other_err),
        Ok(_txhash) => Err(ExecutorError::Anyhow(anyhow::anyhow!("Report accepted"))),
    }
}

#[test]
fn check_vkey_matches() -> Result<()> {
    let test_files = test_utils::files::TestFiles::new_from_manifest_dir();
    let proof = test_files.read_proof(STORED_PROOF_FILE_NAME)?;
    assert_eq!(test_utils::SP1_CLIENT.vk().bytes32(), proof.vkey, "Vkey in stored proof and in client mismatch. Please run write_test_fixture script to generate new stored proof");
    Ok(())
}

#[test]
fn check_old_slot_matches() -> Result<()> {
    let test_files = test_utils::files::TestFiles::new_from_manifest_dir();
    let proof = test_files.read_proof(STORED_PROOF_FILE_NAME)?;
    assert_eq!(
        test_utils::DEPLOY_SLOT,
        proof.metadata.state_for_previous_report.slot,
        "Stored proof targets wrong previous slot, should be {}, got {}",
        test_utils::DEPLOY_SLOT,
        proof.metadata.state_for_previous_report.slot
    );
    Ok(())
}

#[tokio::test(flavor = "multi_thread")]
async fn report_tampering_sanity_check_should_pass() -> Result<()> {
    let executor = TestExecutor::new(id).await?;

    let result = executor.run_test().await;
    match result {
        Ok(_txhash) => {
            tracing::info!("Sanity check succeeded - submitting valid report with no tampering succeeds");
            Ok(())
        }
        Err(err) => Err(err),
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn report_tampering_report_slot() -> Result<()> {
    let executor = TestExecutor::new(wrap_report_mapper(|report| {
        let mut new_report = report.clone();
        new_report.reference_slot = new_report.reference_slot - 1;
        new_report
    }))
    .await?;

    assert_rejects(executor.run_test().await)
}

#[tokio::test(flavor = "multi_thread")]
async fn report_tampering_report_slot2() -> Result<()> {
    let executor = TestExecutor::new(wrap_report_mapper(|report| {
        let mut new_report = report.clone();
        new_report.reference_slot = new_report.reference_slot + 10;
        new_report
    }))
    .await?;

    assert_rejects(executor.run_test().await)
}

#[tokio::test(flavor = "multi_thread")]
async fn report_tampering_report_cl_balance() -> Result<()> {
    let executor = TestExecutor::new(wrap_report_mapper(|report| {
        let mut new_report = report.clone();
        new_report.lido_cl_balance += 50;
        new_report
    }))
    .await?;

    assert_rejects(executor.run_test().await)
}

#[tokio::test(flavor = "multi_thread")]
async fn report_tampering_report_deposited_count() -> Result<()> {
    let executor = TestExecutor::new(wrap_report_mapper(|report| {
        let mut new_report = report.clone();
        new_report.deposited_lido_validators += 1;
        new_report
    }))
    .await?;

    assert_rejects(executor.run_test().await)
}

#[tokio::test(flavor = "multi_thread")]
async fn report_tampering_report_exited_count() -> Result<()> {
    let executor = TestExecutor::new(wrap_report_mapper(|report| {
        let mut new_report = report.clone();
        new_report.exited_lido_validators += 1;
        new_report
    }))
    .await?;

    assert_rejects(executor.run_test().await)
}

#[tokio::test(flavor = "multi_thread")]
async fn report_tampering_metadata_slot() -> Result<()> {
    let executor = TestExecutor::new(wrap_metadata_mapper(|metadata| {
        let mut new_metadata = metadata.clone();
        new_metadata.bc_slot = new_metadata.bc_slot - 10;
        new_metadata
    }))
    .await?;

    assert_rejects(executor.run_test().await)
}

#[tokio::test(flavor = "multi_thread")]
async fn report_tampering_metadata_slot2() -> Result<()> {
    let executor = TestExecutor::new(wrap_metadata_mapper(|metadata| {
        let mut new_metadata = metadata.clone();
        new_metadata.bc_slot = new_metadata.bc_slot + 10;
        new_metadata
    }))
    .await?;

    assert_rejects(executor.run_test().await)
}

#[tokio::test(flavor = "multi_thread")]
async fn report_tampering_metadata_epoch() -> Result<()> {
    let executor = TestExecutor::new(wrap_metadata_mapper(|metadata| {
        let mut new_metadata = metadata.clone();
        new_metadata.epoch = 9876543;
        new_metadata
    }))
    .await?;

    assert_rejects(executor.run_test().await)
}

#[tokio::test(flavor = "multi_thread")]
async fn report_tampering_metadata_withdrawal_credentials() -> Result<()> {
    let executor = TestExecutor::new(wrap_metadata_mapper(|metadata| {
        let mut new_metadata = metadata.clone();
        new_metadata.lido_withdrawal_credentials =
            hex!("010000000000000000000000abcdefabcdefabcdefabcdefabcdefabcdefabcd");
        new_metadata
    }))
    .await?;

    assert_rejects(executor.run_test().await)
}

#[tokio::test(flavor = "multi_thread")]
async fn report_tampering_metadata_beacon_block_hash() -> Result<()> {
    let executor = TestExecutor::new(wrap_metadata_mapper(|metadata| {
        let mut new_metadata = metadata.clone();
        new_metadata.beacon_block_hash = hex!("123456789000000000000000abcdefabcdefabcdefabcdefabcdefabcdefabcd");
        new_metadata
    }))
    .await?;

    assert_rejects(executor.run_test().await)
}

#[tokio::test(flavor = "multi_thread")]
async fn report_tampering_metadata_old_state_slot() -> Result<()> {
    let executor = TestExecutor::new(wrap_metadata_mapper(|metadata| {
        let mut new_metadata = metadata.clone();
        new_metadata.state_for_previous_report.slot = new_metadata.state_for_previous_report.slot - 10;
        new_metadata
    }))
    .await?;

    assert_rejects(executor.run_test().await)
}

#[tokio::test(flavor = "multi_thread")]
async fn report_tampering_metadata_old_state_merkle_root() -> Result<()> {
    let executor = TestExecutor::new(wrap_metadata_mapper(|metadata| {
        let mut new_metadata = metadata.clone();
        new_metadata.state_for_previous_report.merkle_root =
            hex!("1234567890000000000000000000000000000000000000000000000000000000");
        new_metadata
    }))
    .await?;

    assert_rejects(executor.run_test().await)
}

#[tokio::test(flavor = "multi_thread")]
async fn report_tampering_metadata_new_state_slot() -> Result<()> {
    let executor = TestExecutor::new(wrap_metadata_mapper(|metadata| {
        let mut new_metadata = metadata.clone();
        new_metadata.new_state.slot = new_metadata.new_state.slot + 10;
        new_metadata
    }))
    .await?;

    assert_rejects(executor.run_test().await)
}

#[tokio::test(flavor = "multi_thread")]
async fn report_tampering_metadata_new_state_merkle_root() -> Result<()> {
    let executor = TestExecutor::new(wrap_metadata_mapper(|metadata| {
        let mut new_metadata = metadata.clone();
        new_metadata.new_state.merkle_root = hex!("1234567890000000000000000000000000000000000000000000000000000000");
        new_metadata
    }))
    .await?;

    assert_rejects(executor.run_test().await)
}

#[tokio::test(flavor = "multi_thread")]
async fn report_tampering_withdrawal_wrong_address() -> Result<()> {
    let executor = TestExecutor::new(wrap_metadata_mapper(|metadata| {
        let mut new_metadata = metadata.clone();
        new_metadata.withdrawal_vault_data.vault_address = hex!("1234567890000000000000000000000000000000").into();
        new_metadata
    }))
    .await?;

    assert_rejects(executor.run_test().await)
}

#[tokio::test(flavor = "multi_thread")]
async fn report_tampering_withdrawal_wrong_balance() -> Result<()> {
    let executor = TestExecutor::new(wrap_metadata_mapper(|metadata| {
        let mut new_metadata = metadata.clone();
        new_metadata.withdrawal_vault_data.balance = alloy_primitives::U256::from(1234567890u64);
        new_metadata
    }))
    .await?;

    assert_rejects(executor.run_test().await)
}
