use crate::beacon_state_reader::{BeaconStateReader, StateId};

use crate::proof_storage;
use crate::scripts::shared as shared_logic;
use crate::sp1_client_wrapper::SP1ClientWrapper;

use std::path::PathBuf;
use tokio::try_join;

use log;
use sp1_lido_accounting_zk_shared::eth_consensus_layer::Hash256;
use sp1_lido_accounting_zk_shared::io::eth_io::ReferenceSlot;

pub async fn run(
    client: impl SP1ClientWrapper,
    bs_reader: impl BeaconStateReader,
    target_slot: ReferenceSlot,
    previous_slot: ReferenceSlot,
    withdrawal_credentials: &[u8; 32],
    fixture_files: Vec<PathBuf>,
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

    let lido_withdrawal_credentials: Hash256 = withdrawal_credentials.into();

    let (program_input, public_values) = shared_logic::prepare_program_input(
        ReferenceSlot(target_slot.0),
        &target_bs,
        &target_bh,
        &old_bs,
        &lido_withdrawal_credentials,
        true,
    );

    let proof = client.prove(program_input).expect("Failed to generate proof");
    log::info!("Generated proof");

    client.verify_proof(&proof).expect("Failed to verify proof");
    log::info!("Verified proof");

    shared_logic::verify_public_values(&proof.public_values, &public_values).expect("Failed to verify public inputs");
    log::info!("Verified public values");

    for fixture_file in fixture_files {
        proof_storage::store_proof_and_metadata(&proof, client.vk(), fixture_file.as_path());
    }

    anyhow::Ok(())
}
