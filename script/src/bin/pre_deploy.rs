use clap::Parser;
use sp1_lido_accounting_scripts::scripts;

#[derive(Parser, Debug)]
#[clap(author, version, about, long_about = None)]
struct PreDeployArgs {
    #[clap(long)]
    target_slot: u64,
}

#[tokio::main]
async fn main() {
    sp1_sdk::utils::setup_logger();
    let args = PreDeployArgs::parse();

    let (network, client, bs_reader) = scripts::prelude::initialize();

    log::info!(
        "Running pre-deploy for network {:?}, slot: {}",
        network,
        args.target_slot
    );

    scripts::pre_deploy::run(client, bs_reader, args.target_slot, network)
        .await
        .expect("Failed to run `pre-deploy");
}
