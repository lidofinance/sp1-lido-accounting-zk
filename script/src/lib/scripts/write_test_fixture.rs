use crate::beacon_state_reader::BeaconStateReader;

use crate::proof_storage;
use crate::scripts::shared as shared_logic;
use crate::sp1_client_wrapper::SP1ClientWrapper;

use std::path::PathBuf;

use log;
use sp1_lido_accounting_zk_shared::eth_consensus_layer::Hash256;

use std::env;

pub async fn run(
    client: SP1ClientWrapper,
    bs_reader: impl BeaconStateReader,
    target_slot: u64,
    previous_slot: u64,
    withdrawal_credentials: &[u8; 32],
) -> anyhow::Result<()> {
    let lido_withdrawal_credentials: Hash256 = withdrawal_credentials.into();
    let (target_bh, target_bs) = bs_reader.read_beacon_state_and_header(target_slot).await?;
    let (_old_bh, old_bs) = bs_reader.read_beacon_state_and_header(previous_slot).await?;

    let (program_input, public_values) =
        shared_logic::prepare_program_input(&target_bs, &target_bh, &old_bs, &lido_withdrawal_credentials);

    let proof = client.prove(program_input).expect("Failed to generate proof");
    log::info!("Generated proof");

    client.verify_proof(&proof).expect("Failed to verify proof");
    log::info!("Verified proof");

    shared_logic::verify_public_values(&proof.public_values, &public_values).expect("Failed to verify public inputs");
    log::info!("Verified public values");

    let fixture_file = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../contracts/src/fixtures/fixture.json");
    proof_storage::store_proof_and_metadata(&proof, client.vk(), fixture_file.as_path());
    anyhow::Ok(())
}
