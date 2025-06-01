use clap::Parser;
use sp1_lido_accounting_scripts::consts::NetworkInfo;
use sp1_lido_accounting_scripts::scripts::prelude::EnvVars;
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
    tracing::debug!("Args: {:?}", args);
    let env_vars = EnvVars::init_from_env_or_crash();

    let script_runtime = scripts::prelude::ScriptRuntime::init(&env_vars)
        .expect("Failed to initialize script runtime");
    let network = script_runtime.network();

    let file_name = format!("proof_{}_{}.json", network.as_str(), args.target_slot);
    let proof_file =
        PathBuf::from(env::var("PROOF_CACHE_DIR").expect("Couldn't read PROOF_CACHE_DIR env var"))
            .join(file_name);
    let stored_proof = proof_storage::read_proof_and_metadata(proof_file.as_path())
        .expect("failed to read cached proof");

    tracing::info!("Sending report");
    let tx_hash = script_runtime
        .lido_infra
        .report_contract
        .submit_report_data(stored_proof.proof, stored_proof.public_values.to_vec())
        .await
        .expect("Failed to submit report");
    tracing::info!("Report transaction complete {}", hex::encode(tx_hash));
}
