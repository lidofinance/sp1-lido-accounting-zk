use std::path::PathBuf;

use clap::Parser;

use sp1_lido_accounting_dev_scripts::scripts as dev_scripts;
use sp1_lido_accounting_scripts::beacon_state_reader::{BeaconStateReader, StateId};
use sp1_lido_accounting_scripts::consts::NetworkInfo;
use sp1_lido_accounting_scripts::scripts;
use sp1_lido_accounting_zk_shared::io::eth_io::ReferenceSlot;

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
    tracing::debug!("Args: {:?}", args);

    let script_runtime = scripts::prelude::ScriptRuntime::init_from_env()
        .expect("Failed to initialize script runtime");

    let refslot = match args.target_ref_slot {
        Some(refslot) => ReferenceSlot(refslot),
        None => {
            let bh = script_runtime
                .bs_reader()
                .read_beacon_block_header(&StateId::Finalized)
                .await
                .expect("Couldn't automatically determine target ref slot");
            ReferenceSlot(bh.slot)
        }
    };
    let previous_slot = match args.previous_ref_slot {
        Some(refslot) => ReferenceSlot(refslot),
        None => {
            let last_state_slot = script_runtime
                .report_contract
                .get_latest_validator_state_slot()
                .await
                .expect("Couldn't automatically determine previuous ref slot");
            ReferenceSlot(last_state_slot.0)
        }
    };

    tracing::info!(
        "Running for network {:?}, slot: {}, previous_slot: {}",
        script_runtime.network().as_str(),
        refslot,
        previous_slot
    );

    let project_root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));

    let fixture_files = vec![
        project_root.join("../contracts/test/fixtures/fixture.json"),
        project_root.join("../script/tests/data/proofs/fixture.json"),
    ];

    let withdrawal_vault_data_filename = format!("vault_data_{refslot}.json");

    let withdrawal_vault_fixture_files = vec![project_root
        .join("../script/tests/data/withdrawal_vault_account_proofs/")
        .join(withdrawal_vault_data_filename)];

    dev_scripts::write_test_fixture::run(
        &script_runtime,
        refslot,
        previous_slot,
        fixture_files,
        withdrawal_vault_fixture_files,
    )
    .await
    .expect("Failed to run `write_test_fixture");
}
