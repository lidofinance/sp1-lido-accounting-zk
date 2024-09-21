use clap::Parser;
use sp1_sdk::{HashableKey, ProverClient};
use std::{env, path::PathBuf};
use tree_hash::TreeHash;

use sp1_lido_accounting_scripts::beacon_state_reader::{BeaconStateReader, BeaconStateReaderEnum};
use sp1_lido_accounting_scripts::consts::Network;
use sp1_lido_accounting_zk_shared::{
    eth_consensus_layer::Hash256,
    io::eth_io::{ContractDeployParametersRust, LidoValidatorStateRust},
    lido::LidoValidatorState,
};

#[derive(Parser, Debug)]
#[clap(author, version, about, long_about = None)]
struct PreDeployArgs {
    #[clap(long)]
    slot: u64,
}

fn read_network() -> Network {
    let chain = env::var("EVM_CHAIN").expect("Couldn't read EVM_CHAIN env var");
    Network::from_str(&chain).unwrap()
}

#[tokio::main]
async fn main() {
    sp1_sdk::utils::setup_logger();
    let args = PreDeployArgs::parse();
    let slot = args.slot;

    let network = read_network();
    let network_config = network.get_config();
    let network_name = network.as_str().to_owned();
    log::info!("Running for network {:?}, slot: {}", network, args.slot);

    let bs_reader: BeaconStateReaderEnum = BeaconStateReaderEnum::new_from_env(&network);
    let lido_withdrawal_credentials: Hash256 = network_config.lido_withdrawal_credentials.into();

    let bs = bs_reader
        .read_beacon_state(slot)
        .await
        .expect("Failed to read beacon state");
    assert_eq!(bs.slot, slot);

    let bh = bs_reader
        .read_beacon_block_header(slot)
        .await
        .expect("Failed to read beacon block header");
    assert_eq!(bh.slot, slot);

    let lido_validator_state = LidoValidatorState::compute_from_beacon_state(&bs, &lido_withdrawal_credentials);

    let prover_client = ProverClient::local();
    let (_pk, vk) = prover_client.setup(sp1_lido_accounting_scripts::ELF);
    let mut vk_bytes: [u8; 32] = [0; 32];
    let vk = vk.bytes32();
    let stripped_vk = vk.strip_prefix("0x").unwrap_or(&vk);
    hex::decode_to_slice(stripped_vk.as_bytes(), &mut vk_bytes).expect("Failed to decode verification key to [u8; 32]");

    let deploy_manifesto = ContractDeployParametersRust {
        network: network_name.clone(),
        verifier: network_config.verifier,
        vkey: vk_bytes,
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
    std::fs::create_dir_all(&deploy_manifesto_path).expect("failed to create deploy manifesto path");
    std::fs::write(
        deploy_manifesto_file.clone(),
        serde_json::to_string_pretty(&deploy_manifesto).unwrap(),
    )
    .expect("failed to write deploy manifest0");
    log::info!("Successfully written deploy manifesto to {deploy_manifesto_file:?}");
}
