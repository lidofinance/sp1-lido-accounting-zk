use std::str::FromStr;

pub struct NetworkConfig {
    pub chain_id: u64,
    pub genesis_block_timestamp: u64,
}

pub trait NetworkInfo {
    fn as_str(&self) -> String;
    fn get_config(&self) -> NetworkConfig;
}

#[derive(Debug, Clone)]
pub enum Network {
    Mainnet,
    Sepolia,
    Holesky,
    Hoodi,
    Fusaka,
}

#[derive(Debug, thiserror::Error)]
pub enum NetworkParseError {
    #[error("Failed to parse network,  value={value}, error={error}")]
    FailedToParseNetwork { value: String, error: String },
}

impl FromStr for Network {
    type Err = NetworkParseError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "mainnet" => Ok(Self::Mainnet),
            "sepolia" => Ok(Self::Sepolia),
            "holesky" => Ok(Self::Holesky),
            "hoodi" => Ok(Self::Hoodi),
            "fusaka" => Ok(Self::Fusaka),
            val => Err(NetworkParseError::FailedToParseNetwork {
                value: val.to_string(),
                error: "Unknown network".to_string(),
            }),
        }
    }
}

impl NetworkInfo for Network {
    fn as_str(&self) -> String {
        let val = match self {
            Self::Mainnet => "mainnet",
            Self::Sepolia => "sepolia",
            Self::Holesky => "holesky",
            Self::Hoodi => "hoodi",
            Self::Fusaka => "fusaka",
        };
        val.to_owned()
    }

    fn get_config(&self) -> NetworkConfig {
        match self {
            Self::Mainnet => NetworkConfig {
                chain_id: 1,
                genesis_block_timestamp: 1606824023,
            },
            Self::Sepolia => NetworkConfig {
                chain_id: 11155111,
                genesis_block_timestamp: 1655733600,
            },
            Self::Holesky => NetworkConfig {
                chain_id: 17000,
                genesis_block_timestamp: 1695902400,
            },
            Self::Hoodi => NetworkConfig {
                chain_id: 560048,
                genesis_block_timestamp: 1742213400,
            },
            Self::Fusaka => NetworkConfig {
                chain_id: 7023102237,
                genesis_block_timestamp: 1753280940,
            },
        }
    }
}

#[derive(Debug, Clone)]
pub enum WrappedNetwork {
    Anvil(Network),
    Id(Network),
}

impl FromStr for WrappedNetwork {
    type Err = NetworkParseError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value.split_once('-') {
            Some(("anvil", network_id)) => {
                let network = network_id.parse::<Network>()?;
                Ok(Self::Anvil(network))
            }
            _ => {
                let network = value.parse::<Network>()?;
                Ok(Self::Id(network))
            }
        }
    }
}

impl NetworkInfo for WrappedNetwork {
    fn as_str(&self) -> String {
        match self {
            Self::Anvil(fork) => format!("anvil-{}", fork.as_str()),
            Self::Id(network) => network.as_str().to_owned(),
        }
    }

    fn get_config(&self) -> NetworkConfig {
        match self {
            Self::Id(network) => network.get_config(),
            Self::Anvil(fork) => {
                let mut fork_config = fork.get_config();
                fork_config.chain_id = 31337;
                fork_config
            }
        }
    }
}
