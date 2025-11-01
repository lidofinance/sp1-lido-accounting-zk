use serde::de::DeserializeOwned;
use serde::Serialize;
use std::fs::{self, File};
use std::io::{BufReader, Read};
use std::path::Path;
use std::str::FromStr;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum Error {
    #[error("Json error: {0}")]
    JsonError(#[from] serde_json::Error),
    #[error("Io error: {0}")]
    IoError(#[from] std::io::Error),
}

pub type Result<T> = std::result::Result<T, Error>;

pub fn read_binary<P: AsRef<Path>>(path: P) -> Result<Vec<u8>> {
    let mut file = File::open(path)?;
    let mut buffer = Vec::new();
    file.read_to_end(&mut buffer)?;
    Ok(buffer)
}

pub fn read_untyped_json<P: AsRef<Path>>(path: P) -> Result<serde_json::Value> {
    let file = File::open(path)?;
    let res = serde_json::from_reader(BufReader::new(file))?;
    Ok(res)
}

pub fn read_json<P: AsRef<Path>, T: DeserializeOwned>(path: P) -> Result<T> {
    let file_content = fs::read(path)?;
    let res = serde_json::from_slice(file_content.as_slice())?;
    Ok(res)
}

pub fn write_json<T: Serialize>(path: &Path, value: &T) -> Result<()> {
    // Save the fixture to a file.
    if let Some(fixture_path) = path.parent() {
        std::fs::create_dir_all(fixture_path).expect("failed to create fixture path");
    }

    let json = serde_json::to_string_pretty(value)?;
    std::fs::write(path, json)?;
    Ok(())
}

pub fn read_env<T: FromStr>(env_var: &str, default: T) -> T {
    if let Ok(str) = std::env::var(env_var) {
        if let Ok(value) = T::from_str(&str) {
            value
        } else {
            default
        }
    } else {
        default
    }
}
