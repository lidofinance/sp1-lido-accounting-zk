use std::path::PathBuf;

use clap::Parser;

use sp1_lido_accounting_scripts::beacon_state_reader::{BeaconStateReader, StateId};
use sp1_lido_accounting_scripts::consts::NetworkInfo;
use sp1_lido_accounting_scripts::scripts;
use sp1_lido_accounting_zk_lib::io::eth_io::ReferenceSlot;

#[derive(Parser, Debug)]
#[clap(author, version, about, long_about = None)]
struct ProveArgs {
    #[clap(long)]
    target_ref_slot: Option<u64>,
    #[clap(long)]
    previous_ref_slot: Option<u64>,
}

#[tokio::main]
async fn main() {
    sp1_sdk::utils::setup_logger();
    let args = ProveArgs::parse();
    log::debug!("Args: {:?}", args);

    let (network, client, bs_reader) = scripts::prelude::initialize();
    let (eth_client, contract) = scripts::prelude::initialize_eth();

    let refslot = match args.target_ref_slot {
        Some(refslot) => ReferenceSlot(refslot),
        None => {
            let bh = bs_reader
                .read_beacon_block_header(&StateId::Finalized)
                .await
                .expect("Couldn't automatically determine target ref slot");
            ReferenceSlot(bh.slot)
        }
    };
    let previous_slot = match args.previous_ref_slot {
        Some(refslot) => ReferenceSlot(refslot),
        None => {
            let last_state_slot = contract
                .get_latest_validator_state_slot()
                .await
                .expect("Couldn't automatically determine previuous ref slot");
            ReferenceSlot(last_state_slot.0)
        }
    };

    log::info!(
        "Running for network {:?}, slot: {}, previous_slot: {}",
        network,
        refslot,
        previous_slot
    );

    let project_root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));

    let fixture_files = vec![
        project_root.join("../contracts/test/fixtures/fixture.json"),
        project_root.join("../script/tests/data/proofs/fixture.json"),
    ];

    scripts::write_test_fixture::run(
        &client,
        &bs_reader,
        &eth_client,
        refslot,
        previous_slot,
        &network.get_config(),
        fixture_files,
    )
    .await
    .expect("Failed to run `write_test_fixture");
}
