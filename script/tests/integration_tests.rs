use alloy::node_bindings::{Anvil, AnvilInstance};
use alloy::transports::http::reqwest::Url;
use anyhow::{Context, Result};
use sp1_lido_accounting_scripts::{
    beacon_state_reader::{BeaconStateReader, BeaconStateReaderEnum, StateId},
    consts::{self, NetworkInfo},
    eth_client::{Contract, EthELClient, ProviderFactory, Sp1LidoAccountingReportContractWrapper},
    scripts,
    sp1_client_wrapper::{SP1ClientWrapper, SP1ClientWrapperImpl},
};
use sp1_lido_accounting_zk_shared::{
    eth_consensus_layer::BeaconState,
    eth_spec,
    io::eth_io::{BeaconChainSlot, HaveSlotWithBlock},
};
use sp1_sdk::ProverClient;
use std::{env, sync::Arc};
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

    let contract = Sp1LidoAccountingReportContractWrapper::deploy(Arc::new(provider), &deploy_params)
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
    let network = &test_utils::NETWORK;
    let deploy_slot = test_utils::DEPLOY_SLOT;
    let (_anvil, sp1_client, el_client, bs_reader, contract, finalized_slot) = set_up(network, deploy_slot).await?;

    scripts::submit::run(
        &sp1_client,
        &bs_reader,
        &contract,
        &el_client,
        mark_as_refslot(finalized_slot),
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
    let network = &test_utils::NETWORK;
    let deploy_slot = test_utils::DEPLOY_SLOT;
    let (_anvil, sp1_client, el_client, bs_reader, contract, finalized_slot) = set_up(network, deploy_slot).await?;
    let intermediate_slot: BeaconChainSlot = finalized_slot - eth_spec::SlotsPerEpoch::to_u64();

    scripts::submit::run(
        &sp1_client,
        &bs_reader,
        &contract,
        &el_client,
        mark_as_refslot(intermediate_slot),
        None, // alternatively Some(deploy_slot) should do the same
        network.clone(),
        scripts::submit::Flags {
            verify: true,
            store: true,
        },
    )
    .await
    .context("Failed to perform deploy -> intermediate update")?;

    scripts::submit::run(
        &sp1_client,
        &bs_reader,
        &contract,
        &el_client,
        mark_as_refslot(finalized_slot),
        None, // alternatively Some(first_run_slot) should do the same
        network.clone(),
        scripts::submit::Flags {
            verify: true,
            store: true,
        },
    )
    .await
    .context("Failed to perform intermediate -> finalized update")?;
    Ok(())
}

// Note: this will hit SP1 prover network - will take noticeable time (a few mins) and might incur
// costs.
#[ignore]
#[tokio::test(flavor = "multi_thread")]
async fn non_latest_state_success() -> Result<()> {
    let network = &test_utils::NETWORK;
    let deploy_slot = test_utils::DEPLOY_SLOT;
    let (_anvil, sp1_client, el_client, bs_reader, contract, finalized_slot) = set_up(network, deploy_slot).await?;
    let intermediate_slot: BeaconChainSlot = finalized_slot - eth_spec::SlotsPerEpoch::to_u64();

    scripts::submit::run(
        &sp1_client,
        &bs_reader,
        &contract,
        &el_client,
        mark_as_refslot(intermediate_slot),
        Some(mark_as_refslot(deploy_slot)),
        network.clone(),
        scripts::submit::Flags {
            verify: true,
            store: true,
        },
    )
    .await
    .context("Failed to run perform deploy -> intermediate update")?;

    scripts::submit::run(
        &sp1_client,
        &bs_reader,
        &contract,
        &el_client,
        mark_as_refslot(finalized_slot),
        Some(mark_as_refslot(deploy_slot)),
        network.clone(),
        scripts::submit::Flags {
            verify: true,
            store: true,
        },
    )
    .await
    .context("Failed to perform deploy -> finalized update")?;
    Ok(())
}

// Note: this will hit SP1 prover network - will take noticeable time (a few mins) and might incur
// costs.
#[ignore]
#[tokio::test(flavor = "multi_thread")]
async fn resubmit_success() -> Result<()> {
    let network = &test_utils::NETWORK;
    let deploy_slot = test_utils::DEPLOY_SLOT;
    let (_anvil, sp1_client, el_client, bs_reader, contract, finalized_slot) = set_up(network, deploy_slot).await?;

    scripts::submit::run(
        &sp1_client,
        &bs_reader,
        &contract,
        &el_client,
        mark_as_refslot(finalized_slot),
        Some(mark_as_refslot(deploy_slot)),
        network.clone(),
        scripts::submit::Flags {
            verify: true,
            store: true,
        },
    )
    .await
    .context("Failed to run perform initial deploy -> finalized update")?;

    scripts::submit::run(
        &sp1_client,
        &bs_reader,
        &contract,
        &el_client,
        mark_as_refslot(finalized_slot),
        Some(mark_as_refslot(deploy_slot)),
        network.clone(),
        scripts::submit::Flags {
            verify: true,
            store: true,
        },
    )
    .await
    .context("Failed to run perform repeated deploy -> finalized update")?;
    Ok(())
}

async fn set_up(
    network: &impl NetworkInfo,
    deploy_slot: BeaconChainSlot,
) -> Result<(
    // Anvil instance is terminated when variable holding a reference to it goes out of scope
    // So the variable ownership need to be assumed by the test function
    AnvilInstance,
    SP1ClientWrapperImpl,
    EthELClient,
    BeaconStateReaderEnum,
    Contract,
    BeaconChainSlot,
)> {
    sp1_sdk::utils::setup_logger();

    let sp1_client = SP1ClientWrapperImpl::new(ProverClient::network(), consts::ELF);
    let bs_reader = BeaconStateReaderEnum::new_from_env(network);

    let test_files = TestFiles::new_from_manifest_dir();

    let deploy_bs: BeaconState = test_files
        .read_beacon_state(&StateId::Slot(deploy_slot))
        .await
        .map_err(eyre_to_anyhow)?;
    let deploy_params = scripts::deploy::prepare_deploy_params(sp1_client.vk_bytes(), &deploy_bs, network);

    let finalized_block_header = bs_reader.read_beacon_block_header(&StateId::Finalized).await?;
    let finalized_slot: BeaconChainSlot = finalized_block_header.bc_slot();

    let finalized_bs = test_utils::read_latest_bs_at_or_before(&bs_reader, finalized_slot, test_utils::RETRIES)
        .await
        .map_err(eyre_to_anyhow)?;

    let fork_url = env::var("FORK_URL").expect("FORK_URL env var must be specified");
    let anvil = Anvil::new()
        .fork(fork_url)
        .fork_block_number(finalized_bs.latest_execution_payload_header.block_number + 2)
        .try_spawn()?;
    let provider = ProviderFactory::create_provider(anvil.keys()[0].clone(), anvil.endpoint().parse()?);
    let prov = Arc::new(provider);

    log::info!("Deploying contract with parameters {:?}", deploy_params);
    let contract = Sp1LidoAccountingReportContractWrapper::deploy(Arc::clone(&prov), &deploy_params)
        .await
        .map_err(eyre_to_anyhow)?;
    let el_client = EthELClient::new(Arc::clone(&prov));
    log::info!("Deployed contract at {}", contract.address());

    Ok((anvil, sp1_client, el_client, bs_reader, contract, finalized_slot))
}
