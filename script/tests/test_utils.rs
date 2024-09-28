use std::{env, path::PathBuf};

use eyre::{Result, WrapErr};
use sp1_lido_accounting_scripts::consts::{Network, NetworkInfo, WrappedNetwork};
use sp1_lido_accounting_scripts::eth_client::ContractDeployParametersRust;
use sp1_lido_accounting_scripts::proof_storage::StoredProof;
use sp1_lido_accounting_scripts::{proof_storage, utils};

pub static NETWORK: WrappedNetwork = WrappedNetwork::Anvil(Network::Sepolia);
pub const DEPLOY_SLOT: u64 = 5832096;
pub const DEPLOY_BLOCK: u64 = 6649650;

pub struct TestFiles {
    pub base: PathBuf,
}

impl TestFiles {
    pub fn new(base: PathBuf) -> Self {
        Self { base }
    }
    pub fn new_from_manifest_dir() -> Self {
        let base = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/data");
        Self::new(base)
    }

    pub fn deploy(&self) -> PathBuf {
        self.base.join("deploy")
    }

    pub fn proofs(&self) -> PathBuf {
        self.base.join("proofs")
    }

    pub fn read_deploy(&self, network: &impl NetworkInfo, slot: u64) -> Result<ContractDeployParametersRust> {
        let deploy_args_file = self.deploy().join(format!("{}-{}-deploy.json", network.as_str(), slot));
        utils::read_json(deploy_args_file.as_path())
            .wrap_err(format!("Failed to read deploy args from file {:#?}", deploy_args_file))
    }

    pub fn read_proof(&self, file_name: &str) -> Result<StoredProof> {
        let proof_file = self.proofs().join(file_name);
        proof_storage::read_proof_and_metadata(proof_file.as_path())
            .wrap_err(format!("Failed to read proof from file {:#?}", proof_file))
    }
}
