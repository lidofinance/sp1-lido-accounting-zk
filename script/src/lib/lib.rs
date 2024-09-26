use std::fs;
use std::path::Path;

use alloy_sol_types::SolType;
use serde::{Deserialize, Serialize};
use sp1_lido_accounting_zk_shared::io::eth_io::{PublicValuesSolidity, ReportMetadataRust, ReportRust};
use sp1_sdk::HashableKey;
use sp1_sdk::{SP1ProofWithPublicValues, SP1VerifyingKey};

use sp1_lido_accounting_zk_shared::io::serde_utils::serde_hex_as_string;

pub mod beacon_state_reader;
pub mod consts;
pub mod eth_client;
pub mod script_logic;
pub mod validator_delta;

pub const ELF: &[u8] = include_bytes!("../../../program/elf/riscv32im-succinct-zkvm-elf");

pub const CONTRACT_ABI: &str =
    "../../../contracts/out/Sp1LidoAccountingReportContract.sol/Sp1LidoAccountingReportContract.json";

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StoredProof {
    pub vkey: String,
    pub report: ReportRust,
    pub metadata: ReportMetadataRust,
    #[serde(with = "serde_hex_as_string::HexStringProtocol")]
    pub public_values: Vec<u8>,
    #[serde(with = "serde_hex_as_string::HexStringProtocol")]
    pub proof: Vec<u8>,
}

pub fn store_proof_and_metadata(proof: &SP1ProofWithPublicValues, vk: &SP1VerifyingKey, fixture_file: &Path) {
    let bytes = proof.public_values.to_vec();
    let public_values: PublicValuesSolidity = PublicValuesSolidity::abi_decode(bytes.as_slice(), false).unwrap();

    let fixture = StoredProof {
        vkey: vk.bytes32(),
        report: public_values.report.into(),
        metadata: public_values.metadata.into(),
        public_values: bytes,
        proof: proof.bytes(),
    };

    // Save the fixture to a file.
    if let Some(fixture_path) = fixture_file.parent() {
        std::fs::create_dir_all(fixture_path).expect("failed to create fixture path");
    }
    std::fs::write(fixture_file, serde_json::to_string_pretty(&fixture).unwrap()).expect("failed to write fixture");
    log::info!("Successfully written test fixture to {fixture_file:?}");
}

pub fn read_proof_and_metadata(proof_file: &Path) -> Result<StoredProof, serde_json::Error> {
    let file_content = fs::read(proof_file).expect("Failed to read file");
    let proof: StoredProof = serde_json::from_slice(file_content.as_slice())?;
    Ok(proof)
}
