use crate::beacon_state_reader::BeaconStateReader;
use crate::scripts::shared as shared_logic;
use crate::sp1_client_wrapper::SP1ClientWrapper;

use sp1_lido_accounting_zk_shared::eth_consensus_layer::Hash256;

pub async fn run(
    client: SP1ClientWrapper,
    bs_reader: impl BeaconStateReader,
    target_slot: u64,
    previous_slot: u64,
    withdrawal_credentials: &[u8; 32],
) -> anyhow::Result<()> {
    let lido_withdrawal_credentials: Hash256 = withdrawal_credentials.into();

    let target_bh = bs_reader.read_beacon_block_header(target_slot).await?;
    let target_bs = bs_reader.read_beacon_state(target_slot).await?;
    let old_bs = bs_reader.read_beacon_state(previous_slot).await?;

    let (program_input, public_values) =
        shared_logic::prepare_program_input(&target_bs, &target_bh, &old_bs, &lido_withdrawal_credentials);

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
