use clap::Parser;
use sp1_lido_accounting_scripts::{consts::NetworkInfo, scripts};

#[derive(Parser, Debug)]
#[clap(author, version, about, long_about = None)]
struct ExecuteArgs {
    #[clap(long, default_value = "5800000")]
    target_slot: u64,
    #[clap(long, default_value = "5000000")]
    previous_slot: u64,
}

#[tokio::main]
async fn main() {
    sp1_sdk::utils::setup_logger();
    let args = ExecuteArgs::parse();
    log::debug!("Args: {:?}", args);

    let (network, client, bs_reader) = scripts::prelude::initialize();

    log::info!(
        "Running for network {:?}, slot: {}, previous_slot: {}",
        network,
        args.target_slot,
        args.previous_slot
    );

    scripts::execute::run(
        client,
        bs_reader,
        args.target_slot,
        args.previous_slot,
        &network.get_config().lido_withdrawal_credentials,
    )
    .await
    .expect("Failed to run `execute");
}
