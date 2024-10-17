use anyhow;
use clap::Parser;
use sp1_lido_accounting_scripts::scripts;

// cargo run --bin submit --release -- --target-slot 5982336 --store --local-verify

#[derive(Parser, Debug)]
#[clap(author, version, about, long_about = None)]
struct ProveArgs {
    #[clap(long, default_value = "5800000")]
    target_slot: u64,
    #[clap(long, required = false)]
    previous_slot: Option<u64>,
    #[clap(long, required = false)]
    store: bool,
    #[clap(long, required = false)]
    local_verify: bool,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    sp1_sdk::utils::setup_logger();
    let args = ProveArgs::parse();
    log::debug!("Args: {:?}", args);

    let (network, client, bs_reader) = scripts::prelude::initialize();
    let contract = scripts::prelude::initialize_contract();

    let tx_hash = scripts::submit::run(
        &client,
        &bs_reader,
        contract,
        args.target_slot,
        args.previous_slot,
        network,
        scripts::submit::Flags {
            verify: args.local_verify,
            store: args.store,
        },
    )
    .await?;
    log::info!("Report transaction complete {}", hex::encode(tx_hash));
    Ok(())
}
