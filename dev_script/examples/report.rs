use hex::FromHex;
use serde_json::Value;
use sp1_lido_accounting_scripts::beacon_state_reader::StateId;
use sp1_lido_accounting_scripts::consts;

use sp1_lido_accounting_dev_scripts::synthetic::{
    BalanceGenerationMode, GenerationSpec, SyntheticBeaconStateCreator,
};

use sp1_lido_accounting_scripts::beacon_state_reader::{
    file::FileBasedBeaconStateReader, BeaconStateReader,
};
use sp1_lido_accounting_zk_shared::io::eth_io::{BeaconChainSlot, HaveEpoch, ReferenceSlot};
use std::path::PathBuf;

use sp1_lido_accounting_zk_shared::circuit_logic::report::ReportData;
use sp1_lido_accounting_zk_shared::eth_consensus_layer::Hash256;

use simple_logger::SimpleLogger;

fn hex_str_to_h256(hex_str: &str) -> Hash256 {
    <[u8; 32]>::from_hex(hex_str)
        .expect("Couldn't parse hex_str as H256")
        .into()
}

fn verify_report(report: &ReportData, manifesto: &Value) {
    assert_eq!(report.slot.0, manifesto["report"]["slot"].as_u64().unwrap());
    assert_eq!(report.epoch, manifesto["report"]["epoch"].as_u64().unwrap());
    assert_eq!(
        report.lido_withdrawal_credentials,
        hex_str_to_h256(
            manifesto["report"]["lido_withdrawal_credentials"]
                .as_str()
                .unwrap()
        )
    );
    assert_eq!(
        report.deposited_lido_validators,
        manifesto["report"]["lido_deposited_validators"]
            .as_u64()
            .unwrap()
    );
    assert_eq!(
        report.exited_lido_validators,
        manifesto["report"]["lido_exited_validators"]
            .as_u64()
            .unwrap()
    );
    assert_eq!(
        report.lido_cl_balance,
        manifesto["report"]["lido_cl_balance"].as_u64().unwrap()
    );
}

#[tokio::main]
async fn main() {
    SimpleLogger::new().env().init().unwrap();
    let ssz_folder = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../temp");
    let withdrawal_creds: Hash256 = consts::lido_credentials::MAINNET.into();
    let creator = SyntheticBeaconStateCreator::new(&ssz_folder, false, true)
        .expect("Failed to create synthetic beacon state creator");
    let reader: FileBasedBeaconStateReader =
        FileBasedBeaconStateReader::new(&ssz_folder).expect("Failed to create beacon state reader");

    let old_slot = BeaconChainSlot(9760032);
    let new_slot = old_slot + 216000; // (30 * 24 * 60 * 60 / 12) slots per month

    let old_beacon_state = reader
        .read_beacon_state(&StateId::Slot(old_slot))
        .await
        .expect("Failed to read beacon state");
    tracing::info!(
        "Read Old Beacon State for slot {:?}, with {} validators",
        old_beacon_state.slot,
        old_beacon_state.validators.to_vec().len(),
    );
    let old_report = ReportData::compute(
        ReferenceSlot(old_slot.0),
        old_beacon_state.epoch(),
        &old_beacon_state.validators,
        &old_beacon_state.balances,
        &withdrawal_creds,
    );

    // Step 1.5. generate a "new" beacon state with controlled parameters

    // 2020-12-01 => 2023-12-01: +880K => ~25K/mo
    let new_non_lido_validators_a_month = 15_000;
    // At the time of writing, lido operated ~400K validators out of 1M => 10K/mo
    let new_lido_validators_a_month = 10_000;
    let new_exited_lido_validators = 500;
    let created_but_not_deposited = 300;

    let generation_spec = GenerationSpec {
        slot: new_slot.0,
        non_lido_validators: new_non_lido_validators_a_month,
        deposited_lido_validators: new_lido_validators_a_month
            - created_but_not_deposited
            - new_exited_lido_validators,
        exited_lido_validators: new_exited_lido_validators,
        pending_deposit_lido_validators: created_but_not_deposited,
        balances_generation_mode: BalanceGenerationMode::FIXED,
        shuffle: false,
        base_slot: Some(old_slot.0),
        overwrite: false,
    };

    creator
        .create_beacon_state(generation_spec)
        .await
        .expect("Failed to create new BeaconState");

    let new_beacon_state = reader
        .read_beacon_state(&StateId::Slot(new_slot))
        .await
        .expect("Failed to read beacon state");
    tracing::info!(
        "Read New Beacon State for slot {:?}, with {} validators",
        new_beacon_state.slot,
        new_beacon_state.validators.to_vec().len(),
    );

    // Step 2: read manifesto
    let manifesto = creator
        .read_manifesto(new_slot.0)
        .await
        .expect("Failed to read manifesto json");

    // Step 3: Compute report
    let new_report = ReportData::compute(
        ReferenceSlot(new_beacon_state.slot),
        new_beacon_state.epoch(),
        &new_beacon_state.validators,
        &new_beacon_state.balances,
        &withdrawal_creds,
    );

    // Step 4: verify report matches
    verify_report(&new_report, &manifesto);
    tracing::info!(
        "Old Report: {:>16} balance, {:>8} deposited validators, {:>8} exited validators",
        old_report.lido_cl_balance,
        old_report.deposited_lido_validators,
        old_report.exited_lido_validators
    );
    tracing::info!(
        "New Report: {:>16} balance, {:>8} deposited validators, {:>8} exited validators",
        new_report.lido_cl_balance,
        new_report.deposited_lido_validators,
        new_report.exited_lido_validators
    );
    tracing::info!(
        "Manifesto: {:>16} balance, {:>8} all validators, {:>8} exited validators",
        new_report.lido_cl_balance,
        new_report.deposited_lido_validators,
        new_report.exited_lido_validators
    );
}
