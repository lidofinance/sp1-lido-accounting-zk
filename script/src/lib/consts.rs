use hex_literal::hex;

pub struct NetworkConfig {
    pub chain_id: u64,
    pub genesis_block_timestamp: u64,
    pub verifier: [u8; 20],
    pub lido_withdrawal_credentials: [u8; 32],
}

// https://docs.succinct.xyz/onchain-verification/contract-addresses.html
const SP1_GATEWAY: [u8; 20] = hex!("3B6041173B80E77f038f3F2C0f9744f04837185e");

#[derive(Debug)]
pub enum Network {
    Mainnet,
    Sepolia,
    Holesky,
    Anvil,
}

pub mod lido_credentials {
    use hex_literal::hex;
    pub const MAINNET: [u8; 32] = hex!("010000000000000000000000b9d7934878b5fb9610b3fe8a5e441e8fad7e293f");
    pub const SEPOLIA: [u8; 32] = hex!("010000000000000000000000De7318Afa67eaD6d6bbC8224dfCe5ed6e4b86d76");
    pub const HOLESKY: [u8; 32] = hex!("010000000000000000000000F0179dEC45a37423EAD4FaD5fCb136197872EAd9");
}

impl Network {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Mainnet => "mainnet",
            Self::Sepolia => "sepolia",
            Self::Holesky => "holesky",
            Self::Anvil => "anvil",
        }
    }

    pub fn from_str(val: &str) -> Option<Self> {
        match val {
            "mainnet" => Some(Self::Mainnet),
            "sepolia" => Some(Self::Sepolia),
            "holesky" => Some(Self::Holesky),
            "anvil" => Some(Self::Anvil),
            _ => None,
        }
    }

    pub fn get_config(&self) -> NetworkConfig {
        match self {
            Self::Mainnet => NetworkConfig {
                chain_id: 1,
                genesis_block_timestamp: 1606824023,
                verifier: SP1_GATEWAY,
                lido_withdrawal_credentials: lido_credentials::MAINNET,
            },
            Self::Sepolia => NetworkConfig {
                chain_id: 11155111,
                genesis_block_timestamp: 1655733600,
                verifier: SP1_GATEWAY,
                lido_withdrawal_credentials: lido_credentials::SEPOLIA,
            },
            Self::Holesky => NetworkConfig {
                chain_id: 17000,
                genesis_block_timestamp: 1695902400,
                verifier: SP1_GATEWAY,
                lido_withdrawal_credentials: lido_credentials::HOLESKY,
            },
            Self::Anvil => NetworkConfig {
                chain_id: 31337,
                genesis_block_timestamp: 1655733600,
                verifier: SP1_GATEWAY,
                lido_withdrawal_credentials: lido_credentials::SEPOLIA,
            },
        }
    }
}
