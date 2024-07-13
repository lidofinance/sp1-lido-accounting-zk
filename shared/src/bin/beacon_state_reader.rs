use eth_consensus_layer_ssz::BeaconState;
use log;
use ssz::{Decode, Encode};
use std::fs::File;
use std::io::{self, Read};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

pub trait BeaconStateReader {
    async fn read_beacon_state(&self, slot: u64) -> BeaconState;
}

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
        let failed_to_run_err = format!(
            "Failed to execute poetry in {}",
            &folder.as_os_str().to_str().unwrap()
        );
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
        return PathBuf::from(&no_trailing_newline);
    }

    fn get_script(&self) -> PathBuf {
        return self.synth_gen_folder().join("main.py");
    }

    fn create_file_name(&self, slot: u64) -> PathBuf {
        return PathBuf::from(&self.ssz_store_location).join(format!("bs_{}.ssz", slot));
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
        command
            .status()
            .expect("Failed to execute beacon state generator");
    }

    async fn read_beacon_state_from_file(&self, file_path: &Path) -> BeaconState {
        log::info!("Reading from file {:?}", file_path);
        let data = read_binary_file(file_path).unwrap();
        return BeaconState::from_ssz_bytes(&data).unwrap();
    }
}

impl BeaconStateReader for SyntheticBeaconStateReader {
    async fn read_beacon_state(&self, slot: u64) -> BeaconState {
        let file_name = self.create_file_name(slot);
        self.generate_beacon_state(&file_name, slot).await;
        return self.read_beacon_state_from_file(&file_name).await;
    }
}

fn read_binary_file<P: AsRef<Path>>(path: P) -> io::Result<Vec<u8>> {
    let mut file = File::open(path)?;
    let mut buffer = Vec::new();
    file.read_to_end(&mut buffer)?;
    Ok(buffer)
}
