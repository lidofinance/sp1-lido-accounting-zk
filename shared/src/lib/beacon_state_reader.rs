use anyhow;
use log;
use serde::de::DeserializeOwned;
use ssz::Decode;
use std::fs::File;
use std::io::{self, BufReader, Read};
use std::path::{Path, PathBuf};
use std::{env, fs};

use crate::eth_consensus_layer::{BeaconBlockHeader, BeaconState};

pub trait BeaconStateReader {
    #[allow(async_fn_in_trait)]
    async fn read_beacon_state(&self, slot: u64) -> anyhow::Result<BeaconState>;
    #[allow(async_fn_in_trait)]
    async fn read_beacon_block_header(&self, _slot: u64) -> anyhow::Result<BeaconBlockHeader>;
}

pub struct FileBasedBeaconChainStore {
    pub store_location: PathBuf,
}

impl FileBasedBeaconChainStore {
    pub fn new(store_location: &Path) -> Self {
        let abs_path = Self::abs_path(PathBuf::from(store_location)).expect(&format!(
            "Failed to convert {} into absolute path",
            store_location.display()
        ));
        Self {
            store_location: abs_path,
        }
    }

    fn abs_path(path: PathBuf) -> io::Result<PathBuf> {
        if path.is_absolute() {
            Ok(path)
        } else {
            Ok(env::current_dir()?.join(path))
        }
    }

    pub fn get_beacon_state_path(&self, slot: u64) -> PathBuf {
        self.store_location.join(format!("bs_{}.ssz", slot))
    }

    pub fn get_beacon_block_header_path(&self, slot: u64) -> PathBuf {
        self.store_location.join(format!("bs_{}_header.json", slot))
    }

    pub fn exists(path: &Path) -> bool {
        let result = Path::exists(&path);
        if result {
            log::debug!("Path exists {:?}", path);
        } else {
            log::debug!("Path does not exist ({:?})", path);
        }
        return result;
    }

    pub fn delete(path: &Path) -> io::Result<()> {
        fs::remove_file(path)?;
        Ok(())
    }
}

pub struct FileBasedBeaconStateReader {
    file_store: FileBasedBeaconChainStore,
}

pub fn read_binary<P: AsRef<Path>>(path: P) -> io::Result<Vec<u8>> {
    let mut file = File::open(path)?;
    let mut buffer = Vec::new();
    file.read_to_end(&mut buffer)?;
    Ok(buffer)
}

// TODO: better mapping serde_json::Err to std::error::Error/anyhow::Error
fn map_err_to_anyhow<T>(value: serde_json::Result<T>) -> anyhow::Result<T> {
    value.map_err(|err| anyhow::anyhow!("Couldn't parse json {:#?}", err))
}

pub async fn read_untyped_json<P: AsRef<Path>>(path: P) -> anyhow::Result<serde_json::Value> {
    let file = File::open(path)?;
    map_err_to_anyhow(serde_json::from_reader(BufReader::new(file)))
}

pub async fn read_json<P: AsRef<Path>, T: DeserializeOwned>(path: P) -> anyhow::Result<T> {
    let file = File::open(path)?;
    map_err_to_anyhow(serde_json::from_reader(BufReader::new(file)))
}

impl FileBasedBeaconStateReader {
    pub fn new(store_location: &Path) -> Self {
        Self {
            file_store: FileBasedBeaconChainStore::new(store_location),
        }
    }
}

impl BeaconStateReader for FileBasedBeaconStateReader {
    async fn read_beacon_state(&self, slot: u64) -> anyhow::Result<BeaconState> {
        let beacon_state_path = self.file_store.get_beacon_state_path(slot);
        log::info!("Reading BeaconState from file {:?}", beacon_state_path);
        let data = read_binary(beacon_state_path)?;
        // TODO: better mapping ssz::DecodeError to std::error::Error/anyhow::Error
        BeaconState::from_ssz_bytes(&data)
            .map_err(|decode_err| anyhow::anyhow!("Couldn't decode ssz {:#?}", decode_err))
    }

    async fn read_beacon_block_header(&self, slot: u64) -> anyhow::Result<BeaconBlockHeader> {
        let beacon_block_header_path = self.file_store.get_beacon_block_header_path(slot);
        log::info!("Reading BeaconBlock from file {:?}", &beacon_block_header_path);
        let res: BeaconBlockHeader = read_json(&beacon_block_header_path).await?;
        Ok(res)
    }
}
