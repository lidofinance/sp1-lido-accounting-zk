use anyhow::anyhow;
use clap::Parser;
use sp1_lido_accounting_scripts::beacon_state_reader::{BeaconStateReader, BeaconStateReaderEnum};
use sp1_lido_accounting_scripts::consts::Network;
use sp1_lido_accounting_scripts::script_logic::{prepare_program_input, verify_public_values};
use sp1_lido_accounting_scripts::{store_proof_and_metadata, ELF};
use sp1_sdk::{ProverClient, SP1ProofWithPublicValues, SP1ProvingKey, SP1Stdin, SP1VerifyingKey};
use std::path::PathBuf;

use sp1_lido_accounting_zk_shared::eth_consensus_layer::{BeaconBlockHeader, BeaconState, Hash256};
use sp1_lido_accounting_zk_shared::io::program_io::ProgramInput;

use anyhow::Result;
use log;

use std::env;

#[derive(Parser, Debug)]
#[clap(author, version, about, long_about = None)]
struct ProveArgs {
    #[clap(long, default_value = "5800000")]
    target_slot: u64,
    #[clap(long, default_value = "5000000")]
    previous_slot: u64,
}

struct ScriptSteps {
    client: ProverClient,
    pk: SP1ProvingKey,
    vk: SP1VerifyingKey,
}

impl ScriptSteps {
    pub fn new(client: ProverClient) -> Self {
        let (pk, vk) = client.setup(ELF);
        Self { client, pk, vk }
    }

    pub fn prove(&self, input: SP1Stdin) -> Result<SP1ProofWithPublicValues> {
        self.client.prove(&self.pk, input).plonk().run()
    }

    pub fn verify_proof(&self, proof: &SP1ProofWithPublicValues) -> Result<()> {
        log::info!("Verifying proof");
        self.client
            .verify(proof, &self.vk)
            .map_err(|err| anyhow!("Couldn't verify {:#?}", err))
    }
}

fn write_sp1_stdin(program_input: &ProgramInput) -> SP1Stdin {
    log::info!("Writing program input to SP1Stdin");
    let mut stdin: SP1Stdin = SP1Stdin::new();
    stdin.write(&program_input);
    stdin
}

async fn read_beacon_states(
    bs_reader: impl BeaconStateReader,
    target_slot: u64,
    previous_slot: u64,
) -> (BeaconState, BeaconBlockHeader, BeaconState) {
    let bs = bs_reader
        .read_beacon_state(target_slot)
        .await
        .expect("Failed to read beacon state");
    let bh = bs_reader
        .read_beacon_block_header(target_slot)
        .await
        .expect("Failed to read beacon block header");

    let old_bs = bs_reader
        .read_beacon_state(previous_slot)
        .await
        .expect("Failed to read previous beacon state");

    assert_eq!(bs.slot, target_slot);
    assert_eq!(bh.slot, target_slot);
    assert_eq!(old_bs.slot, previous_slot);

    (bs, bh, old_bs)
}

#[tokio::main]
async fn main() {
    sp1_sdk::utils::setup_logger();
    let args = ProveArgs::parse();
    log::debug!("Args: {:?}", args);

    let chain = env::var("EVM_CHAIN").expect("Couldn't read EVM_CHAIN env var");
    let network = Network::from_str(&chain).unwrap();
    let network_config = network.get_config();
    log::info!(
        "Running for network {:?}, slot: {}, previous_slot: {}",
        network,
        args.target_slot,
        args.previous_slot
    );
    let lido_withdrawal_credentials: Hash256 = network_config.lido_withdrawal_credentials.into();
    let bs_reader = BeaconStateReaderEnum::new_from_env(&network);

    let (bs, bh, old_bs) = read_beacon_states(bs_reader, args.target_slot, args.previous_slot).await;
    let (program_input, public_values) = prepare_program_input(&bs, &bh, &old_bs, &lido_withdrawal_credentials);

    let prover_client = ProverClient::network();
    let steps = ScriptSteps::new(prover_client);

    log::info!("Proving program");
    let stdin = write_sp1_stdin(&program_input);

    let proof = steps.prove(stdin).expect("Failed to generate proof");
    log::info!("Generated proof");

    steps.verify_proof(&proof).expect("Failed to verify proof");
    log::info!("Verified proof");

    verify_public_values(&proof.public_values, &public_values).expect("Failed to verify public inputs");
    log::info!("Verified public values");

    let fixture_file = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../contracts/src/fixtures/fixture.json");
    store_proof_and_metadata(&proof, &steps.vk, fixture_file.as_path());
}
