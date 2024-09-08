pub mod beacon_state_reader_enum;

pub const ELF: &[u8] = include_bytes!("../../../program/elf/riscv32im-succinct-zkvm-elf");

pub const CONTRACT_ABI: &[u8] =
    include_bytes!("../../../contracts/out/Sp1LidoAccountingReportContract.sol/Sp1LidoAccountingReportContract.json");
