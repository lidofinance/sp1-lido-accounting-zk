use crate::beacon_state_reader::BeaconStateReader;
use crate::eth_client::Sp1LidoAccountingReportContractWrapper;
use crate::sp1_client_wrapper::SP1ClientWrapper;

use sp1_lido_accounting_zk_shared::eth_consensus_layer::Hash256;

use std::{
    fs,
    path::{Path, PathBuf},
};
use tree_hash::TreeHash;

use crate::consts::NetworkInfo;
use sp1_lido_accounting_zk_shared::{
    io::eth_io::{ContractDeployParametersRust, LidoValidatorStateRust},
    lido::LidoValidatorState,
};

use super::prelude::DefaultProvider;

pub enum Source {
    Network { slot: u64 },
    File { slot: u64, path: PathBuf },
}

async fn read_form_network(
    client: SP1ClientWrapper,
    bs_reader: impl BeaconStateReader,
    network: impl NetworkInfo,
    target_slot: u64,
) -> anyhow::Result<ContractDeployParametersRust> {
    let network_config = network.get_config();
    let network_name = network.as_str();
    let lido_withdrawal_credentials: Hash256 = network_config.lido_withdrawal_credentials.into();
    let target_bs = bs_reader.read_beacon_state(target_slot).await?;
    let lido_validator_state = LidoValidatorState::compute_from_beacon_state(&target_bs, &lido_withdrawal_credentials);

    let result = ContractDeployParametersRust {
        network: network_name.clone(),
        verifier: network_config.verifier,
        vkey: client.vk_bytes(),
        withdrawal_credentials: lido_withdrawal_credentials.to_fixed_bytes(),
        genesis_timestamp: network_config.genesis_block_timestamp,
        initial_validator_state: LidoValidatorStateRust {
            slot: lido_validator_state.slot,
            merkle_root: lido_validator_state.tree_hash_root().to_fixed_bytes(),
        },
    };
    Ok(result)
}

async fn read_from_file(slot: u64, file: &Path) -> anyhow::Result<ContractDeployParametersRust> {
    log::info!("Reading deploy parameters for {} from {:?}", slot, file.as_os_str());
    let file_content = fs::read(file)?;
    let deploy_params: ContractDeployParametersRust = serde_json::from_slice(file_content.as_slice())?;
    if deploy_params.initial_validator_state.slot == slot {
        Ok(deploy_params)
    } else {
        Err(anyhow::anyhow!("Slot from stored manifesto != target slot"))
    }
}

pub async fn run(
    client: SP1ClientWrapper,
    bs_reader: impl BeaconStateReader,
    source: Source,
    provider: DefaultProvider,
    network: impl NetworkInfo,
    write_manifesto: Option<String>,
    dry_run: bool,
) -> anyhow::Result<()> {
    let deploy_params = match source {
        Source::Network { slot } => read_form_network(client, bs_reader, network, slot).await?,
        Source::File { slot, path } => read_from_file(slot, &path).await?,
    };

    if let Some(store_manifesto_file_str) = write_manifesto {
        let store_manifesto_file = PathBuf::from(store_manifesto_file_str);
        log::debug!("Writing manifesto to {:?}", store_manifesto_file.as_os_str());
        if let Some(parent_folder) = store_manifesto_file.parent() {
            std::fs::create_dir_all(parent_folder).expect("Failed to create parent folder");
        }
        std::fs::write(
            store_manifesto_file,
            serde_json::to_string_pretty(&deploy_params).unwrap(),
        )?;
        log::info!("Deploy manifesto {:?}", deploy_params);
    }

    if !dry_run {
        log::info!("Deploying contract");
        let deployed = Sp1LidoAccountingReportContractWrapper::deploy(provider, &deploy_params)
            .await
            // .map_err(|e| anyhow::anyhow!("Failed to deploy {:?}", e))?;
            .expect("Failed to deploy");
        log::info!("Deployed contract to {}", deployed.address())
    } else {
        log::info!("Dryrun is set, not deploying");
    }

    anyhow::Ok(())
}
