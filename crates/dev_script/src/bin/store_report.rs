use clap::Parser;
use sp1_lido_accounting_dev_scripts::scripts as dev_scripts;
use sp1_lido_accounting_scripts::scripts::{self, prelude::EnvVars};
use sp1_lido_accounting_zk_shared::io::eth_io::ReferenceSlot;

// cargo run --bin submit --release -- --target-slot 5982336 --store --local-verify

#[derive(Parser, Debug)]
#[clap(author, version, about, long_about = None)]
struct ProveArgs {
    #[clap(long, required = false)]
    target_ref_slot: Option<u64>,
    #[clap(long, required = false)]
    previous_ref_slot: Option<u64>,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    sp1_sdk::utils::setup_logger();
    let args = ProveArgs::parse();
    tracing::debug!("Args: {:?}", args);
    let env_vars = EnvVars::init_from_env_or_crash();

    let script_runtime = scripts::prelude::ScriptRuntime::init(&env_vars)
        .expect("Failed to initialize script runtime");

    dev_scripts::store_report::run(
        &script_runtime,
        args.target_ref_slot.map(ReferenceSlot),
        args.previous_ref_slot.map(ReferenceSlot),
    )
    .await?;

    Ok(())
}
