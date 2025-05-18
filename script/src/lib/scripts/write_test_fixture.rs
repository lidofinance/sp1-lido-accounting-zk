use crate::beacon_state_reader::{BeaconStateReader, StateId};

use crate::consts::NetworkConfig;
use crate::eth_client::EthELClient;
use crate::scripts::shared as shared_logic;
use crate::sp1_client_wrapper::SP1ClientWrapper;
use crate::{proof_storage, utils};

use sp1_lido_accounting_zk_shared::io::program_io::WithdrawalVaultData;
use std::path::{Path, PathBuf};
use tokio::try_join;

use alloy_primitives::Address;
use sp1_lido_accounting_zk_shared::eth_consensus_layer::Hash256;
use sp1_lido_accounting_zk_shared::io::eth_io::ReferenceSlot;

fn store_withdrawal_vault_data(data: &WithdrawalVaultData, proof_file: &Path) {
    utils::write_json(proof_file, &data).expect("failed to write fixture");
}

pub async fn run(
    client: &impl SP1ClientWrapper,
    bs_reader: &impl BeaconStateReader,
    eth_client: &EthELClient,
    target_slot: ReferenceSlot,
    previous_slot: ReferenceSlot,
    network_config: &NetworkConfig,
    fixture_files: Vec<PathBuf>,
    withdrawal_vault_fixture_files: Vec<PathBuf>,
) -> anyhow::Result<()> {
    let (actual_target_slot, actual_previous_slot) = try_join!(
        bs_reader.find_bc_slot_for_refslot(target_slot),
        bs_reader.find_bc_slot_for_refslot(previous_slot)
    )?;
    let target_state_id = StateId::Slot(actual_target_slot);
    let previous_state_id = StateId::Slot(actual_previous_slot);
    let ((target_bh, target_bs), (_old_bh, old_bs)) = try_join!(
        bs_reader.read_beacon_state_and_header(&target_state_id),
        bs_reader.read_beacon_state_and_header(&previous_state_id)
    )?;

    let lido_withdrawal_credentials: Hash256 = network_config.lido_withdrawal_credentials.into();

    let lido_withdrawal_vault: Address = network_config.lido_withdrwawal_vault_address.into();
    let execution_layer_block_hash = target_bs.latest_execution_payload_header.block_hash;
    let withdrawal_vault_data = eth_client
        .get_withdrawal_vault_data(lido_withdrawal_vault, execution_layer_block_hash)
        .await?;

    for fixture_file in withdrawal_vault_fixture_files {
        store_withdrawal_vault_data(&withdrawal_vault_data, fixture_file.as_path());
    }

    let (program_input, public_values) = shared_logic::prepare_program_input(
        ReferenceSlot(target_slot.0),
        &target_bs,
        &target_bh,
        &old_bs,
        &lido_withdrawal_credentials,
        withdrawal_vault_data,
        true,
    );

    let proof = client.prove(program_input).expect("Failed to generate proof");
    tracing::info!("Generated proof");

    client.verify_proof(&proof).expect("Failed to verify proof");
    tracing::info!("Verified proof");

    shared_logic::verify_public_values(&proof.public_values, &public_values).expect("Failed to verify public inputs");
    tracing::info!("Verified public values");

    for fixture_file in fixture_files {
        proof_storage::store_proof_and_metadata(&proof, client.vk(), fixture_file.as_path());
    }

    anyhow::Ok(())
}
