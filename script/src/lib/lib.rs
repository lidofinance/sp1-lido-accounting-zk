pub mod beacon_state_reader_enum;
pub mod eth_client;
pub mod validator_delta;

pub const ELF: &[u8] = include_bytes!("../../../program/elf/riscv32im-succinct-zkvm-elf");

pub const CONTRACT_ABI: &str =
    "../../../contracts/out/Sp1LidoAccountingReportContract.sol/Sp1LidoAccountingReportContract.json";
// pub const CONTRACT_ABI_BYTES: &[u8] =
//     include_bytes!("../../../contracts/out/Sp1LidoAccountingReportContract.sol/Sp1LidoAccountingReportContract.json");
