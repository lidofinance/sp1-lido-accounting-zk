use std::path::PathBuf;

use clap::Parser;
use sp1_lido_accounting_scripts::scripts;

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
}

#[tokio::main]
async fn main() {
    sp1_sdk::utils::setup_logger();
    let args = PreDeployArgs::parse();

    let (network, client, bs_reader) = scripts::prelude::initialize();
    let provider = scripts::prelude::initialize_provider();

    log::info!(
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
        scripts::deploy::Source::Network { slot: args.target_slot }
    };

    scripts::deploy::run(client, bs_reader, source, provider, network, args.store, args.dry_run)
        .await
        .expect("Failed to run `deploy");
}
