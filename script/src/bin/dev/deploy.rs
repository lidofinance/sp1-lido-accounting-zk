use std::path::PathBuf;

use clap::Parser;
use sp1_lido_accounting_scripts::{consts::NetworkInfo, scripts};
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

    let (network, client, bs_reader) = scripts::prelude::initialize();
    let provider = scripts::prelude::initialize_provider();

    tracing::info!(
        "Running pre-deploy for network {:?}, slot: {}",
        network,
        args.target_slot
    );

    let source = if let Some(path) = args.source {
        scripts::deploy::Source::File {
            slot: args.target_slot,
            path: PathBuf::from(path),
        }
    } else {
        scripts::deploy::Source::Network {
            slot: BeaconChainSlot(args.target_slot),
        }
    };

    let network_config = network.get_config();

    let verification = if args.verify {
        let constracts_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../contracts/");
        scripts::deploy::Verification::Verify {
            contracts_path: constracts_dir,
            chain_id: network_config.chain_id,
        }
    } else {
        scripts::deploy::Verification::Skip
    };

    scripts::deploy::run(
        client,
        bs_reader,
        source,
        provider,
        network,
        args.store,
        args.dry_run,
        verification,
    )
    .await
    .expect("Failed to run `deploy");
}
