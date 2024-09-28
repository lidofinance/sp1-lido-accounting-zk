use serde::de::DeserializeOwned;
use serde::Serialize;
use std::fs::{self, File};
use std::io::{BufReader, Read};
use std::path::Path;

#[derive(Debug)]
pub enum Error {
    JsonError(serde_json::Error),
    IoError(std::io::Error),
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

impl From<Error> for anyhow::Error {
    fn from(value: Error) -> Self {
        anyhow::anyhow!("{:#?}", value)
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

    std::fs::write(path, serde_json::to_string_pretty(value).unwrap())?;
    Ok(())
}
