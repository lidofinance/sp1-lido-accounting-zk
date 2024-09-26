use anyhow::anyhow;

use sp1_core_machine::io::SP1PublicValues; // TODO: remove when Sp1PublicValues are exported from sp1_sdk
use sp1_sdk::{
    ExecutionReport, HashableKey, ProverClient, SP1ProofWithPublicValues, SP1ProvingKey, SP1Stdin, SP1VerifyingKey,
};

use anyhow::Result;
use log;
use sp1_lido_accounting_zk_shared::io::program_io::ProgramInput;

pub struct SP1ClientWrapper {
    client: ProverClient,
    elf: Vec<u8>,
    pk: SP1ProvingKey,
    vk: SP1VerifyingKey,
}

impl SP1ClientWrapper {
    pub fn new(client: ProverClient, elf: &[u8]) -> Self {
        let (pk, vk) = client.setup(elf);
        Self {
            client,
            elf: elf.to_owned(),
            pk,
            vk,
        }
    }

    pub fn vk(&self) -> &'_ SP1VerifyingKey {
        &self.vk
    }

    pub fn vk_bytes(&self) -> [u8; 32] {
        let mut vk_bytes: [u8; 32] = [0; 32];
        let vk = self.vk.bytes32();
        let stripped_vk = vk.strip_prefix("0x").unwrap_or(&vk);
        hex::decode_to_slice(stripped_vk.as_bytes(), &mut vk_bytes)
            .expect("Failed to decode verification key to [u8; 32]");
        vk_bytes
    }

    pub fn prove(&self, input: ProgramInput) -> Result<SP1ProofWithPublicValues> {
        let sp1_stdin = self.write_sp1_stdin(&input);
        self.client.prove(&self.pk, sp1_stdin).plonk().run()
    }

    pub fn verify_proof(&self, proof: &SP1ProofWithPublicValues) -> Result<()> {
        log::info!("Verifying proof");
        self.client
            .verify(proof, &self.vk)
            .map_err(|err| anyhow!("Couldn't verify {:#?}", err))
    }

    pub fn execute(&self, input: ProgramInput) -> Result<(SP1PublicValues, ExecutionReport)> {
        let sp1_stdin = self.write_sp1_stdin(&input);
        self.client.execute(self.elf.as_slice(), sp1_stdin).run()
    }

    fn write_sp1_stdin(&self, program_input: &ProgramInput) -> SP1Stdin {
        log::info!("Writing program input to SP1Stdin");
        let mut stdin: SP1Stdin = SP1Stdin::new();
        stdin.write(&program_input);
        stdin
    }
}
