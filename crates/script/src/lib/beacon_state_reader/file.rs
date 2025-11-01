use crate::prometheus_metrics;
use crate::utils::{read_binary, read_json};
use ssz::{Decode, Encode};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::{env, fs};
use std::{io, vec};

use sp1_lido_accounting_zk_shared::eth_consensus_layer::{BeaconBlockHeader, BeaconState};

use super::{BeaconStateReader, InitializationError, StateId};

pub struct FileBasedBeaconChainStore {
    pub store_location: PathBuf,
}

impl FileBasedBeaconChainStore {
    pub fn new(store_location: &Path) -> Result<Self, InitializationError> {
        let store_location = Self::abs_path(store_location.to_path_buf())?;
        Ok(Self { store_location })
    }

    fn abs_path(path: PathBuf) -> io::Result<PathBuf> {
        if path.is_absolute() {
            Ok(path)
        } else {
            Ok(env::current_dir()?.join(path))
        }
    }

    pub fn get_beacon_state_path(&self, state_id: &str) -> PathBuf {
        self.store_location.join(format!("bs_{}.ssz", state_id))
    }

    pub fn get_beacon_block_header_path(&self, state_id: &str) -> PathBuf {
        self.store_location.join(format!("bs_{}_header.json", state_id))
    }

    pub fn exists(path: &Path) -> bool {
        let result = Path::exists(path);
        if result {
            tracing::debug!("Path exists {:?}", path);
        } else {
            tracing::debug!("Path does not exist ({:?})", path);
        }
        result
    }

    pub fn ensure_exists(&self) -> io::Result<()> {
        std::fs::create_dir_all(self.store_location.clone())
    }

    pub fn delete(path: &Path) -> io::Result<()> {
        fs::remove_file(path)?;
        Ok(())
    }
}

pub struct FileBasedBeaconStateReader {
    fallthrough_file_stores: Vec<FileBasedBeaconChainStore>,
    main_file_store: FileBasedBeaconChainStore,
    metrics_reporter: Arc<prometheus_metrics::Service>,
}

impl FileBasedBeaconStateReader {
    pub fn new(
        store_location: &Path,
        metrics_reporter: Arc<prometheus_metrics::Service>,
    ) -> Result<Self, InitializationError> {
        Ok(Self {
            main_file_store: FileBasedBeaconChainStore::new(store_location)?,
            fallthrough_file_stores: vec![],
            metrics_reporter,
        })
    }

    pub fn new_with_fallthrough(
        store_location: &Path,
        fallthrough_locations: &[&Path],
        metrics_reporter: Arc<prometheus_metrics::Service>,
    ) -> Result<Self, InitializationError> {
        let fallthrough_file_stores: Vec<FileBasedBeaconChainStore> = fallthrough_locations
            .iter()
            .map(|&path| FileBasedBeaconChainStore::new(path))
            .collect::<Result<Vec<_>, _>>()?;
        Ok(Self {
            main_file_store: FileBasedBeaconChainStore::new(store_location)?,
            fallthrough_file_stores,
            metrics_reporter,
        })
    }
}

impl FileBasedBeaconStateReader {
    async fn read_beacon_state_impl_single(
        &self,
        store: &FileBasedBeaconChainStore,
        state_id: &StateId,
    ) -> anyhow::Result<BeaconState> {
        let permanent_state = state_id.get_permanent_str()?;
        let beacon_state_path = store.get_beacon_state_path(&permanent_state);
        tracing::info!(
            state_id = ?state_id,
            "Reading BeaconState {state_id:?} from file {beacon_state_path:?}",
        );
        let data = read_binary(beacon_state_path)?;
        BeaconState::from_ssz_bytes(&data)
            .map_err(|decode_err| anyhow::anyhow!("Couldn't decode BeaconState ssz for {state_id:?} {decode_err:#?}"))
            .inspect(
                |bs| tracing::debug!(state_id=?state_id, slot=bs.slot(), "Read BeaconState {} for {state_id:?}", bs.slot()),
            )
            .inspect_err(|e| tracing::debug!(state_id=?state_id, "{e:?}"))
    }

    async fn read_beacon_state_impl(&self, state_id: &StateId) -> anyhow::Result<BeaconState> {
        for store in &self.fallthrough_file_stores {
            match self.read_beacon_state_impl_single(store, state_id).await {
                Ok(beacon_state) => return Ok(beacon_state),
                Err(e) => {
                    tracing::debug!(state_id=?state_id, "Failed to read BeaconState for {state_id:?} from store: {e:#?}");
                }
            }
        }
        self.read_beacon_state_impl_single(&self.main_file_store, state_id)
            .await
    }

    async fn read_beacon_block_header_impl_single(
        &self,
        store: &FileBasedBeaconChainStore,
        state_id: &StateId,
    ) -> anyhow::Result<BeaconBlockHeader> {
        let permanent_state = state_id.get_permanent_str()?;
        let beacon_block_header_path = store.get_beacon_block_header_path(&permanent_state);
        tracing::info!(
            state_id = ?state_id,
            "Reading BeaconBlockHeader for {state_id:?} from file {beacon_block_header_path:?}",
        );
        let res: BeaconBlockHeader = read_json(&beacon_block_header_path)
            .inspect(|bh: &BeaconBlockHeader| tracing::debug!(state_id = ?state_id, slot=bh.slot, "Read BeaconBlockHeader {} for {state_id:?}", bh.slot))
            .inspect_err(|e| tracing::debug!(state_id = ?state_id, "Failed to read BeaconBlockHeader for {state_id:?}: {e:#?}"))?;
        Ok(res)
    }

