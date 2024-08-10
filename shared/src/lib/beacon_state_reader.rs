use anyhow::{anyhow, Result};
use log;
use ssz::Decode;
use std::env;
use std::fs::File;
use std::io::{self, Read};
use std::path::{Path, PathBuf};

use crate::eth_consensus_layer::{BeaconBlockHeader, BeaconState};

pub trait BeaconStateReader {
    async fn read_beacon_state(&self, slot: u64) -> Result<BeaconState>;
    async fn read_beacon_block_header(&self, _slot: u64) -> Result<BeaconBlockHeader>;
}

pub struct FileBasedBeaconStateReader {
    beacon_state_path: PathBuf,
    beacon_block_header_path: PathBuf,
}

pub fn read_binary_file<P: AsRef<Path>>(path: P) -> io::Result<Vec<u8>> {
    let mut file = File::open(path)?;
    let mut buffer = Vec::new();
    file.read_to_end(&mut buffer)?;
    Ok(buffer)
}

impl FileBasedBeaconStateReader {
    pub fn new(beacon_state_path: PathBuf, beacon_block_header_path: PathBuf) -> Self {
        Self {
            beacon_state_path,
            beacon_block_header_path,
        }
    }

    pub fn for_slot(base_path: &PathBuf, slot: u64) -> Self {
        let store_location_fs = Self::abs_path(PathBuf::from(base_path))
            .expect(&format!("Failed to convert {} into absolute path", base_path.display()));

        FileBasedBeaconStateReader::new(
            store_location_fs.join(format!("bs_{}.ssz", slot)),
            store_location_fs.join(format!("bs_{}_header.ssz", slot)),
        )
    }

    pub fn beacon_state_path(&self) -> &Path {
        return &self.beacon_state_path;
    }

    pub fn beacon_block_header_path(&self) -> &Path {
        return &self.beacon_block_header_path;
    }

    pub fn beacon_state_exists(&self) -> bool {
        Self::exists(&self.beacon_state_path)
    }

    pub fn beacon_block_header_exists(&self) -> bool {
        Self::exists(&self.beacon_block_header_path)
    }

    fn exists(path: &Path) -> bool {
        let result = Path::exists(&path);
        log::debug!("Checked path {:?} - {}", path, result);
        return result;
    }

    fn abs_path(path: PathBuf) -> io::Result<PathBuf> {
        if path.is_absolute() {
            Ok(path)
        } else {
            Ok(env::current_dir()?.join(path))
        }
    }
}

impl BeaconStateReader for FileBasedBeaconStateReader {
    async fn read_beacon_state(&self, _slot: u64) -> Result<BeaconState> {
        log::info!("Reading BeaconState from file {:?}", &self.beacon_state_path);
        let data = read_binary_file(&self.beacon_state_path)?;
        // TODO: better mapping ssz::DecodeError to std::error::Error/anyhow::Error
        BeaconState::from_ssz_bytes(&data).map_err(|decode_err| anyhow!("Couldn't decode ssz {:#?}", decode_err))
    }

    async fn read_beacon_block_header(&self, _slot: u64) -> Result<BeaconBlockHeader> {
        log::info!("Reading BeaconBlock from file {:?}", &self.beacon_block_header_path);
        let data = read_binary_file(&self.beacon_block_header_path)?;
        // TODO: better mapping ssz::DecodeError to std::error::Error/anyhow::Error
        BeaconBlockHeader::from_ssz_bytes(&data).map_err(|decode_err| anyhow!("Couldn't decode ssz {:#?}", decode_err))
    }
}
