use crate::consts::NetworkInfo;
use crate::eth_client::ContractDeployParametersRust;
use sp1_lido_accounting_zk_shared::eth_consensus_layer::{BeaconState, Hash256};

use tree_hash::TreeHash;

use sp1_lido_accounting_zk_shared::{io::eth_io::LidoValidatorStateRust, lido::LidoValidatorState};

pub fn prepare_deploy_params(
    vkey: [u8; 32],
    target_bs: &BeaconState,
    network: &impl NetworkInfo,
) -> ContractDeployParametersRust {
    let network_config = network.get_config();
    let network_name = network.as_str();
    let lido_withdrawal_credentials: Hash256 = network_config.lido_withdrawal_credentials.into();
    let lido_validator_state = LidoValidatorState::compute_from_beacon_state(target_bs, &lido_withdrawal_credentials);

    ContractDeployParametersRust {
        network: network_name.clone(),
        verifier: network_config.verifier,
        vkey,
        withdrawal_credentials: lido_withdrawal_credentials.0,
        withdrawal_vault_address: network_config.lido_withdrwawal_vault_address,
        genesis_timestamp: network_config.genesis_block_timestamp,
        initial_validator_state: LidoValidatorStateRust {
            slot: lido_validator_state.slot,
            merkle_root: lido_validator_state.tree_hash_root().0,
        },
    }
}
