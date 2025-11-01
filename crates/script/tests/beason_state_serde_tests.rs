mod test_utils;

use eyre::{eyre, Result};
use sp1_lido_accounting_zk_shared::eth_consensus_layer::BeaconState;
use ssz::Decode;
use test_utils::files::TestFiles;

const ELECTRA_BS_SLOT: u64 = 1621090;
const FULU_BS_SLOT: u64 = 1646720;

fn bs_variant_name(value: BeaconState) -> &'static str {
    match value {
        BeaconState::Electra(_) => "Electra",
        BeaconState::Fulu(_) => "Fulu",
    }
}

#[tokio::test]
async fn test_deser_electra() -> Result<()> {
    let test_files = TestFiles::new_from_manifest_dir();
    let raw_ssz = test_files.read_bs_ssz(ELECTRA_BS_SLOT).await?;
    let beacon_state =
        BeaconState::from_ssz_bytes(&raw_ssz).map_err(|err| eyre!("Failed to decode Electra BeaconState {err:#?}"))?;

    match beacon_state {
        BeaconState::Electra(inner) => assert_eq!(inner.slot, ELECTRA_BS_SLOT),
        _ => panic!(
            "Expected Electra beacon state, got {:?} variant",
            bs_variant_name(beacon_state)
        ),
    }
    Ok(())
}

#[tokio::test]
async fn test_deser_fulu() -> Result<()> {
    let test_files = TestFiles::new_from_manifest_dir();
    let raw_ssz = test_files.read_bs_ssz(FULU_BS_SLOT).await?;
    let beacon_state =
        BeaconState::from_ssz_bytes(&raw_ssz).map_err(|err| eyre!("Failed to decode Electra BeaconState {err:#?}"))?;

    match beacon_state {
        BeaconState::Fulu(inner) => assert_eq!(inner.slot, FULU_BS_SLOT),
        _ => panic!(
            "Expected Fulu beacon state, got {:?} variant",
            bs_variant_name(beacon_state)
        ),
    }
    Ok(())
}