    async fn read_beacon_block_header_impl(&self, state_id: &StateId) -> anyhow::Result<BeaconBlockHeader> {
        for store in &self.fallthrough_file_stores {
            match self.read_beacon_block_header_impl_single(store, state_id).await {
                Ok(block_header) => return Ok(block_header),
                Err(e) => {
                    tracing::debug!(state_id=?state_id, "Failed to read BeaconBlockHeader for {state_id:?} from store: {e:#?}");
                }
            }
        }
        self.read_beacon_block_header_impl_single(&self.main_file_store, state_id)
            .await
    }
}

impl BeaconStateReader for FileBasedBeaconStateReader {
    async fn read_beacon_state(&self, state_id: &StateId) -> anyhow::Result<BeaconState> {
        self.metrics_reporter
            .run_with_metrics_and_logs_async(
                prometheus_metrics::services::beacon_state_reader::READ_BEACON_STATE,
                || self.read_beacon_state_impl(state_id),
            )
            .await
    }

    async fn read_beacon_block_header(&self, state_id: &StateId) -> anyhow::Result<BeaconBlockHeader> {
        self.metrics_reporter
            .run_with_metrics_and_logs_async(
                prometheus_metrics::services::beacon_state_reader::READ_BEACON_BLOCK_HEADER,
                || self.read_beacon_block_header_impl(state_id),
            )
            .await
    }
}

pub struct FileBeaconStateWriter {
    file_store: FileBasedBeaconChainStore,
    metrics_reporter: Arc<prometheus_metrics::Service>,
}

impl FileBeaconStateWriter {
    pub fn new(
        store_location: &Path,
        metrics_reporter: Arc<prometheus_metrics::Service>,
    ) -> Result<Self, InitializationError> {
        Ok(Self {
            file_store: FileBasedBeaconChainStore::new(store_location)?,
            metrics_reporter,
        })
    }

    fn ensure_folder_exists(&self) -> anyhow::Result<()> {
        self.file_store.ensure_exists().map_err(|io_err| {
            let msg = format!("Couldn't create folders {io_err:#?}");
            tracing::debug!(msg);
            anyhow::anyhow!(msg)
        })
    }

    fn write_beacon_state_impl(&self, bs: &BeaconState) -> anyhow::Result<()> {
        self.ensure_folder_exists()?;
        let slot = *bs.slot();
        let file_path = self.file_store.get_beacon_state_path(&slot.to_string());
        tracing::info!(slot = slot, "Writing BeaconState {} to {:?}", bs.slot(), file_path);

        fs::write(file_path, bs.as_ssz_bytes())
            .map_err(|write_err| anyhow::anyhow!("Couldn't write BeaconState {}, {write_err:#?}", slot))
            .inspect(|_val| tracing::debug!(slot = bs.slot(), "Wrote BeaconState {}", slot))
            .inspect_err(|e| tracing::debug!(slot = bs.slot(), "{e:?}"))
    }

    pub fn write_beacon_state(&self, bs: &BeaconState) -> anyhow::Result<()> {
        self.metrics_reporter.run_with_metrics_and_logs(
            prometheus_metrics::services::beacon_state_reader::WEITE_BEACON_STATE,
            || self.write_beacon_state_impl(bs),
        )
    }

    fn write_beacon_block_header_impl(&self, bh: &BeaconBlockHeader) -> anyhow::Result<()> {
        self.ensure_folder_exists()?;
        let file_path = self.file_store.get_beacon_block_header_path(&bh.slot.to_string());
        tracing::info!(
            slot = bh.slot,
            "Writing BeaconBlockHeader {} to {:?}",
            bh.slot,
            file_path
        );

        let mut serialized: Vec<u8> = Vec::new();

        serde_json::to_writer(&mut serialized, &bh)
            .map_err(|serde_err| anyhow::anyhow!("Couldn't encode BeaconBlockHeader as json: {serde_err:#?}"))
            .inspect(|_val| tracing::debug!(slot = bh.slot, "Serialized BeaconBlockHeader to json"))
            .inspect_err(|e| tracing::debug!(slot = bh.slot, "{e:?}"))?;

        fs::write(file_path, serialized)
            .map_err(|write_err| anyhow::anyhow!("Couldn't write BeaconBlockHeader {} {write_err:#?}", bh.slot))
            .inspect(|_val| tracing::debug!(slot = bh.slot, "Wrote BeaconBlockHeader {}", bh.slot))
            .inspect_err(|e| tracing::debug!(slot = bh.slot, "{e:?}"))
    }

    pub fn write_beacon_block_header(&self, bh: &BeaconBlockHeader) -> anyhow::Result<()> {
        self.metrics_reporter.run_with_metrics_and_logs(
            prometheus_metrics::services::beacon_state_reader::WRITE_BEACON_BLOCK_HEADER,
            || self.write_beacon_block_header_impl(bh),
        )
    }
}
