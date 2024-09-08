use sp1_lido_accounting_zk_shared::beacon_state_reader::file::FileBasedBeaconStateReader;
use sp1_lido_accounting_zk_shared::beacon_state_reader::reqwest::{
    CachedReqwestBeaconStateReader, ReqwestBeaconStateReader,
};
use sp1_lido_accounting_zk_shared::beacon_state_reader::BeaconStateReader;
use sp1_lido_accounting_zk_shared::consts::Network;
use sp1_lido_accounting_zk_shared::eth_consensus_layer::{BeaconBlockHeader, BeaconState};
use std::env;
use std::path::PathBuf;

pub enum BeaconStateReaderEnum {
    File(FileBasedBeaconStateReader),
    RPC(ReqwestBeaconStateReader),
    RPCCached(CachedReqwestBeaconStateReader),
}

impl BeaconStateReaderEnum {
    pub fn new_from_env(network: Network) -> BeaconStateReaderEnum {
        let bs_reader_mode_var = env::var("BS_READER_MODE").expect("Failed to read BS_READER_MODE from env");
        let maybe_file_store_location =
            env::var("BS_FILE_STORE").map(|base| PathBuf::from(base).join(network.as_str()));
        let maybe_rpc_endpoint = env::var("CONSENSUS_LAYER_RPC");
        let maybe_bs_endpoint = env::var("BEACON_STATE_RPC");

        match bs_reader_mode_var.to_lowercase().as_str() {
            "file" => {
                let file_store = maybe_file_store_location.expect(&format!(
                    "BS_FILE_STORE must be specified for mode {bs_reader_mode_var}"
                ));
                return BeaconStateReaderEnum::File(FileBasedBeaconStateReader::new(&file_store));
            }
            "rpc" => {
                let rpc_endpoint = maybe_rpc_endpoint.expect(&format!(
                    "CONSENSUS_LAYER_RPC must be specified for mode {bs_reader_mode_var}"
                ));
                let bs_endpoint = maybe_bs_endpoint.expect(&format!(
                    "BEACON_STATE_RPC must be specified for mode {bs_reader_mode_var}"
                ));
                return BeaconStateReaderEnum::RPC(ReqwestBeaconStateReader::new(&rpc_endpoint, &bs_endpoint));
            }
            "rpc_cached" => {
                let file_store = maybe_file_store_location.expect(&format!(
                    "BS_FILE_STORE must be specified for mode {bs_reader_mode_var}"
                ));
                let rpc_endpoint = maybe_rpc_endpoint.expect(&format!(
                    "CONSENSUS_LAYER_RPC must be specified for mode {bs_reader_mode_var}"
                ));
                let bs_endpoint = maybe_bs_endpoint.expect(&format!(
                    "BEACON_STATE_RPC must be specified for mode {bs_reader_mode_var}"
                ));
                return BeaconStateReaderEnum::RPCCached(CachedReqwestBeaconStateReader::new(
                    &rpc_endpoint,
                    &bs_endpoint,
                    &file_store,
                ));
            }
            _ => {
                panic!("invalid value for BS_READER_MODE enviroment variable: expected 'file', 'rpc', or 'rpc_cached'")
            }
        }
    }
}

impl BeaconStateReader for BeaconStateReaderEnum {
    async fn read_beacon_state(&self, slot: u64) -> anyhow::Result<BeaconState> {
        match self {
            Self::File(reader) => reader.read_beacon_state(slot).await,
            Self::RPC(reader) => reader.read_beacon_state(slot).await,
            Self::RPCCached(reader) => reader.read_beacon_state(slot).await,
        }
    }

    async fn read_beacon_block_header(&self, slot: u64) -> anyhow::Result<BeaconBlockHeader> {
        match self {
            Self::File(reader) => reader.read_beacon_block_header(slot).await,
            Self::RPC(reader) => reader.read_beacon_block_header(slot).await,
            Self::RPCCached(reader) => reader.read_beacon_block_header(slot).await,
        }
    }
}
