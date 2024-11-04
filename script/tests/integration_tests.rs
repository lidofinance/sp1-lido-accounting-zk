use alloy::node_bindings::Anvil;
use alloy::transports::http::reqwest::Url;
use anyhow::Result;
use sp1_lido_accounting_scripts::{
    beacon_state_reader::{BeaconStateReader, BeaconStateReaderEnum, StateId},
    consts,
    eth_client::{ProviderFactory, Sp1LidoAccountingReportContractWrapper},
    scripts,
    sp1_client_wrapper::{SP1ClientWrapper, SP1ClientWrapperImpl},
};
use sp1_lido_accounting_zk_shared::{eth_consensus_layer::BeaconState, eth_spec, io::eth_io::HaveSlotWithBlock};
use sp1_sdk::ProverClient;
use std::env;
use test_utils::{eyre_to_anyhow, mark_as_refslot, TestFiles};
use typenum::Unsigned;
mod test_utils;

#[tokio::test]
async fn deploy() -> Result<()> {
    let test_files = TestFiles::new_from_manifest_dir();
    let deploy_slot = test_utils::DEPLOY_SLOT;
    let deploy_params = test_files
        .read_deploy(&test_utils::NETWORK, deploy_slot)
        .map_err(eyre_to_anyhow)?;

    let anvil = Anvil::new().block_time(1).try_spawn()?;
    let endpoint: Url = anvil.endpoint().parse()?;
    let key = anvil.keys()[0].clone();
    let provider = ProviderFactory::create_provider(key, endpoint);

    let contract = Sp1LidoAccountingReportContractWrapper::deploy(provider.clone(), &deploy_params)
        .await
        .map_err(eyre_to_anyhow)?;
    log::info!("Deployed contract at {}", contract.address());

    let latest_report_slot_response = contract.get_latest_validator_state_slot().await?;
    assert_eq!(latest_report_slot_response, deploy_slot);
    Ok(())
}

// Note: this will hit SP1 prover network - will take noticeable time (a few mins) and might incur
// costs.
#[ignore]
#[tokio::test(flavor = "multi_thread")]
async fn submission_success() -> Result<()> {
    sp1_sdk::utils::setup_logger();
    let network = &test_utils::NETWORK;
    let client = SP1ClientWrapperImpl::new(ProverClient::network(), consts::ELF);
    let bs_reader = BeaconStateReaderEnum::new_from_env(network);

    let test_files = TestFiles::new_from_manifest_dir();
    let deploy_slot = test_utils::DEPLOY_SLOT;
    let deploy_bs: BeaconState = test_files
        .read_beacon_state(&StateId::Slot(deploy_slot))
        .await
        .map_err(eyre_to_anyhow)?;
    let deploy_params = scripts::deploy::prepare_deploy_params(client.vk_bytes(), &deploy_bs, network);

    let finalized_block_header = bs_reader.read_beacon_block_header(&StateId::Finalized).await?;
    let bc_slot = finalized_block_header.bc_slot();
    let target_slot = mark_as_refslot(bc_slot);
    let finalized_bs = test_utils::read_latest_bs_at_or_before(&bs_reader, bc_slot, test_utils::RETRIES)
        .await
        .map_err(eyre_to_anyhow)?;
    let fork_url = env::var("FORK_URL").expect("FORK_URL env var must be specified");
    let anvil = Anvil::new()
        .fork(fork_url)
        .fork_block_number(finalized_bs.latest_execution_payload_header.block_number + 2)
        .try_spawn()?;
    let provider = ProviderFactory::create_provider(anvil.keys()[0].clone(), anvil.endpoint().parse()?);

    log::info!("Deploying contract with parameters {:?}", deploy_params);
    let contract = Sp1LidoAccountingReportContractWrapper::deploy(provider.clone(), &deploy_params)
        .await
        .map_err(eyre_to_anyhow)?;
    log::info!("Deployed contract at {}", contract.address());

    scripts::submit::run(
        &client,
        &bs_reader,
        &contract,
        target_slot,
        None, // alternatively Some(deploy_slot) should do the same
        network.clone(),
        scripts::submit::Flags {
            verify: true,
            store: true,
        },
    )
    .await
    .expect("Failed to execute script");
    Ok(())
}

// Note: this will hit SP1 prover network - will take noticeable time (a few mins) and might incur
// costs.
#[ignore]
#[tokio::test(flavor = "multi_thread")]
async fn two_submission_success() -> Result<()> {
    sp1_sdk::utils::setup_logger();
    let network = &test_utils::NETWORK;
    let client = SP1ClientWrapperImpl::new(ProverClient::network(), consts::ELF);
    let bs_reader = BeaconStateReaderEnum::new_from_env(network);

    let test_files = TestFiles::new_from_manifest_dir();
    let deploy_slot = test_utils::DEPLOY_SLOT;
    let deploy_bs: BeaconState = test_files
        .read_beacon_state(&StateId::Slot(deploy_slot))
        .await
        .map_err(eyre_to_anyhow)?;
    let deploy_params = scripts::deploy::prepare_deploy_params(client.vk_bytes(), &deploy_bs, network);

    let finalized_block_header = bs_reader.read_beacon_block_header(&StateId::Finalized).await?;
    let finalized_bs =
        test_utils::read_latest_bs_at_or_before(&bs_reader, finalized_block_header.bc_slot(), test_utils::RETRIES)
            .await
            .map_err(eyre_to_anyhow)?;
    let fork_url = env::var("FORK_URL").expect("FORK_URL env var must be specified");
    let anvil = Anvil::new()
        .fork(fork_url)
        .fork_block_number(finalized_bs.latest_execution_payload_header.block_number + 2)
        .try_spawn()?;
    let provider = ProviderFactory::create_provider(anvil.keys()[0].clone(), anvil.endpoint().parse()?);

    log::info!("Deploying contract with parameters {:?}", deploy_params);
    let contract = Sp1LidoAccountingReportContractWrapper::deploy(provider.clone(), &deploy_params)
        .await
        .map_err(eyre_to_anyhow)?;
    log::info!("Deployed contract at {}", contract.address());

    let first_run_slot = mark_as_refslot(finalized_block_header.bc_slot() - eth_spec::SlotsPerEpoch::to_u64());
    scripts::submit::run(
        &client,
        &bs_reader,
        &contract,
        first_run_slot,
        None, // alternatively Some(deploy_slot) should do the same
        network.clone(),
        scripts::submit::Flags {
            verify: true,
            store: true,
        },
    )
    .await
    .expect("Failed to execute script");

    let second_run_slot = mark_as_refslot(finalized_block_header.bc_slot());
    scripts::submit::run(
        &client,
        &bs_reader,
        &contract,
        second_run_slot,
        None, // alternatively Some(first_run_slot) should do the same
        network.clone(),
        scripts::submit::Flags {
            verify: true,
            store: true,
        },
    )
    .await?;
    Ok(())
}
