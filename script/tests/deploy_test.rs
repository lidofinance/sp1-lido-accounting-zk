use std::{env, path::PathBuf};

use alloy::node_bindings::Anvil;
use alloy::transports::http::reqwest::Url;

use eyre::Result;
use sp1_lido_accounting_scripts::{
    eth_client::{ContractDeployParametersRust, ProviderFactory, Sp1LidoAccountingReportContractWrapper},
    utils,
};

#[tokio::test]
async fn deploy() -> Result<()> {
    simple_logger::init().unwrap();
    let deploy_args_file =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/data/deploy/anvil-sepolia-deploy.json");
    let deploy_params: ContractDeployParametersRust =
        utils::read_json(deploy_args_file).expect("Failed to read deployment args");

    let anvil = Anvil::new().block_time(1).try_spawn()?;
    let endpoint: Url = anvil.endpoint().parse()?;
    let key = anvil.keys()[0].clone();
    let provider = ProviderFactory::create_provider(key, endpoint);

    let slot = deploy_params.initial_validator_state.slot;

    let contract = Sp1LidoAccountingReportContractWrapper::deploy(provider.clone(), &deploy_params).await?;
    log::info!("Deployed contract at {}", contract.address());

    let latest_report_slot_response = contract.get_latest_report_slot().await?;
    assert_eq!(latest_report_slot_response, slot);
    Ok(())
}
