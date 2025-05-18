use crate::beacon_state_reader::{BeaconStateReader, StateId};
use crate::consts::NetworkConfig;
use crate::eth_client::{Contract, EthELClient};
use crate::scripts::shared as shared_logic;
use crate::sp1_client_wrapper::SP1ClientWrapper;

use sp1_lido_accounting_zk_shared::eth_consensus_layer::Hash256;
use sp1_lido_accounting_zk_shared::io::eth_io::{BeaconChainSlot, ReferenceSlot};
use tokio::try_join;

async fn get_previous_bc_slot(
    maybe_previous_ref_slot: Option<ReferenceSlot>,
    bs_reader: &impl BeaconStateReader,
    contract: &Contract,
) -> anyhow::Result<BeaconChainSlot> {
    let result = match maybe_previous_ref_slot {
        Some(prev) => bs_reader.find_bc_slot_for_refslot(prev).await?,
        None => contract.get_latest_validator_state_slot().await?,
    };
    Ok(result)
}

pub async fn run(
    client: &impl SP1ClientWrapper,
    bs_reader: &impl BeaconStateReader,
    contract: &Contract,
    eth_client: &EthELClient,
    target_slot: ReferenceSlot,
    maybe_previous_slot: Option<ReferenceSlot>,
    network_config: &NetworkConfig,
) -> anyhow::Result<()> {
    let (actual_target_slot, actual_previous_slot) = try_join!(
        bs_reader.find_bc_slot_for_refslot(target_slot),
        get_previous_bc_slot(maybe_previous_slot, bs_reader, contract),
    )?;
    let target_state_id = StateId::Slot(actual_target_slot);
    let old_state_id = StateId::Slot(actual_previous_slot);
    let ((target_bh, target_bs), (_old_bh, old_bs)) = try_join!(
        bs_reader.read_beacon_state_and_header(&target_state_id),
        bs_reader.read_beacon_state_and_header(&old_state_id)
    )?;

    let lido_withdrawal_credentials: Hash256 = network_config.lido_withdrawal_credentials.into();

    let execution_layer_block_hash = target_bs.latest_execution_payload_header.block_hash;
    let withdrawal_vault_data = eth_client
        .get_withdrawal_vault_data(
            network_config.lido_withdrwawal_vault_address.into(),
            execution_layer_block_hash,
        )
        .await?;

    let (program_input, public_values) = shared_logic::prepare_program_input(
        target_slot,
        &target_bs,
        &target_bh,
        &old_bs,
        &lido_withdrawal_credentials,
        withdrawal_vault_data,
        true,
    );

    tracing::info!("Executing program");
    let (exec_public_values, execution_report) = client.execute(program_input).unwrap();

    tracing::info!(
        "Executed program with {} cycles",
        execution_report.total_instruction_count() + execution_report.total_syscall_count()
    );
    tracing::debug!("Full execution report:\n{}", execution_report);

    shared_logic::verify_public_values(&exec_public_values, &public_values).expect("Failed to verify public inputs");
    tracing::info!("Successfully verified public values!");
    anyhow::Ok(())
}
