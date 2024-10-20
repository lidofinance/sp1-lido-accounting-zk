use std::collections::HashMap;
use std::{env, path::PathBuf};

use eyre::{eyre, Result, WrapErr};
use sp1_lido_accounting_scripts::beacon_state_reader::file::FileBasedBeaconStateReader;
use sp1_lido_accounting_scripts::beacon_state_reader::{BeaconStateReader, StateId};
use sp1_lido_accounting_scripts::consts::{Network, NetworkInfo, WrappedNetwork};
use sp1_lido_accounting_scripts::eth_client::ContractDeployParametersRust;
use sp1_lido_accounting_scripts::proof_storage::StoredProof;
use sp1_lido_accounting_scripts::{proof_storage, utils};
use sp1_lido_accounting_zk_shared::eth_consensus_layer::{BeaconBlockHeader, BeaconState, Hash256};
use sp1_lido_accounting_zk_shared::eth_spec;
use tree_hash::TreeHash;
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

pub struct TamperableBeaconStateReader<'a, T, Mut>
where
    T: BeaconStateReader,
    Mut: Fn(BeaconState) -> BeaconState,
{
    inner: &'a T,
    beacon_state_mutators: HashMap<StateId, Mut>,
    should_update_block_header: HashMap<StateId, bool>,
}

impl<'a, T, Mut> TamperableBeaconStateReader<'a, T, Mut>
where
    T: BeaconStateReader,
    Mut: Fn(BeaconState) -> BeaconState,
{
    pub fn new(inner: &'a T) -> Self {
        Self {
            inner,
            beacon_state_mutators: HashMap::new(),
            should_update_block_header: HashMap::new(),
        }
    }

    pub fn set_mutator(&mut self, state_id: StateId, update_block_header: bool, mutator: Mut) -> &mut Self {
        self.beacon_state_mutators.insert(state_id.clone(), mutator);
        self.should_update_block_header
            .insert(state_id.clone(), update_block_header);
        self
    }
}

impl<'a, T, Mut> BeaconStateReader for TamperableBeaconStateReader<'a, T, Mut>
where
    T: BeaconStateReader,
    Mut: Fn(BeaconState) -> BeaconState,
{
    async fn read_beacon_state(&self, state_id: &StateId) -> anyhow::Result<BeaconState> {
        let (_, bs) = self.read_beacon_state_and_header(state_id).await?;
        Ok(bs)
    }

    async fn read_beacon_block_header(
        &self,
        state_id: &StateId,
    ) -> anyhow::Result<sp1_lido_accounting_zk_shared::eth_consensus_layer::BeaconBlockHeader> {
        let (bh, _) = self.read_beacon_state_and_header(state_id).await?;
        Ok(bh)
    }

    async fn read_beacon_state_and_header(
        &self,
        state_id: &StateId,
    ) -> anyhow::Result<(BeaconBlockHeader, BeaconState)> {
        let (orig_bh, orig_bs) = self.inner.read_beacon_state_and_header(state_id).await?;

        let (result_bh, result_bs) = match self.beacon_state_mutators.get(state_id) {
            Some(mutator) => {
                let new_bs = (mutator)(orig_bs);
                let new_bh = match self.should_update_block_header.get(state_id) {
                    Some(true) => {
                        let mut new_bh = orig_bh.clone();
                        new_bh.state_root = new_bs.tree_hash_root();
                        new_bh
                    }
                    _ => orig_bh,
                };
                (new_bh, new_bs)
            }
            None => (orig_bh, orig_bs),
        };
        Ok((result_bh, result_bs))
    }
}
