use sp1_sdk::include_elf;

pub const ELF: &[u8] = include_elf!("sp1-lido-accounting-zk-program");

pub mod sp1_verifier {
    use hex_literal::hex;
    // https://docs.succinct.xyz/onchain-verification/contract-addresses.html
    // const SP1_GROTH_GATEWAY: [u8; 20] = hex!("397A5f7f3dBd538f23DE225B51f532c34448dA9B");
    // const SP1_PLONK_GATEWAY: [u8; 20] = hex!("3B6041173B80E77f038f3F2C0f9744f04837185e");

    // https://github.com/succinctlabs/sp1-contracts/tree/main/contracts/deployments
    // The contract addresses matches between mainnet, sepolia and holesky
    const SP1_GROTH_VERIFIER: [u8; 20] = hex!("a27A057CAb1a4798c6242F6eE5b2416B7Cd45E5D");
    const SP1_PLONK_VERIFIER: [u8; 20] = hex!("E00a3cBFC45241b33c0A44C78e26168CBc55EC63");

    pub enum VerificationMode {
        Groth16,
        Plonk,
    }

    pub const VERIFICATION_MODE: VerificationMode = VerificationMode::Plonk;
    pub static VERIFIER_ADDRESS: [u8; 20] = match VERIFICATION_MODE {
        VerificationMode::Groth16 => SP1_GROTH_VERIFIER,
        VerificationMode::Plonk => SP1_PLONK_VERIFIER,
    };
}

pub struct NetworkConfig {
    pub chain_id: u64,
    pub genesis_block_timestamp: u64,
    pub verifier: [u8; 20],
    pub lido_withdrawal_credentials: [u8; 32],
    pub lido_withdrwawal_vault_address: [u8; 20],
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
}

impl NetworkInfo for Network {
    fn as_str(&self) -> String {
        let val = match self {
            Self::Mainnet => "mainnet",
            Self::Sepolia => "sepolia",
            Self::Holesky => "holesky",
        };
        val.to_owned()
    }

    fn get_config(&self) -> NetworkConfig {
        match self {
            Self::Mainnet => NetworkConfig {
                chain_id: 1,
                genesis_block_timestamp: 1606824023,
                verifier: sp1_verifier::VERIFIER_ADDRESS,
                lido_withdrawal_credentials: lido_credentials::MAINNET,
                lido_withdrwawal_vault_address: lido_withdrawal_vault::MAINNET,
            },
            Self::Sepolia => NetworkConfig {
                chain_id: 11155111,
                genesis_block_timestamp: 1655733600,
                verifier: sp1_verifier::VERIFIER_ADDRESS,
                lido_withdrawal_credentials: lido_credentials::SEPOLIA,
                lido_withdrwawal_vault_address: lido_withdrawal_vault::SEPOLIA,
            },
            Self::Holesky => NetworkConfig {
                chain_id: 17000,
                genesis_block_timestamp: 1695902400,
                verifier: sp1_verifier::VERIFIER_ADDRESS,
                lido_withdrawal_credentials: lido_credentials::HOLESKY,
                lido_withdrwawal_vault_address: lido_withdrawal_vault::HOLESKY,
            },
        }
    }
}

#[derive(Debug, Clone)]
pub enum WrappedNetwork {
    Anvil(Network),
    Id(Network),
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

pub mod lido_credentials {
    use hex_literal::hex;
    pub const MAINNET: [u8; 32] = hex!("010000000000000000000000b9d7934878b5fb9610b3fe8a5e441e8fad7e293f");
    pub const SEPOLIA: [u8; 32] = hex!("010000000000000000000000De7318Afa67eaD6d6bbC8224dfCe5ed6e4b86d76");
    pub const HOLESKY: [u8; 32] = hex!("010000000000000000000000F0179dEC45a37423EAD4FaD5fCb136197872EAd9");
}

pub mod lido_withdrawal_vault {
    use hex_literal::hex;
    pub const MAINNET: [u8; 20] = hex!("b9d7934878b5fb9610b3fe8a5e441e8fad7e293f");
    pub const SEPOLIA: [u8; 20] = hex!("De7318Afa67eaD6d6bbC8224dfCe5ed6e4b86d76");
    pub const HOLESKY: [u8; 20] = hex!("F0179dEC45a37423EAD4FaD5fCb136197872EAd9");
}

pub fn read_network(val: &str) -> WrappedNetwork {
    let is_anvil = val.starts_with("anvil");
    let base_network: &str = if is_anvil {
        let mut parts = val.splitn(2, '-');
        parts.nth(1).unwrap()
    } else {
        val
    };

    let network = match base_network {
        "mainnet" => Network::Mainnet,
        "sepolia" => Network::Sepolia,
        "holesky" => Network::Holesky,
        _ => panic!("Unknown network"),
    };

    if is_anvil {
        WrappedNetwork::Anvil(network)
    } else {
        WrappedNetwork::Id(network)
    }
}
