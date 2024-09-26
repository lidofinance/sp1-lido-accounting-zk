use clap::Parser;
use sp1_lido_accounting_scripts::consts::Network;
use sp1_lido_accounting_scripts::{proof_storage, scripts};

use std::env;
use std::path::PathBuf;

#[derive(Parser, Debug)]
#[clap(author, version, about, long_about = None)]
struct ProveArgs {
    #[clap(long, default_value = "5800000")]
    target_slot: u64,
}

#[tokio::main]
async fn main() {
    sp1_sdk::utils::setup_logger();
    let args = ProveArgs::parse();
    log::debug!("Args: {:?}", args);

    let chain = env::var("EVM_CHAIN").expect("Couldn't read EVM_CHAIN env var");
    let network = Network::from_str(&chain).unwrap();

    let contract = scripts::prelude::initialize_contract();

    let file_name = format!("proof_{}_{}.json", network.as_str(), args.target_slot);
    let proof_file = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../temp/proofs")
        .join(file_name);
    let stored_proof =
        proof_storage::read_proof_and_metadata(proof_file.as_path()).expect("failed to read cached proof");

    log::info!("Sending report");
    let tx_hash = contract
        .submit_report_data(
            args.target_slot,
            stored_proof.report,
            stored_proof.metadata,
            stored_proof.proof,
            stored_proof.public_values.to_vec(),
        )
        .await
        .expect("Failed to submit report");
    log::info!("Report transaction complete {}", hex::encode(tx_hash));
}
