use sp1_lido_accounting_scripts::beacon_state_reader::{BeaconStateReader, StateId};
use sp1_lido_accounting_scripts::consts::NetworkInfo;
use sp1_lido_accounting_scripts::eth_client::{
    ContractDeployParametersRust, Sp1LidoAccountingReportContractWrapper,
};
use sp1_lido_accounting_scripts::scripts::prelude::ScriptRuntime;
use sp1_lido_accounting_scripts::sp1_client_wrapper::SP1ClientWrapper;
use sp1_lido_accounting_scripts::utils;

use alloy::providers::WalletProvider;
use alloy_primitives::Address;
use sp1_lido_accounting_zk_shared::io::eth_io::BeaconChainSlot;

use std::sync::Arc;
use std::{
    path::{Path, PathBuf},
    process::Command,
};

pub enum Source {
    Network { slot: BeaconChainSlot },
    File { slot: u64, path: PathBuf },
}

async fn compute_from_network(
    client: &impl SP1ClientWrapper,
    bs_reader: &impl BeaconStateReader,
    network: &impl NetworkInfo,
    target_slot: BeaconChainSlot,
) -> anyhow::Result<ContractDeployParametersRust> {
    let target_bs = bs_reader
        .read_beacon_state(&StateId::Slot(target_slot))
        .await?;

    Ok(sp1_lido_accounting_scripts::deploy::prepare_deploy_params(
        client.vk_bytes(),
        &target_bs,
        network,
    ))
}

async fn read_from_file(slot: u64, file: &Path) -> anyhow::Result<ContractDeployParametersRust> {
    tracing::info!(
        "Reading deploy parameters for {} from {:?}",
        slot,
        file.as_os_str()
    );
    let deploy_params: ContractDeployParametersRust = utils::read_json(file)?;
    if deploy_params.initial_validator_state.slot.0 == slot {
        Ok(deploy_params)
    } else {
        Err(anyhow::anyhow!("Slot from stored manifesto != target slot"))
    }
}

async fn verify(contracts_dir: &Path, address: &Address, chain_id: u64) -> anyhow::Result<()> {
    let address_str = hex::encode(address);
    tracing::debug!("Contracts folder {:#?}", contracts_dir.as_os_str());
    tracing::info!("Verifying contract at {}", address_str);

    let mut command = Command::new("forge");
    command
        .current_dir(contracts_dir)
        .arg("verify-contract")
        .arg(address_str)
        .arg("Sp1LidoAccountingReportContract")
        .args(["--chain_id", &chain_id.to_string()])
        .arg("--watch");

    tracing::debug!("Verification command {:#?}", command);
    command.status()?;
    tracing::info!("Verified successfully");
    Ok(())
}

pub enum Verification {
    Skip,
    Verify {
        contracts_path: PathBuf,
        chain_id: u64,
    },
}

pub async fn run(
    runtime: &ScriptRuntime,
    source: Source,
    write_manifesto: Option<String>,
    dry_run: bool,
    verification: Verification,
) -> anyhow::Result<()> {
    if let Verification::Verify {
        contracts_path,
        chain_id,
    } = verification
    {
        panic!("Verification is not yet supported");
    }
    let deploy_params = match source {
        Source::Network { slot } => {
            compute_from_network(
                &runtime.sp1_client,
                runtime.bs_reader(),
                runtime.network(),
                slot,
            )
            .await?
        }
        Source::File { slot, path } => read_from_file(slot, &path).await?,
    };

    if let Some(store_manifesto_file_str) = write_manifesto {
        let store_manifesto_file = PathBuf::from(store_manifesto_file_str);
        tracing::debug!(
            "Writing manifesto to {:?}",
            store_manifesto_file.as_os_str()
        );
        if let Some(parent_folder) = store_manifesto_file.parent() {
            std::fs::create_dir_all(parent_folder).expect("Failed to create parent folder");
        }
        std::fs::write(
            store_manifesto_file,
            serde_json::to_string_pretty(&deploy_params).unwrap(),
        )?;
        tracing::info!("Deploy manifesto {:?}", deploy_params);
    }

    if dry_run {
        tracing::info!("Dryrun is set, not deploying");
        return anyhow::Ok(());
    }

    tracing::info!("Deploying contract");
    tracing::debug!(
        "Deploying as {}",
        hex::encode(runtime.provider.default_signer_address())
    );
    let deployed = Sp1LidoAccountingReportContractWrapper::deploy(
        Arc::clone(&runtime.provider),
        &deploy_params,
    )
    .await
    // .map_err(|e| anyhow::anyhow!("Failed to deploy {:?}", e))?;
    .expect("Failed to deploy");
    tracing::info!("Deployed contract to {}", deployed.address());

    match verification {
        Verification::Skip => tracing::info!("Skipping verification"),
        Verification::Verify {
            contracts_path,
            chain_id,
        } => verify(contracts_path.as_path(), deployed.address(), chain_id).await?,
    }

    anyhow::Ok(())
}
