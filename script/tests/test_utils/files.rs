use std::{env, path::PathBuf};

use eyre::{eyre, Result, WrapErr};
use sp1_lido_accounting_scripts::beacon_state_reader::file::FileBasedBeaconStateReader;
use sp1_lido_accounting_scripts::beacon_state_reader::{BeaconStateReader, StateId};
use sp1_lido_accounting_scripts::consts::NetworkInfo;
use sp1_lido_accounting_scripts::eth_client::ContractDeployParametersRust;
use sp1_lido_accounting_scripts::proof_storage::StoredProof;
use sp1_lido_accounting_scripts::utils::read_json;
use sp1_lido_accounting_scripts::{proof_storage, utils};
use sp1_lido_accounting_zk_shared::eth_consensus_layer::{BeaconBlockHeader, BeaconState};
use sp1_lido_accounting_zk_shared::io::eth_io::BeaconChainSlot;
use sp1_lido_accounting_zk_shared::io::program_io::WithdrawalVaultData;

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

    fn deploys(&self) -> PathBuf {
        self.base.join("deploy")
    }

    fn proofs(&self) -> PathBuf {
        self.base.join("proofs")
    }

    fn beacon_states(&self) -> PathBuf {
        self.base.join("beacon_states")
    }

    fn withdrawal_vault_account_proofs(&self) -> PathBuf {
        self.base.join("withdrawal_vault_account_proofs")
    }

    pub fn read_deploy(
        &self,
        network: &impl NetworkInfo,
        slot: BeaconChainSlot,
    ) -> Result<ContractDeployParametersRust> {
        let deploy_args_file = self
            .deploys()
            .join(format!("{}-{}-deploy.json", network.as_str(), slot.0));
        utils::read_json(deploy_args_file.as_path())
            .wrap_err(format!("Failed to read deploy args from file {:#?}", deploy_args_file))
    }

    pub fn read_proof(&self, file_name: &str) -> Result<StoredProof> {
        let proof_file = self.proofs().join(file_name);
        proof_storage::read_proof_and_metadata(proof_file.as_path())
            .wrap_err(format!("Failed to read proof from file {:#?}", proof_file))
    }

    pub async fn read_beacon_state(&self, state_id: &StateId) -> Result<BeaconState> {
        let file_reader = FileBasedBeaconStateReader::new(&self.beacon_states());
        file_reader
            .read_beacon_state(state_id)
            .await
            .map_err(|err| eyre!("Failed to read beacon state {:#?}", err))
    }

    pub async fn read_beacon_block_header(&self, state_id: &StateId) -> Result<BeaconBlockHeader> {
        let file_reader = FileBasedBeaconStateReader::new(&self.beacon_states());
        file_reader
            .read_beacon_block_header(state_id)
            .await
            .map_err(|err| eyre!("Failed to read beacon block header {:#?}", err))
    }

    pub async fn read_withdrawal_vault_data(&self, state_id: &StateId) -> Result<WithdrawalVaultData> {
        let folder = self.withdrawal_vault_account_proofs();
        let permanent_state_id = state_id
            .get_permanent_str()
            .map_err(|err| eyre!("Failed to get permanent str for StateId {:#?}", err))?;
        let file_path = folder.join(format!("vault_data_{}.json", permanent_state_id));
        log::info!("Reading WithdrawalVault account proof from file {:?}", &file_path);
        let res: WithdrawalVaultData = read_json(&file_path)?;
        Ok(res)
    }
}
