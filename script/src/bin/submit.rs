use alloy::primitives::{Address, U256};
use anyhow::anyhow;
use clap::Parser;
use sp1_lido_accounting_scripts::beacon_state_reader::{BeaconStateReader, BeaconStateReaderEnum};
use sp1_lido_accounting_scripts::consts::Network;
use sp1_lido_accounting_scripts::eth_client::{ProviderFactory, Sp1LidoAccountingReportContract};
use sp1_lido_accounting_scripts::script_logic::{prepare_program_input, verify_public_values};

use sp1_sdk::{ProverClient, SP1ProofWithPublicValues, SP1ProvingKey, SP1Stdin, SP1VerifyingKey};

use sp1_lido_accounting_zk_shared::eth_consensus_layer::{BeaconBlockHeader, BeaconState, Hash256};
use sp1_lido_accounting_zk_shared::io::program_io::ProgramInput;

use anyhow::Result;
use log;

use std::env;
use std::path::PathBuf;

#[derive(Parser, Debug)]
#[clap(author, version, about, long_about = None)]
struct ProveArgs {
    #[clap(long, default_value = "5800000")]
    target_slot: u64,
    #[clap(long, required = false)]
    previous_slot: Option<u64>,
    #[clap(long, required = false)]
    store: bool,
}

struct ScriptSteps {
    client: ProverClient,
    pk: SP1ProvingKey,
    vk: SP1VerifyingKey,
}

impl ScriptSteps {
    pub fn new(client: ProverClient) -> Self {
        let (pk, vk) = client.setup(sp1_lido_accounting_scripts::ELF);
        Self { client, pk, vk }
    }

    pub fn vk(&self) -> &'_ SP1VerifyingKey {
        &self.vk
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

    let lido_withdrawal_credentials: Hash256 = network_config.lido_withdrawal_credentials.into();
    let bs_reader = BeaconStateReaderEnum::new_from_env(&network);
    let provider = ProviderFactory::create_from_env().expect("Failed to create HTTP provider");
    let address: Address = env::var("CONTRACT_ADDRESS")
        .expect("Failed to read CONTRACT_ADDRESS env var")
        .parse()
        .expect("Failed to parse CONTRACT_ADDRESS into URL");
    let contract = Sp1LidoAccountingReportContract::new(address, provider);

    let previous_slot = if let Some(prev) = args.previous_slot {
        prev
    } else {
        let latest_report_response = contract
            .getLatestLidoValidatorStateSlot()
            .call()
            .await
            .expect("Failed to read latest report slot from contract");
        let latest_report_slot = latest_report_response._0;
        latest_report_slot.to::<u64>()
    };

    log::info!(
        "Submitting report for network {:?}, slot: {}, previous_slot: {}",
        network,
        args.target_slot,
        previous_slot,
    );

    let (bs, bh, old_bs) = read_beacon_states(bs_reader, args.target_slot, previous_slot).await;
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

    if args.store {
        let file_name = format!("proof_{}_{}.json", network.as_str(), args.target_slot);
        let proof_file = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../temp/proofs")
            .join(file_name);
        sp1_lido_accounting_scripts::store_proof_and_metadata(&proof, steps.vk(), proof_file.as_path());
    }

    log::info!("Sending report");
    let tx_builder = contract.submitReportData(
        U256::from(bs.slot),
        public_values.report.into(),
        public_values.metadata.into(),
        proof.bytes().into(),
        proof.public_values.to_vec().into(),
    );
    let tx_call = tx_builder.send().await;

    if let Err(alloy::contract::Error::TransportError(alloy::transports::RpcError::ErrorResp(error_payload))) = tx_call
    {
        if let Some(revert_bytes) = error_payload.as_revert_data() {
            let err = sp1_lido_accounting_scripts::eth_client::Error::parse_rejection(revert_bytes.to_vec());
            panic!("Failed to submit report {:#?}", err);
        } else {
            panic!("Error payload {:#?}", error_payload);
        }
    } else if let Ok(tx) = tx_call {
        log::info!("Waiting for report transaction");
        let tx_result = tx.watch().await.expect("Failed to wait for confirmation");
        log::info!("Report transaction complete {}", hex::encode(tx_result.0));
    }
}
