use anyhow::{anyhow, Result};
use log;
use ssz::Decode;
use std::fs::File;
use std::io::{self, Read};
use std::path::{Path, PathBuf};

use crate::eth_consensus_layer::BeaconState;

pub trait BeaconStateReader {
    async fn read_beacon_state(&self, slot: u64) -> Result<BeaconState>;
}

pub struct FileBasedBeaconStateReader {
    file_path: PathBuf,
}

pub fn read_binary_file<P: AsRef<Path>>(path: P) -> io::Result<Vec<u8>> {
    let mut file = File::open(path)?;
    let mut buffer = Vec::new();
    file.read_to_end(&mut buffer)?;
    Ok(buffer)
}

impl FileBasedBeaconStateReader {
    pub fn new(file_path: PathBuf) -> Self {
        Self { file_path }
    }
}

impl BeaconStateReader for FileBasedBeaconStateReader {
    async fn read_beacon_state(&self, _slot: u64) -> Result<BeaconState> {
        log::info!("Reading from file {:?}", &self.file_path);
        let data = read_binary_file(&self.file_path)?;
        // TODO: better mapping ssz::DecodeError to std::error::Error/anyhow::Error
        BeaconState::from_ssz_bytes(&data).map_err(|decode_err| anyhow!("Couldn't decode ssz {:#?}", decode_err))
    }
}
