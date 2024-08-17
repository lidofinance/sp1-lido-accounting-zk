use hex::FromHex;
use log;
use serde_json::Value;

use ssz_types::typenum::Unsigned;
use std::path::PathBuf;
use util::synthetic_beacon_state_reader::SyntheticBeaconStateCreator;

mod util;
use crate::util::synthetic_beacon_state_reader::{BalanceGenerationMode, SyntheticBeaconStateCreator};
use sp1_lido_accounting_zk_shared::beacon_state_reader::BeaconStateReader;
use sp1_lido_accounting_zk_shared::eth_consensus_layer::{Hash256, SlotsPerEpoch};
use sp1_lido_accounting_zk_shared::report::ReportData;

use simple_logger::SimpleLogger;

fn hex_str_to_h256(hex_str: &str) -> Hash256 {
    <[u8; 32]>::from_hex(hex_str)
        .expect("Couldn't parse hex_str as H256")
        .into()
}

fn verify_report(report: &ReportData, manifesto: &Value) {
    assert_eq!(report.slot, manifesto["report"]["slot"].as_u64().unwrap());
    assert_eq!(report.epoch, manifesto["report"]["epoch"].as_u64().unwrap());
    assert_eq!(
        report.lido_withdrawal_credentials,
        hex_str_to_h256(manifesto["report"]["lido_withdrawal_credentials"].as_str().unwrap())
    );
    assert_eq!(
        report.deposited_lido_validators,
        manifesto["report"]["lido_validators"].as_u64().unwrap()
    );
    assert_eq!(
        report.exited_lido_validators,
        manifesto["report"]["lido_exited_validators"].as_u64().unwrap()
    );
    assert_eq!(
        report.lido_cl_balance,
        manifesto["report"]["lido_cl_balance"].as_u64().unwrap()
    );
}

#[tokio::main]
async fn main() {
    SimpleLogger::new().env().init().unwrap();
    // Step 1. obtain SSZ-serialized beacon state
    // For now using a "synthetic" generator based on reference implementation (py-ssz)
    let creator = SyntheticBeaconStateCreator::new(
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../temp"),
        2_u64.pow(12),
        2_u64.pow(6),
        BalanceGenerationMode::SEQUENTIAL,
        true,
        true,
        false,
    );

    let slot = 1000000;
    creator.create_beacon_state(slot, true);
    let reader = creator.get_file_reader(slot);

    let beacon_state = reader
        .read_beacon_state(slot)
        .await
        .expect("Failed to read beacon state");
    log::info!(
        "Read Beacon State for slot {:?}, with {} validators",
        beacon_state.slot,
        beacon_state.validators.to_vec().len(),
    );

    // Step 2: read manifesto
    let manifesto = reader
        .read_manifesto(slot)
        .await
        .expect("Failed to read manifesto json");
    let lido_withdrawal_creds = hex_str_to_h256(manifesto["report"]["lido_withdrawal_credentials"].as_str().unwrap());

    // Step 3: Compute report
    let epoch = beacon_state.slot / SlotsPerEpoch::to_u64();
    let report = ReportData::compute(
        beacon_state.slot,
        epoch,
        &beacon_state.validators,
        &beacon_state.balances,
        &lido_withdrawal_creds,
    );

    // Step 4: verify report matches
    verify_report(&report, &manifesto);
    log::info!(
        "Report   : {:>16} balance, {:>8} all validators, {:>8} exited validators",
        report.lido_cl_balance,
        report.deposited_lido_validators,
        report.exited_lido_validators
    );
    log::info!(
        "Manifesto: {:>16} balance, {:>8} all validators, {:>8} exited validators",
        report.lido_cl_balance,
        report.deposited_lido_validators,
        report.exited_lido_validators
    );
}
