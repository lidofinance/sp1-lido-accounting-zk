use alloy::node_bindings::Anvil;
use alloy::transports::http::reqwest::Url;
use anyhow::{Context, Result};
use sp1_lido_accounting_scripts::{
    eth_client::{ProviderFactory, Sp1LidoAccountingReportContractWrapper},
    scripts,
};
use sp1_lido_accounting_zk_shared::{eth_spec, io::eth_io::BeaconChainSlot};
use std::sync::Arc;
use test_utils::env::IntegrationTestEnvironment;
use test_utils::DEPLOY_SLOT;
mod test_utils;
use test_utils::{eyre_to_anyhow, files::TestFiles, mark_as_refslot};
use typenum::Unsigned;

const DEFAULT_FLAGS: scripts::submit::Flags = scripts::submit::Flags {
    verify_input: true,
    verify_proof: false,
    dry_run: false,
    report_cycles: false,
};

mod success_tests {
    use super::*;
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
        tracing::info!("Deployed contract at {}", contract.address());

        let latest_report_slot_response = contract.get_latest_validator_state_slot().await?;
        assert_eq!(latest_report_slot_response, deploy_slot);
        Ok(())
    }

    // Note: this will hit SP1 prover network - will take noticeable time (a few mins) and might incur
    // costs.
    #[ignore]
    #[tokio::test(flavor = "multi_thread")]
    async fn submission_success() -> Result<()> {
        let env = IntegrationTestEnvironment::default().await?;
        let finalized_slot = env.get_finalized_slot().await?;

        scripts::submit::run(
            &env.script_runtime,
            Some(mark_as_refslot(finalized_slot)),
            None, // alternatively Some(deploy_slot) should do the same
            &DEFAULT_FLAGS,
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
        let env = IntegrationTestEnvironment::default().await?;
        let finalized_slot = env.get_finalized_slot().await?;
        let intermediate_slot = finalized_slot - eth_spec::SlotsPerEpoch::to_u64();

        scripts::submit::run(
            &env.script_runtime,
            Some(mark_as_refslot(intermediate_slot)),
            None, // alternatively Some(deploy_slot) should do the same
            &DEFAULT_FLAGS,
        )
        .await
        .context("Failed to perform deploy -> intermediate update")?;

        scripts::submit::run(
            &env.script_runtime,
            Some(mark_as_refslot(finalized_slot)),
            None, // alternatively Some(deploy_slot) should do the same
            &DEFAULT_FLAGS,
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
        let deploy_slot = DEPLOY_SLOT;
        let env = IntegrationTestEnvironment::new(test_utils::NETWORK.clone(), deploy_slot, None).await?;
        let finalized_slot = env.get_finalized_slot().await?;
        let intermediate_slot: BeaconChainSlot = finalized_slot - eth_spec::SlotsPerEpoch::to_u64();

        scripts::submit::run(
            &env.script_runtime,
            Some(mark_as_refslot(intermediate_slot)),
            Some(mark_as_refslot(deploy_slot)),
            &DEFAULT_FLAGS,
        )
        .await
        .context("Failed to run perform deploy -> intermediate update")?;

        scripts::submit::run(
            &env.script_runtime,
            Some(mark_as_refslot(finalized_slot)),
            Some(mark_as_refslot(deploy_slot)),
            &DEFAULT_FLAGS,
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
        let deploy_slot = DEPLOY_SLOT;
        let env = IntegrationTestEnvironment::new(test_utils::NETWORK.clone(), deploy_slot, None).await?;
        let finalized_slot = env.get_finalized_slot().await?;

        scripts::submit::run(
            &env.script_runtime,
            Some(mark_as_refslot(finalized_slot)),
            Some(mark_as_refslot(deploy_slot)),
            &DEFAULT_FLAGS,
        )
        .await
        .context("Failed to run perform initial deploy -> finalized update")?;

        scripts::submit::run(
            &env.script_runtime,
            Some(mark_as_refslot(finalized_slot)),
            Some(mark_as_refslot(deploy_slot)),
            &DEFAULT_FLAGS,
        )
        .await
        .context("Failed to run perform repeated deploy -> finalized update")?;
        Ok(())
    }
}
