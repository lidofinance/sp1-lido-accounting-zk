use std::path::PathBuf;

use clap::Parser;

use sp1_lido_accounting_scripts::consts::NetworkInfo;
use sp1_lido_accounting_scripts::scripts;
use sp1_lido_accounting_zk_shared::io::eth_io::ReferenceSlot;

#[derive(Parser, Debug)]
#[clap(author, version, about, long_about = None)]
struct ProveArgs {
    #[clap(long, default_value = "5800000")]
    target_ref_slot: u64,
    #[clap(long, default_value = "5000000")]
    previous_ref_slot: u64,
}

#[tokio::main]
async fn main() {
    sp1_sdk::utils::setup_logger();
    let args = ProveArgs::parse();
    log::debug!("Args: {:?}", args);

    let (network, client, bs_reader) = scripts::prelude::initialize();
    log::info!(
        "Running for network {:?}, slot: {}, previous_slot: {}",
        network,
        args.target_ref_slot,
        args.previous_ref_slot
    );

    let project_root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));

    let fixture_files = vec![
        project_root.join("../contracts/test/fixtures/fixture.json"),
        project_root.join("../script/tests/data/proofs/fixture.json"),
    ];

    scripts::write_test_fixture::run(
        client,
        bs_reader,
        ReferenceSlot(args.target_ref_slot),
        ReferenceSlot(args.previous_ref_slot),
        &network.get_config().lido_withdrawal_credentials,
        fixture_files,
    )
    .await
    .expect("Failed to run `write_test_fixture");
}
