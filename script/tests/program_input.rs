mod test_utils;

use sp1_lido_accounting_scripts::{
    beacon_state_reader::StateId, consts::NetworkInfo, scripts::shared::prepare_program_input,
};
use test_utils::{files::TestFiles, mark_as_refslot, DEPLOY_SLOT, REPORT_COMPUTE_SLOT};
use thiserror::Error;

#[derive(Debug, Error)]
enum TestError {
    #[error("Eyre error: {0:#?}")]
    Eyre(eyre::Error),
    #[error("Anyhow error: {0:#?}")]
    Anyhow(anyhow::Error),
}

impl From<eyre::Error> for TestError {
    fn from(value: eyre::Error) -> Self {
        TestError::Eyre(value)
    }
}

impl From<anyhow::Error> for TestError {
    fn from(value: anyhow::Error) -> Self {
        TestError::Anyhow(value)
    }
}

type Result<T> = std::result::Result<T, TestError>;

#[tokio::test(flavor = "multi_thread")]
async fn program_input_integration_test() -> Result<()> {
    let test_files = TestFiles::new_from_manifest_dir();

    let network = &test_utils::NETWORK;
    let network_config = network.get_config();

    let old_state_id = StateId::Slot(DEPLOY_SLOT);
    let report_state_id = StateId::Slot(REPORT_COMPUTE_SLOT);
    let report_refslot = mark_as_refslot(REPORT_COMPUTE_SLOT);

    let old_bs = test_files.read_beacon_state(&old_state_id).await?;
    let new_bs = test_files.read_beacon_state(&report_state_id).await?;
    let new_bh = test_files.read_beacon_block_header(&report_state_id).await?;

    // sanity-check
    let lido_validator_ids = [1973, 1974, 1975, 1976, 1977, 1978];
    let balances: Vec<u64> = lido_validator_ids.iter().map(|idx| new_bs.balances[*idx]).collect();
    let cl_balance_sum: u64 = balances.iter().sum();
    assert_eq!(cl_balance_sum, 96000663228);

    let withdrawal_vault_data = test_files.read_withdrawal_vault_data(&report_state_id).await?;
    let expected_wv_balance = withdrawal_vault_data.balance;

    let (_program_input, public_values) = prepare_program_input(
        report_refslot,
        &new_bs,
        &new_bh,
        &old_bs,
        &network_config.lido_withdrawal_credentials.into(),
        withdrawal_vault_data,
        true,
    );

    assert_eq!(public_values.report.lido_cl_balance, cl_balance_sum);
    assert_eq!(public_values.report.deposited_lido_validators, 6);
    assert_eq!(public_values.report.exited_lido_validators, 3);
    assert_eq!(public_values.report.reference_slot, report_refslot);
    assert_eq!(public_values.report.lido_withdrawal_vault_balance, expected_wv_balance);
    Ok(())
}
