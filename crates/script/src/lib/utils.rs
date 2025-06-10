use serde::de::DeserializeOwned;
use serde::Serialize;
use std::fmt;
use std::fs::{self, File};
use std::io::{BufReader, Read};
use std::path::Path;
use std::str::FromStr;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum Error {
    JsonError(serde_json::Error),
    IoError(std::io::Error),
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::JsonError(err) => write!(f, "JsonError({:#?}", err),
            Self::IoError(err) => write!(f, "IoError({:#?}", err),
        }
    }
}

impl From<std::io::Error> for Error {
    fn from(value: std::io::Error) -> Self {
        Error::IoError(value)
    }
}

impl From<serde_json::Error> for Error {
    fn from(value: serde_json::Error) -> Self {
        Error::JsonError(value)
    }
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
