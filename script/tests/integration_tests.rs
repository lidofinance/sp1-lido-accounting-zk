use alloy::node_bindings::Anvil;
use alloy::transports::http::reqwest::Url;
use eyre::Result;
use sp1_lido_accounting_scripts::eth_client::{ProviderFactory, Sp1LidoAccountingReportContractWrapper};
use std::env;
use test_utils::TestFiles;

mod test_utils;

#[tokio::test]
async fn deploy() -> Result<()> {
    let test_files = TestFiles::new_from_manifest_dir();
    let deploy_slot = test_utils::DEPLOY_SLOT;
    let deploy_params = test_files.read_deploy(&test_utils::NETWORK, deploy_slot)?;

    let anvil = Anvil::new().block_time(1).try_spawn()?;
    let endpoint: Url = anvil.endpoint().parse()?;
    let key = anvil.keys()[0].clone();
    let provider = ProviderFactory::create_provider(key, endpoint);

    let contract = Sp1LidoAccountingReportContractWrapper::deploy(provider.clone(), &deploy_params).await?;
    log::info!("Deployed contract at {}", contract.address());

    let latest_report_slot_response = contract.get_latest_report_slot().await?;
    assert_eq!(latest_report_slot_response, deploy_slot);
    Ok(())
}

#[tokio::test]
async fn submission_success() -> Result<()> {
    let test_files = TestFiles::new_from_manifest_dir();
    let deploy_slot = test_utils::DEPLOY_SLOT;
    let deploy_params = test_files.read_deploy(&test_utils::NETWORK, deploy_slot)?;
    let stored_proof = test_files.read_proof("proof_anvil-sepolia_5930016.json")?;
    let target_slot = stored_proof.metadata.slot;

    let fork_url = env::var("FORK_URL").expect("FORK_URL env var must be specified");
    let anvil = Anvil::new()
        .fork(fork_url)
        .fork_block_number(test_utils::DEPLOY_BLOCK)
        .try_spawn()?;
    let provider = ProviderFactory::create_provider(anvil.keys()[0].clone(), anvil.endpoint().parse()?);

    let contract = Sp1LidoAccountingReportContractWrapper::deploy(provider.clone(), &deploy_params).await?;
    log::info!("Deployed contract at {}", contract.address());

    let report_copy = stored_proof.report.clone();

    log::info!("Sending report");
    let tx_hash = contract
        .submit_report_data(
            stored_proof.metadata.slot,
            stored_proof.report,
            stored_proof.metadata,
            stored_proof.proof,
            stored_proof.public_values.to_vec(),
        )
        .await
        .expect("Failed to submit report");

    log::info!("Report submission successful: tx hash {}", hex::encode(tx_hash));

    log::info!("Reading report from contract");
    let report_data = contract.get_report(target_slot).await.expect("Failed to read report");

    assert_eq!(report_data, report_copy);
    log::info!("Report match");
    Ok(())
}
