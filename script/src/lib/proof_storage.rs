use std::path::Path;

use alloy_sol_types::SolType;
use serde::{Deserialize, Serialize};
use sp1_lido_accounting_zk_lib::io::eth_io::{PublicValuesSolidity, ReportMetadataRust, ReportRust};
use sp1_sdk::HashableKey;
use sp1_sdk::{SP1ProofWithPublicValues, SP1VerifyingKey};

use sp1_lido_accounting_zk_lib::io::serde_utils::serde_hex_as_string;

use crate::utils;

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

pub fn store_proof_and_metadata(proof: &SP1ProofWithPublicValues, vk: &SP1VerifyingKey, proof_file: &Path) {
    let bytes = proof.public_values.to_vec();
    let public_values: PublicValuesSolidity = PublicValuesSolidity::abi_decode(bytes.as_slice(), false).unwrap();

    let stored_proof = StoredProof {
        vkey: vk.bytes32(),
        report: public_values.report.into(),
        metadata: public_values.metadata.into(),
        public_values: bytes,
        proof: proof.bytes(),
    };

    utils::write_json(proof_file, &stored_proof).expect("failed to write fixture");
    log::info!("Successfully written proof data to {proof_file:?}");
}

pub fn read_proof_and_metadata(proof_file: &Path) -> utils::Result<StoredProof> {
    utils::read_json(proof_file)
}
