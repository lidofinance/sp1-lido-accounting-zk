use clap::Parser;
use sp1_lido_accounting_scripts::scripts;

#[derive(Parser, Debug)]
#[clap(author, version, about, long_about = None)]
struct ProveArgs {
    #[clap(long, default_value = "5800000")]
    target_slot: u64,
    #[clap(long, required = false)]
    previous_slot: Option<u64>,
    #[clap(long, required = false)]
    store: bool,
}

#[tokio::main]
async fn main() {
    sp1_sdk::utils::setup_logger();
    let args = ProveArgs::parse();
    log::debug!("Args: {:?}", args);

    let (network, client, bs_reader) = scripts::prelude::initialize();
    let contract = scripts::prelude::initialize_contract();

    let tx_hash = scripts::submit::run(
        client,
        bs_reader,
        contract,
        args.target_slot,
        args.previous_slot,
        network,
        scripts::submit::Flags {
            verify: true,
            store: args.store,
        },
    )
    .await
    .expect("Failed to run `submit");
    log::info!("Report transaction complete {}", hex::encode(tx_hash));
}
