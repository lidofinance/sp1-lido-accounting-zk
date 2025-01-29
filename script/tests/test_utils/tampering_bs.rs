use std::{collections::HashMap, sync::Arc};

use sp1_lido_accounting_scripts::beacon_state_reader::{BeaconStateReader, StateId};
use sp1_lido_accounting_zk_shared::eth_consensus_layer::{BeaconBlockHeader, BeaconState};
use tree_hash::TreeHash;

pub struct TamperableBeaconStateReader<T, Mut>
where
    T: BeaconStateReader,
    Mut: Fn(BeaconState) -> BeaconState,
{
    inner: Arc<T>,
    beacon_state_mutators: HashMap<StateId, Mut>,
    should_update_block_header: HashMap<StateId, bool>,
}

impl<T, Mut> TamperableBeaconStateReader<T, Mut>
where
    T: BeaconStateReader,
    Mut: Fn(BeaconState) -> BeaconState,
{
    pub fn new(inner: Arc<T>) -> Self {
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

impl<T, Mut> BeaconStateReader for TamperableBeaconStateReader<T, Mut>
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
