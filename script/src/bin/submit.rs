use clap::Parser;
use sp1_lido_accounting_scripts::scripts;
use sp1_lido_accounting_zk_shared::io::eth_io::ReferenceSlot;

// cargo run --bin submit --release -- --target-slot 5982336 --store --local-verify

#[derive(Parser, Debug)]
#[clap(author, version, about, long_about = None)]
struct ProveArgs {
    #[clap(long, default_value = "5800000")]
    target_ref_slot: u64,
    #[clap(long, required = false)]
    previous_ref_slot: Option<u64>,
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
    let (eth_client, contract) = scripts::prelude::initialize_eth();

    let tx_hash = scripts::submit::run(
        &client,
        &bs_reader,
        &contract,
        &eth_client,
        ReferenceSlot(args.target_ref_slot),
        args.previous_ref_slot.map(ReferenceSlot),
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
