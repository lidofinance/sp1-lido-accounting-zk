use clap::Parser;
use sp1_lido_accounting_scripts::scripts;
use sp1_lido_accounting_zk_shared::io::eth_io::ReferenceSlot;

// cargo run --bin submit --release -- --target-slot 5982336 --store --local-verify

#[derive(Parser, Debug)]
#[clap(author, version, about, long_about = None)]
struct ProveArgs {
    #[clap(long, required = false)]
    target_ref_slot: Option<u64>,
    #[clap(long, required = false)]
    previous_ref_slot: Option<u64>,
    #[clap(long, required = false)]
    store_proof: bool,
    #[clap(long, required = false)]
    store_input: bool,
    #[clap(long, required = false)]
    local_verify: bool,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    sp1_sdk::utils::setup_logger();
    let args = ProveArgs::parse();
    tracing::debug!("Args: {:?}", args);

    let script_runtime = scripts::prelude::ScriptRuntime::init_from_env().expect("Failed to initialize script runtime");

    let tx_hash = scripts::submit::run(
        &script_runtime,
        args.target_ref_slot.map(ReferenceSlot),
        args.previous_ref_slot.map(ReferenceSlot),
        scripts::submit::Flags {
            verify: args.local_verify,
            store_proof: args.store_proof,
            store_input: args.store_input,
        },
    )
    .await?;
    tracing::info!("Report transaction complete {}", hex::encode(tx_hash));
    Ok(())
}
