use sp1_lido_accounting_scripts::sp1_client_wrapper;
use sp1_sdk::{Prover, ProverClient};

#[tokio::main]
async fn main() {
    sp1_sdk::utils::setup_logger();
    let sp1_client = ProverClient::builder().network().build();
    let (_pk, vk) = sp1_client.setup(sp1_client_wrapper::ELF);
    let vk_bytes = sp1_client_wrapper::vk_bytes(&vk).unwrap();
    println!("0x{}", hex::encode(vk_bytes));
}
