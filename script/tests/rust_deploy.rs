use std::{fs, path::PathBuf};

use alloy::node_bindings::Anvil;
use alloy::transports::http::reqwest::Url;

use sp1_lido_accounting_scripts::eth_client::{ProviderFactory, Sp1LidoAccountingReportContractWrapper};
use sp1_lido_accounting_zk_shared::io::eth_io::ContractDeployParametersRust;

use eyre::Result;

#[tokio::test]
async fn main() -> Result<()> {
    simple_logger::init().unwrap();
    let deploy_args_file = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/data/anvil-sepolia-deploy.json");

    println!("{}", deploy_args_file.display());
    let deploy_args_str = fs::read(deploy_args_file)?;
    let deploy_params: ContractDeployParametersRust = serde_json::from_slice(deploy_args_str.as_slice())?;

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
