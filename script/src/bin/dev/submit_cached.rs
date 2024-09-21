use alloy::primitives::{Address, U256};
use clap::Parser;
use sp1_lido_accounting_scripts::consts::Network;
use sp1_lido_accounting_scripts::eth_client::{ProviderFactory, Sp1LidoAccountingReportContract};

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

    let provider = ProviderFactory::create_from_env().expect("Failed to create HTTP provider");
    let address: Address = env::var("CONTRACT_ADDRESS")
        .expect("Failed to read CONTRACT_ADDRESS env var")
        .parse()
        .expect("Failed to parse CONTRACT_ADDRESS into URL");
    let contract = Sp1LidoAccountingReportContract::new(address, provider);

    let file_name = format!("proof_{}_{}.json", network.as_str(), args.target_slot);
    let proof_file = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../temp/proofs")
        .join(file_name);
    let stored_proof = sp1_lido_accounting_scripts::read_proof_and_metadata(proof_file.as_path())
        .expect("failed to read cached proof");

    log::info!("Sending report");
    let tx_builder = contract.submitReportData(
        U256::from(args.target_slot),
        stored_proof.report.into(),
        stored_proof.metadata.into(),
        stored_proof.proof.into(),
        stored_proof.public_values.to_vec().into(),
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
