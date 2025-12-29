mod test_utils;

use alloy::rpc::types::TransactionReceipt;
use alloy_sol_types::SolType;
use sp1_lido_accounting_scripts::{
    beacon_state_reader::{BeaconStateReader, StateId},
    eth_client::Sp1LidoAccountingReportContract::Sp1LidoAccountingReportContractErrors,
    proof_storage::StoredProof,
    scripts::shared as shared_logic,
    sp1_client_wrapper::SP1ClientWrapper,
    InputChecks,
};

use anyhow::Result;
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

use crate::test_utils::{env::SP1_CLIENT, eyre_to_anyhow, TestAssertions};

const STORED_PROOF_FILE_NAME: &str = "fixture.json";

type TestExecutorResult = std::result::Result<TransactionReceipt, test_utils::TestError>;

struct TestExecutor<M: Fn(PublicValuesRust) -> PublicValuesRust> {
    env: IntegrationTestEnvironment,
    tamper_public_values: M,
}

impl<M: Fn(PublicValuesRust) -> PublicValuesRust> TestExecutor<M> {
    async fn new(tamper_public_values: M) -> anyhow::Result<Self> {
        let mut env = IntegrationTestEnvironment::new(
            test_utils::NETWORK.clone(),
            test_utils::DEPLOY_SLOT,
            Some(test_utils::REPORT_COMPUTE_SLOT),
        )
        .await?;
        env.mock_beacon_state_roots_contract().await?;

        let instance = Self {
            env,
            tamper_public_values,
        };
        Ok(instance)
    }

    fn get_stored_proof(&self) -> anyhow::Result<StoredProof> {
        let proof = self
            .env
            .test_files
            .read_proof(STORED_PROOF_FILE_NAME)
            .map_err(eyre_to_anyhow)?;
        Ok(proof)
    }

