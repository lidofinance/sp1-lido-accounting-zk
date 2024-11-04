use clap::Parser;
use sp1_lido_accounting_scripts::{consts::NetworkInfo, scripts};
use sp1_lido_accounting_zk_shared::io::eth_io::ReferenceSlot;

#[derive(Parser, Debug)]
#[clap(author, version, about, long_about = None)]
struct ExecuteArgs {
    #[clap(long, default_value = "5800000")]
    target_ref_slot: u64,
    #[clap(long, default_value = "5000000")]
    previous_ref_slot: u64,
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
        args.target_ref_slot,
        args.previous_ref_slot
    );

    scripts::execute::run(
        &client,
        &bs_reader,
        ReferenceSlot(args.target_ref_slot),
        ReferenceSlot(args.previous_ref_slot),
        &network.get_config().lido_withdrawal_credentials,
    )
    .await
    .expect("Failed to run `execute");
}
