use std::{fs, path::PathBuf};

use alloy::node_bindings::Anvil;
use alloy::primitives::U256;
use alloy::transports::http::reqwest::Url;

use sp1_lido_accounting_scripts::eth_client::{ProviderFactory, Sp1LidoAccountingReportContract};
use sp1_lido_accounting_zk_shared::io::eth_io::ContractDeployParametersRust;

use eyre::Result;

#[tokio::test]
async fn main() -> Result<()> {
    simple_logger::init().unwrap();
    let deploy_args_file =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../contracts/script/deploy_manifesto_mainnet.json");

    println!("{}", deploy_args_file.display());
    let deploy_args_str = fs::read(deploy_args_file)?;
    let deploy_params: ContractDeployParametersRust = serde_json::from_slice(deploy_args_str.as_slice())?;

    let anvil = Anvil::new().block_time(1).try_spawn()?;
    let endpoint: Url = anvil.endpoint().parse()?;
    let key = anvil.keys()[0].clone();
    let provider = ProviderFactory::create(endpoint, key)?;

    let slot = deploy_params.initial_validator_state.slot;

    let contract = Sp1LidoAccountingReportContract::deploy(
        provider.clone(),
        deploy_params.verifier.into(),
        deploy_params.vkey.into(),
        deploy_params.withdrawal_credentials.into(),
        U256::from(deploy_params.genesis_timestamp),
        deploy_params.initial_validator_state.into(),
    )
    .await?;
    log::info!("Deployed contract at {}", contract.address());

    let latest_report_slot_response = contract.getLatestLidoValidatorStateSlot().call().await?;
    assert_eq!(latest_report_slot_response._0, U256::from(slot));
    Ok(())
}
