use clap::Parser;
use sp1_lido_accounting_scripts::{consts::NetworkInfo, scripts, tracing::LoggingConfig};
use sp1_lido_accounting_zk_shared::io::eth_io::ReferenceSlot;

#[derive(Parser, Debug)]
#[clap(author, version, about, long_about = None)]
struct ExecuteArgs {
    #[clap(long)]
    target_ref_slot: u64,
    #[clap(long)]
    previous_ref_slot: Option<u64>,
}

#[tokio::main]
async fn main() {
    sp1_lido_accounting_scripts::tracing::setup_logger(LoggingConfig::default().use_json(true));
    let args = ExecuteArgs::parse();
    tracing::debug!("Args: {:?}", args);

    let (network, client, bs_reader) = scripts::prelude::initialize();
    let (eth_client, contract) = scripts::prelude::initialize_eth();

    let main_span = tracing::info_span!("main", network = network.as_str()).entered();

    tracing::info!(
        "Running for network {:?}, slot: {}, previous_slot: {:?}",
        network,
        args.target_ref_slot,
        args.previous_ref_slot
    );

    scripts::execute::run(
        &client,
        &bs_reader,
        &contract,
        &eth_client,
        ReferenceSlot(args.target_ref_slot),
        args.previous_ref_slot.map(ReferenceSlot),
        &network.get_config(),
    )
    .await
    .expect("Failed to run `execute");

    main_span.exit();
}
