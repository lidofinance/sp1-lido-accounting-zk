pub const ELF: &[u8] = include_bytes!("../../../elf/riscv32im-succinct-zkvm-elf");

pub mod sp1_verifier {
    use hex_literal::hex;
    // https://github.com/succinctlabs/sp1-contracts/tree/main/contracts/deployments
    // The contract addresses matches between mainnet, sepolia and holesky
    const SP1_GROTH_VERIFIER: [u8; 20] = hex!("E780809121774D06aD9B0EEeC620fF4B3913Ced1");
    const SP1_PLONK_VERIFIER: [u8; 20] = hex!("d2832Cf1fC8bA210FfABF62Db9A8781153131d16");

    pub enum VerificationMode {
        Groth16,
        Plonk,
    }

    pub const VERIFICATION_MODE: VerificationMode = VerificationMode::Groth16;
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
        let value = match self {
            Self::Mainnet => "mainnet",
            Self::Sepolia => "sepolia",
            Self::Holesky => "holesky",
        };
        value.to_owned()
    }

    fn get_config(&self) -> NetworkConfig {
        match self {
            Self::Mainnet => NetworkConfig {
                chain_id: 1,
                genesis_block_timestamp: 1606824023,
                verifier: sp1_verifier::VERIFIER_ADDRESS,
                lido_withdrawal_credentials: lido_credentials::MAINNET,
            },
            Self::Sepolia => NetworkConfig {
                chain_id: 11155111,
                genesis_block_timestamp: 1655733600,
                verifier: sp1_verifier::VERIFIER_ADDRESS,
                lido_withdrawal_credentials: lido_credentials::SEPOLIA,
            },
            Self::Holesky => NetworkConfig {
                chain_id: 17000,
                genesis_block_timestamp: 1695902400,
                verifier: sp1_verifier::VERIFIER_ADDRESS,
                lido_withdrawal_credentials: lido_credentials::HOLESKY,
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
