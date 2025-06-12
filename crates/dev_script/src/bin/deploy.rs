use std::path::PathBuf;

use clap::Parser;
use sp1_lido_accounting_dev_scripts::scripts as dev_scripts;
use sp1_lido_accounting_scripts::{
    consts::NetworkInfo,
    scripts::{self, prelude::EnvVars},
};
use sp1_lido_accounting_zk_shared::io::eth_io::BeaconChainSlot;

/*
Run variants:
* Prepare and save deploy manifesto, but don't deploy:
cargo run --bin deploy --release -- --target-slot 5887808 --store "../data/deploy/${EVM_CHAIN}-deploy.json" --dry-run

* Read from manifesto and deploy
cargo run --bin deploy --release -- --target-slot 5887808 --source "../data/deploy/${EVM_CHAIN}-deploy.json"

* Read from manifesto, deploy and verify
cargo run --bin deploy --release -- --target-slot 5887808 --source "../data/deploy/${EVM_CHAIN}-deploy.json" --verify

* Read from network and deploy, don't save manifest
cargo run --bin deploy --release -- --target-slot 5887808
*/

#[derive(Parser, Debug)]
#[clap(author, version, about, long_about = None)]
struct PreDeployArgs {
    #[clap(long)]
    target_slot: u64,
    #[clap(long, required = false)]
    source: Option<String>,
    #[clap(long, required = false)]
    store: Option<String>,
    #[clap(long, default_value = "false")]
    dry_run: bool,
    #[clap(long, default_value = "false")]
    verify: bool,
}

#[tokio::main]
async fn main() {
    sp1_sdk::utils::setup_logger();
    let args = PreDeployArgs::parse();

    let env_vars = EnvVars::init_from_env_or_crash();

    let script_runtime = scripts::prelude::ScriptRuntime::init(&env_vars)
        .expect("Failed to initialize script runtime");

    tracing::info!(
        "Running pre-deploy for network {:?}, slot: {}",
        script_runtime.network().as_str(),
        args.target_slot
    );

    let source = if let Some(path) = args.source {
        dev_scripts::deploy::Source::File {
            slot: args.target_slot,
            path: PathBuf::from(path),
        }
    } else {
        let verifier_address = std::env::var("SP1_VERIFIER_ADDRESS")
            .expect("SP1_VERIFIER_ADDRESS not set")
            .parse()
            .expect("Failed to parse SP1_VERIFIER_ADDRESS to Address");
        let owner_address = std::env::var("OWNER_ADDRESS")
            .expect("OWNER_ADDRESS not set")
            .parse()
            .expect("Failed to parse OWNER_ADDRESS to Address");
        dev_scripts::deploy::Source::Network {
            slot: BeaconChainSlot(args.target_slot),
            verifier: verifier_address,
            owner: owner_address,
        }
    };

    let network_config = script_runtime.network().get_config();

    let verification = if args.verify {
        let constracts_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../contracts/");
        dev_scripts::deploy::Verification::Verify {
            contracts_path: constracts_dir,
            chain_id: network_config.chain_id,
        }
    } else {
        dev_scripts::deploy::Verification::Skip
    };

    dev_scripts::deploy::run(
        &script_runtime,
        source,
        args.store,
        args.dry_run,
        verification,
    )
    .await
    .expect("Failed to run `deploy");
}
