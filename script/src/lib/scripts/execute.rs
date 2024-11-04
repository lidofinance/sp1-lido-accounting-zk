use crate::beacon_state_reader::{BeaconStateReader, StateId};
use crate::scripts::shared as shared_logic;
use crate::sp1_client_wrapper::SP1ClientWrapper;
use sp1_lido_accounting_zk_shared::eth_consensus_layer::Hash256;
use sp1_lido_accounting_zk_shared::io::eth_io::ReferenceSlot;
use tokio::try_join;

pub async fn run(
    client: &impl SP1ClientWrapper,
    bs_reader: &impl BeaconStateReader,
    target_slot: ReferenceSlot,
    previous_slot: ReferenceSlot,
    withdrawal_credentials: &[u8; 32],
) -> anyhow::Result<()> {
    let (actual_target_slot, actual_previous_slot) = try_join!(
        bs_reader.find_bc_slot_for_refslot(target_slot),
        bs_reader.find_bc_slot_for_refslot(previous_slot)
    )?;
    let target_state_id = StateId::Slot(actual_target_slot);
    let old_state_id = StateId::Slot(actual_previous_slot);
    let ((target_bh, target_bs), (_old_bh, old_bs)) = try_join!(
        bs_reader.read_beacon_state_and_header(&target_state_id),
        bs_reader.read_beacon_state_and_header(&old_state_id)
    )?;

    let lido_withdrawal_credentials: Hash256 = withdrawal_credentials.into();

    let (program_input, public_values) = shared_logic::prepare_program_input(
        target_slot,
        &target_bs,
        &target_bh,
        &old_bs,
        &lido_withdrawal_credentials,
        true,
    );

    log::info!("Executing program");
    let (exec_public_values, execution_report) = client.execute(program_input).unwrap();

    log::info!(
        "Executed program with {} cycles",
        execution_report.total_instruction_count() + execution_report.total_syscall_count()
    );
    log::debug!("Full execution report:\n{}", execution_report);

    shared_logic::verify_public_values(&exec_public_values, &public_values).expect("Failed to verify public inputs");
    log::info!("Successfully verified public values!");
    anyhow::Ok(())
}
