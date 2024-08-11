use anyhow::Result;
use log;
use sp1_lido_accounting_zk_shared::beacon_state_reader::{BeaconStateReader, FileBasedBeaconStateReader};
use sp1_lido_accounting_zk_shared::eth_consensus_layer::{BeaconBlockHeader, BeaconState};
use std::fs::File;
use std::io;
use std::io::BufReader;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::{env, fs};

pub enum BalanceGenerationMode {
    RANDOM,
    SEQUENTIAL,
    FIXED,
}

impl BalanceGenerationMode {
    fn to_cmdline(&self) -> &'static str {
        match self {
            BalanceGenerationMode::RANDOM => "random",
            BalanceGenerationMode::SEQUENTIAL => "sequential",
            BalanceGenerationMode::FIXED => "fixed",
        }
    }
}

pub struct SyntheticBeaconStateReader {
    ssz_store_location: PathBuf,
    total_validator_number: u64,
    lido_validator_number: u64,
    balances_generation_mode: BalanceGenerationMode,
    with_check: bool,
    suppress_generator_output: bool,
}

impl SyntheticBeaconStateReader {
    // TODO: derive?
    pub fn new(
        ssz_store_location: PathBuf,
        total_validator_number: u64,
        lido_validator_number: u64,
        balances_generation_mode: BalanceGenerationMode,
        with_check: bool,
        suppress_generator_output: bool,
    ) -> Self {
        Self {
            ssz_store_location,
            total_validator_number,
            lido_validator_number,
            balances_generation_mode,
            with_check,
            suppress_generator_output,
        }
    }

    fn synth_gen_folder(&self) -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../synthetic_beacon_state_gen")
    }

    fn get_python(&self) -> PathBuf {
        let folder = self.synth_gen_folder();
        let failed_to_run_err = format!("Failed to execute poetry in {}", &folder.as_os_str().to_str().unwrap());
        let poerty_run = Command::new("poetry")
            .current_dir(&folder)
            .args(["env", "info", "-e"])
            .output()
            .expect(&failed_to_run_err);
        let full_output = std::str::from_utf8(&poerty_run.stdout)
            .expect("Failed to read output as string - should be path to python executable");
        let no_trailing_newline = full_output
            .strip_suffix("\n")
            .or(full_output.strip_suffix("\r\n"))
            .unwrap_or(full_output);
        log::debug!("Got python location {:?}", no_trailing_newline);
        PathBuf::from(&no_trailing_newline)
    }

    fn get_script(&self) -> PathBuf {
        self.synth_gen_folder().join("main.py")
    }

    fn create_file_reader(&self, slot: u64) -> FileBasedBeaconStateReader {
        FileBasedBeaconStateReader::for_slot(&self.ssz_store_location, slot)
    }

    fn create_manifesto_file_name(&self, slot: u64) -> PathBuf {
        PathBuf::from(&self.ssz_store_location)
            .join(format!("bs_{}_manifesto.json", slot))
            .canonicalize()
            .expect("Failed to canonicalize manifesto path")
    }

    async fn generate_beacon_state(&self, file_path: &Path, slot: u64) {
        log::info!("Generating synthetic beacon state to file {:?}", file_path);
        let python = self.get_python();
        let script = self.get_script();
        let mut command = Command::new(python);
        command
            .arg(script.as_os_str().to_str().unwrap())
            .args(["-f", &file_path.as_os_str().to_str().unwrap()])
            .args(["-v", &self.total_validator_number.to_string()])
            .args(["-l", &self.lido_validator_number.to_string()])
            .args(["-b", self.balances_generation_mode.to_cmdline()])
            .args(["-s", &slot.to_string()]);
        if self.with_check {
            command.arg("--check");
        }
        if self.suppress_generator_output {
            command.stdout(Stdio::null());
        }

        log::debug!("Built command {:?}", command);
        command.status().expect("Failed to execute beacon state generator");
    }

    pub async fn read_manifesto(&self, slot: u64) -> Result<serde_json::Value> {
        self.read_manifesto_from_file(&self.create_manifesto_file_name(slot))
            .await
    }

    async fn read_manifesto_from_file(&self, file_path: &Path) -> Result<serde_json::Value> {
        log::info!("Reading manifesto from file {:?}", file_path);
        let file = File::open(file_path)?;
        let reader = BufReader::new(file);

        // Read the JSON contents of the file as an instance of `User`.
        let res = serde_json::from_reader(reader)?;
        Ok(res)
    }

    pub fn evict_cache(&self, slot: u64) -> io::Result<()> {
        let file_reader = self.create_file_reader(slot);
        fs::remove_file(file_reader.beacon_state_path())?;
        fs::remove_file(file_reader.beacon_block_header_path())?;
        Ok(())
    }
}

impl BeaconStateReader for SyntheticBeaconStateReader {
    async fn read_beacon_state(&self, slot: u64) -> Result<BeaconState> {
        let file_reader = self.create_file_reader(slot);
        if !file_reader.beacon_state_exists() {
            self.generate_beacon_state(file_reader.beacon_state_path(), slot).await;
        }
        file_reader.read_beacon_state(slot).await
    }

    async fn read_beacon_block_header(&self, slot: u64) -> Result<BeaconBlockHeader> {
        let file_reader = self.create_file_reader(slot);
        if !file_reader.beacon_block_header_exists() {
            self.generate_beacon_state(file_reader.beacon_state_path(), slot).await;
        }
        file_reader.read_beacon_block_header(slot).await
    }
}
