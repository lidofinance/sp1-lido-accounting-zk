use crate::beacon_state_reader::BeaconStateReader;
use crate::sp1_client_wrapper::SP1ClientWrapper;

use sp1_lido_accounting_zk_shared::eth_consensus_layer::Hash256;

use std::{env, path::PathBuf};
use tree_hash::TreeHash;

use crate::consts::Network;
use sp1_lido_accounting_zk_shared::{
    io::eth_io::{ContractDeployParametersRust, LidoValidatorStateRust},
    lido::LidoValidatorState,
};

pub async fn run(
    client: SP1ClientWrapper,
    bs_reader: impl BeaconStateReader,
    target_slot: u64,
    network: Network,
) -> anyhow::Result<()> {
    let network_config = network.get_config();
    let network_name = network.as_str().to_owned();
    let lido_withdrawal_credentials: Hash256 = network_config.lido_withdrawal_credentials.into();
    let target_bs = bs_reader.read_beacon_state(target_slot).await?;

    let lido_validator_state = LidoValidatorState::compute_from_beacon_state(&target_bs, &lido_withdrawal_credentials);

    let deploy_manifesto = ContractDeployParametersRust {
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

    log::info!("Deploy manifesto {:?}", deploy_manifesto);

    let deploy_manifesto_path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../contracts/script/");
    let deploy_manifesto_file = deploy_manifesto_path.join(format!("deploy_manifesto_{network_name}.json"));
    std::fs::create_dir_all(&deploy_manifesto_path)?;
    std::fs::write(
        deploy_manifesto_file.clone(),
        serde_json::to_string_pretty(&deploy_manifesto).unwrap(),
    )?;
    log::info!("Successfully written deploy manifesto to {deploy_manifesto_file:?}");
    anyhow::Ok(())
}
