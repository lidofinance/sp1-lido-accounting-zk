pub mod scripts;
#[cfg(feature = "synthetic")]
pub mod synthetic;

pub mod lido {
    pub mod withdrawal_credentials {
        use hex_literal::hex;
        pub const MAINNET: [u8; 32] =
            hex!("010000000000000000000000b9d7934878b5fb9610b3fe8a5e441e8fad7e293f");
    }
}
