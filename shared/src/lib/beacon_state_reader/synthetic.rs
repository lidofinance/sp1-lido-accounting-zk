use anyhow::Result;
use log;
use std::env;
use std::io;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use super::file::{read_untyped_json, FileBasedBeaconChainStore};

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

pub struct SyntheticBeaconStateCreator {
    file_store: FileBasedBeaconChainStore,
    with_check: bool,
    suppress_generator_output: bool,
}

pub struct GenerationSpec {
    pub slot: u64,
    pub non_lido_validators: u64,
    pub deposited_lido_validators: u64,
    pub exited_lido_validators: u64,
    pub pending_deposit_lido_validators: u64,
    pub balances_generation_mode: BalanceGenerationMode,
    pub shuffle: bool,
    pub base_slot: Option<u64>,
    pub overwrite: bool,
}

impl SyntheticBeaconStateCreator {
    // TODO: derive?
    pub fn new(ssz_store_location: &Path, with_check: bool, suppress_generator_output: bool) -> Self {
        Self {
            file_store: FileBasedBeaconChainStore::new(ssz_store_location),
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

    fn create_manifesto_file_name(&self, slot: u64) -> PathBuf {
        PathBuf::from(&self.file_store.store_location)
            .join(format!("bs_{}_manifesto.json", slot))
            .canonicalize()
            .expect("Failed to canonicalize manifesto path")
    }

    async fn generate_beacon_state(&self, file_path: &Path, generation_spec: GenerationSpec) {
        log::info!("Generating synthetic beacon state to file {:?}", file_path);
        let python = self.get_python();
        let script = self.get_script();
        let mut command = Command::new(python);
        command
            .arg(script.as_os_str().to_str().unwrap())
            .args(["--file", &file_path.as_os_str().to_str().unwrap()])
            .args([
                "--non_lido_validators",
                &generation_spec.non_lido_validators.to_string(),
            ])
            .args([
                "--deposited_lido_validators",
                &generation_spec.deposited_lido_validators.to_string(),
            ])
            .args([
                "--exited_lido_validators",
                &generation_spec.exited_lido_validators.to_string(),
            ])
            .args([
                "--pending_deposit_lido_validators",
                &generation_spec.pending_deposit_lido_validators.to_string(),
            ])
            .args(["--balances_mode", generation_spec.balances_generation_mode.to_cmdline()])
            .args(["--slot", &generation_spec.slot.to_string()]);
        if self.with_check {
            command.arg("--check");
        }
        if generation_spec.shuffle {
            command.arg("--shuffle");
        }
        if let Some(base_slot) = generation_spec.base_slot {
            let old_beacon_state_file = self.file_store.get_beacon_state_path(base_slot);
            assert!(
                self.exists(&old_beacon_state_file),
                "Beacon state for base slot {} was not found at {:?}",
                base_slot,
                old_beacon_state_file
            );
            command.args(["--start_from", old_beacon_state_file.as_os_str().to_str().unwrap()]);
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
        let res = read_untyped_json(file_path).await?;
        Ok(res)
    }

    pub fn evict_cache(&self, slot: u64) -> io::Result<()> {
        let beacon_state_file = self.file_store.get_beacon_state_path(slot);
        if self.exists(&beacon_state_file) {
            log::debug!("Evicting beacon state file");
            FileBasedBeaconChainStore::delete(&beacon_state_file)?;
        }

        let beacon_block_header_file = self.file_store.get_beacon_block_header_path(slot);
        if self.exists(&beacon_block_header_file) {
            log::debug!("Evicting beacon block state file");
            FileBasedBeaconChainStore::delete(&beacon_block_header_file)?;
        }
        Ok(())
    }

    pub fn exists(&self, path: &Path) -> bool {
        FileBasedBeaconChainStore::exists(path)
    }

    pub async fn create_beacon_state(&self, generation_spec: GenerationSpec) -> Result<()> {
        if generation_spec.overwrite {
            self.evict_cache(generation_spec.slot)?;
        }
        let beacon_state_file = self.file_store.get_beacon_state_path(generation_spec.slot);
        if !self.exists(&beacon_state_file) {
            self.generate_beacon_state(&beacon_state_file, generation_spec).await;
        }
        Ok(())
    }
}