    async fn run_test(&self) -> TestExecutorResult {
        let lido_withdrawal_credentials: Hash256 = self.env.script_runtime.lido_settings.withdrawal_credentials;
        let stored_proof = self.get_stored_proof()?;

        assert_eq!(
            stored_proof.metadata.bc_slot,
            test_utils::REPORT_COMPUTE_SLOT,
            "Stored proof metadata slot does not match expected report compute slot"
        );

        let reference_slot = stored_proof.report.reference_slot;
        let bc_slot = stored_proof.metadata.bc_slot;

        self.env
            .record_beacon_block_hash(bc_slot.0, stored_proof.metadata.beacon_block_hash.into())
            .await?;

        let previous_slot = self
            .env
            .script_runtime
            .lido_infra
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

        InputChecks::set_relaxed();
        let (_program_input, public_values) = shared_logic::prepare_program_input(
            reference_slot,
            &target_bs,
            &target_bh,
            &old_bs,
            &lido_withdrawal_credentials,
            withdrawal_vault_data,
        )
        .expect("Failed to prepare program input");
        tracing::info!("Reading proof");

        let tampered_public_values = (self.tamper_public_values)(public_values);

        let pub_vals_solidity: PublicValuesSolidity = tampered_public_values
            .try_into()
            .expect("Failed to convert public values to solidity");
        let public_values_bytes: Vec<u8> = PublicValuesSolidity::abi_encode(&pub_vals_solidity);

        tracing::info!("Sending report");
        let result = self
            .env
            .script_runtime
            .lido_infra
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

#[test]
fn check_vkey_matches() -> Result<()> {
    let sp1_client = &SP1_CLIENT;
    let test_files = test_utils::files::TestFiles::new_from_manifest_dir();
    let proof = test_files.read_proof(STORED_PROOF_FILE_NAME).map_err(eyre_to_anyhow)?;
    assert_eq!(sp1_client.vk().bytes32(), proof.vkey, "Vkey in stored proof and in client mismatch. Please run write_test_fixture script to generate new stored proof");
    Ok(())
}

#[test]
fn check_old_slot_matches() -> Result<()> {
    let test_files = test_utils::files::TestFiles::new_from_manifest_dir();
    let proof = test_files.read_proof(STORED_PROOF_FILE_NAME).map_err(eyre_to_anyhow)?;
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
        Err(err) => Err(anyhow::anyhow!("Error: {err:?}")),
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn report_tampering_no_tampering_resubmit_should_fail() -> Result<()> {
    let executor = TestExecutor::new(id).await?;

    executor.run_test().await.expect("Should succeed once");

    TestAssertions::assert_rejected_with(executor.run_test().await, |e| {
        matches!(e, Sp1LidoAccountingReportContractErrors::ReportAlreadyRecorded(_))
    })
}

#[tokio::test(flavor = "multi_thread")]
async fn report_tampering_report_slot() -> Result<()> {
    let executor = TestExecutor::new(wrap_report_mapper(|report| {
        let mut new_report = report.clone();
        new_report.reference_slot -= 1;
        new_report
    }))
    .await?;

    // Record the beacon block hash for the changed bc_slot
    executor
        .env
        .record_beacon_block_hash(test_utils::REPORT_COMPUTE_SLOT.0 - 1, test_utils::NONZERO_HASH.into())
        .await?;

    TestAssertions::assert_rejected_with(executor.run_test().await, |e| {
        matches!(e, Sp1LidoAccountingReportContractErrors::IllegalReferenceSlotError(_))
    })
}

#[tokio::test(flavor = "multi_thread")]
async fn report_tampering_report_slot2() -> Result<()> {
    let executor = TestExecutor::new(wrap_report_mapper(|report| {
        let mut new_report = report.clone();
        new_report.reference_slot += 10;
        new_report
    }))
    .await?;

    // Record the beacon block hash for the changed bc_slot
    executor
        .env
        .record_beacon_block_hash(test_utils::REPORT_COMPUTE_SLOT.0 + 10, test_utils::NONZERO_HASH.into())
        .await?;

    TestAssertions::assert_rejected_with(executor.run_test().await, |e| {
        matches!(e, Sp1LidoAccountingReportContractErrors::IllegalReferenceSlotError(_))
    })
}

#[tokio::test(flavor = "multi_thread")]
async fn report_tampering_report_cl_balance() -> Result<()> {
    let executor = TestExecutor::new(wrap_report_mapper(|report| {
        let mut new_report = report.clone();
        new_report.lido_cl_balance += 50;
        new_report
    }))
    .await?;

    TestAssertions::assert_rejected_with(executor.run_test().await, |e| {
        matches!(e, Sp1LidoAccountingReportContractErrors::Sp1VerificationError(_))
    })
}

#[tokio::test(flavor = "multi_thread")]
async fn report_tampering_report_deposited_count() -> Result<()> {
    let executor = TestExecutor::new(wrap_report_mapper(|report| {
        let mut new_report = report.clone();
        new_report.deposited_lido_validators += 1;
        new_report
    }))
    .await?;

    TestAssertions::assert_rejected_with(executor.run_test().await, |e| {
        matches!(e, Sp1LidoAccountingReportContractErrors::Sp1VerificationError(_))
    })
}

#[tokio::test(flavor = "multi_thread")]
async fn report_tampering_report_exited_count() -> Result<()> {
    let executor = TestExecutor::new(wrap_report_mapper(|report| {
        let mut new_report = report.clone();
        new_report.exited_lido_validators += 1;
        new_report
    }))
    .await?;

    TestAssertions::assert_rejected_with(executor.run_test().await, |e| {
        matches!(e, Sp1LidoAccountingReportContractErrors::Sp1VerificationError(_))
    })
}

#[tokio::test(flavor = "multi_thread")]
async fn report_tampering_metadata_slot() -> Result<()> {
    let executor = TestExecutor::new(wrap_metadata_mapper(|metadata| {
        let mut new_metadata = metadata.clone();
        new_metadata.bc_slot -= 10;
        new_metadata
    }))
    .await?;

    // Record the beacon block hash for the changed bc_slot
    executor
        .env
        .record_beacon_block_hash(test_utils::REPORT_COMPUTE_SLOT.0 - 10, test_utils::NONZERO_HASH.into())
        .await?;

    TestAssertions::assert_rejected_with(executor.run_test().await, |e| {
        matches!(e, Sp1LidoAccountingReportContractErrors::IllegalReferenceSlotError(_))
    })
}

#[tokio::test(flavor = "multi_thread")]
async fn report_tampering_metadata_slot2() -> Result<()> {
    let executor = TestExecutor::new(wrap_metadata_mapper(|metadata| {
        let mut new_metadata = metadata.clone();
        new_metadata.bc_slot += 10;
        new_metadata
    }))
    .await?;

    // Record the beacon block hash for the changed bc_slot
    executor
        .env
        .record_beacon_block_hash(test_utils::REPORT_COMPUTE_SLOT.0 + 10, test_utils::NONZERO_HASH.into())
        .await?;

    TestAssertions::assert_rejected_with(executor.run_test().await, |e| {
        matches!(e, Sp1LidoAccountingReportContractErrors::IllegalReferenceSlotError(_))
    })
}

#[tokio::test(flavor = "multi_thread")]
async fn report_tampering_metadata_epoch() -> Result<()> {
    let executor = TestExecutor::new(wrap_metadata_mapper(|metadata| {
        let mut new_metadata = metadata.clone();
        new_metadata.epoch = 9876543;
        new_metadata
    }))
    .await?;

    TestAssertions::assert_rejected_with(executor.run_test().await, |e| {
        matches!(e, Sp1LidoAccountingReportContractErrors::Sp1VerificationError(_))
    })
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

    TestAssertions::assert_rejected_with(executor.run_test().await, |e| {
        matches!(e, Sp1LidoAccountingReportContractErrors::VerificationError(_))
    })
}

#[tokio::test(flavor = "multi_thread")]
async fn report_tampering_metadata_beacon_block_hash() -> Result<()> {
    let executor = TestExecutor::new(wrap_metadata_mapper(|metadata| {
        let mut new_metadata = metadata.clone();
        new_metadata.beacon_block_hash = hex!("123456789000000000000000abcdefabcdefabcdefabcdefabcdefabcdefabcd");
        new_metadata
    }))
    .await?;

    TestAssertions::assert_rejected_with(executor.run_test().await, |e| {
        matches!(e, Sp1LidoAccountingReportContractErrors::BeaconBlockHashMismatch(_))
    })
}

#[tokio::test(flavor = "multi_thread")]
async fn report_tampering_metadata_old_state_slot() -> Result<()> {
    let executor = TestExecutor::new(wrap_metadata_mapper(|metadata| {
        let mut new_metadata = metadata.clone();
        new_metadata.state_for_previous_report.slot -= 10;
        new_metadata
    }))
    .await?;

    TestAssertions::assert_rejected_with(executor.run_test().await, |e| {
        matches!(e, Sp1LidoAccountingReportContractErrors::VerificationError(_))
    })
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

    TestAssertions::assert_rejected_with(executor.run_test().await, |e| {
        matches!(e, Sp1LidoAccountingReportContractErrors::VerificationError(_))
    })
}

#[tokio::test(flavor = "multi_thread")]
async fn report_tampering_metadata_new_state_slot() -> Result<()> {
    let executor = TestExecutor::new(wrap_metadata_mapper(|metadata| {
        let mut new_metadata = metadata.clone();
        new_metadata.new_state.slot += 10;
        new_metadata
    }))
    .await?;

    TestAssertions::assert_rejected_with(executor.run_test().await, |e| {
        matches!(e, Sp1LidoAccountingReportContractErrors::VerificationError(_))
    })
}

#[tokio::test(flavor = "multi_thread")]
async fn report_tampering_metadata_new_state_merkle_root() -> Result<()> {
    let executor = TestExecutor::new(wrap_metadata_mapper(|metadata| {
        let mut new_metadata = metadata.clone();
        new_metadata.new_state.merkle_root = hex!("1234567890000000000000000000000000000000000000000000000000000000");
        new_metadata
    }))
    .await?;

    TestAssertions::assert_rejected_with(executor.run_test().await, |e| {
        matches!(e, Sp1LidoAccountingReportContractErrors::Sp1VerificationError(_))
    })
}

#[tokio::test(flavor = "multi_thread")]
async fn report_tampering_withdrawal_wrong_address() -> Result<()> {
    let executor = TestExecutor::new(wrap_metadata_mapper(|metadata| {
        let mut new_metadata = metadata.clone();
        new_metadata.withdrawal_vault_data.vault_address = hex!("1234567890000000000000000000000000000000").into();
        new_metadata
    }))
    .await?;

    TestAssertions::assert_rejected_with(executor.run_test().await, |e| {
        matches!(e, Sp1LidoAccountingReportContractErrors::VerificationError(_))
    })
}

#[tokio::test(flavor = "multi_thread")]
async fn report_tampering_withdrawal_wrong_balance() -> Result<()> {
    let executor = TestExecutor::new(wrap_metadata_mapper(|metadata| {
        let mut new_metadata = metadata.clone();
        new_metadata.withdrawal_vault_data.balance = alloy_primitives::U256::from(1234567890u64);
        new_metadata
    }))
    .await?;

    TestAssertions::assert_rejected_with(executor.run_test().await, |e| {
        matches!(e, Sp1LidoAccountingReportContractErrors::VerificationError(_))
    })
}
