use clap::Parser;
use sp1_lido_accounting_scripts::scripts;
use sp1_lido_accounting_scripts::tracing as tracing_config;
use sp1_lido_accounting_scripts::utils::read_env;
use sp1_lido_accounting_zk_shared::io::eth_io::ReferenceSlot;

// cargo run --bin submit --release -- --target-slot 5982336 --store --local-verify

#[derive(Parser, Debug)]
#[clap(author, version, about, long_about = None)]
struct ProveArgs {
    #[clap(long, required = false)]
    target_ref_slot: Option<u64>,
    #[clap(long, required = false)]
    previous_ref_slot: Option<u64>,
    #[clap(long, required = false, default_value = "false")]
    dry_run: bool,
    #[clap(long, required = false, default_value = "false")]
    local_verify: bool,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // logging setup
    tracing_config::setup_logger(
        tracing_config::LoggingConfig::default()
            .with_thread_names(true)
            .use_format(read_env("LOG_FORMAT", tracing_config::LogFormat::Plain)),
    );

    let args = ProveArgs::parse();
    tracing::debug!("Args: {:?}", args);

    let script_runtime = scripts::prelude::ScriptRuntime::init_from_env().expect("Failed to initialize script runtime");

    let flags = scripts::submit::Flags {
        verify: args.local_verify,
        dry_run: args.dry_run,
    };

    let tx_hash = scripts::submit::run(
        &script_runtime,
        args.target_ref_slot.map(ReferenceSlot),
        args.previous_ref_slot.map(ReferenceSlot),
        &flags,
    )
    .await?;
    tracing::info!("Report transaction complete {}", hex::encode(tx_hash));
    Ok(())
}
