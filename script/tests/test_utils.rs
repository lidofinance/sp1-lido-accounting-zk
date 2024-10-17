use std::{env, path::PathBuf};

use eyre::{eyre, Result, WrapErr};
use sp1_lido_accounting_scripts::beacon_state_reader::file::FileBasedBeaconStateReader;
use sp1_lido_accounting_scripts::beacon_state_reader::{BeaconStateReader, StateId};
use sp1_lido_accounting_scripts::consts::{Network, NetworkInfo, WrappedNetwork};
use sp1_lido_accounting_scripts::eth_client::ContractDeployParametersRust;
use sp1_lido_accounting_scripts::proof_storage::StoredProof;
use sp1_lido_accounting_scripts::{proof_storage, utils};
use sp1_lido_accounting_zk_shared::eth_consensus_layer::BeaconState;
use sp1_lido_accounting_zk_shared::eth_spec;
use typenum::Unsigned;

pub static NETWORK: WrappedNetwork = WrappedNetwork::Anvil(Network::Sepolia);
pub const DEPLOY_SLOT: u64 = 5832096;
pub const DEPLOY_BLOCK: u64 = 6649650;
pub const RETRIES: usize = 3;

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

    pub fn read_deploy(&self, network: &impl NetworkInfo, slot: u64) -> Result<ContractDeployParametersRust> {
        let deploy_args_file = self
            .deploys()
            .join(format!("{}-{}-deploy.json", network.as_str(), slot));
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
}

pub async fn read_latest_bs_at_or_before(
    bs_reader: &impl BeaconStateReader,
    slot: u64,
    retries: usize,
) -> Result<BeaconState> {
    let step = eth_spec::SlotsPerEpoch::to_u64();
    let mut attempt = 0;
    let mut current_slot = slot;
    let result = loop {
        log::debug!("Fetching beacon state: attempt {attempt}, target slot {current_slot}");
        let try_bs = bs_reader.read_beacon_state(&StateId::Slot(current_slot)).await;

        if let Ok(beacon_state) = try_bs {
            break Ok(beacon_state);
        } else if attempt > retries {
            break try_bs;
        } else {
            attempt += 1;
            current_slot -= step;
        }
    };
    result.map_err(|e| eyre!("Failed to read beacon state {:#?}", e))
}
