use anyhow::anyhow;

use sp1_core_machine::io::SP1PublicValues; // TODO: remove when Sp1PublicValues are exported from sp1_sdk
use sp1_sdk::{
    ExecutionReport, HashableKey, ProverClient, SP1ProofWithPublicValues, SP1ProvingKey, SP1Stdin, SP1VerifyingKey,
};

use anyhow::Result;
use log;
use sp1_lido_accounting_zk_shared::io::program_io::ProgramInput;

pub trait SP1ClientWrapper {
    fn vk(&self) -> &'_ SP1VerifyingKey;
    fn vk_bytes(&self) -> [u8; 32];
    fn prove(&self, input: ProgramInput) -> Result<SP1ProofWithPublicValues>;
    fn verify_proof(&self, proof: &SP1ProofWithPublicValues) -> Result<()>;
    fn execute(&self, input: ProgramInput) -> Result<(SP1PublicValues, ExecutionReport)>;
}

pub struct SP1ClientWrapperImpl {
    client: ProverClient,
    elf: Vec<u8>,
    pk: SP1ProvingKey,
    vk: SP1VerifyingKey,
}

impl SP1ClientWrapperImpl {
    pub fn new(client: ProverClient, elf: &[u8]) -> Self {
        let (pk, vk) = client.setup(elf);
        Self {
            client,
            elf: elf.to_owned(),
            pk,
            vk,
        }
    }

    fn write_sp1_stdin(&self, program_input: &ProgramInput) -> SP1Stdin {
        log::info!("Writing program input to SP1Stdin");
        let mut stdin: SP1Stdin = SP1Stdin::new();
        stdin.write(&program_input);
        stdin
    }
}

impl SP1ClientWrapper for SP1ClientWrapperImpl {
    fn vk(&self) -> &'_ SP1VerifyingKey {
        &self.vk
    }

    fn vk_bytes(&self) -> [u8; 32] {
        let mut vk_bytes: [u8; 32] = [0; 32];
        let vk = self.vk.bytes32();
        let stripped_vk = vk.strip_prefix("0x").unwrap_or(&vk);
        hex::decode_to_slice(stripped_vk.as_bytes(), &mut vk_bytes)
            .expect("Failed to decode verification key to [u8; 32]");
        vk_bytes
    }

    fn prove(&self, input: ProgramInput) -> Result<SP1ProofWithPublicValues> {
        let sp1_stdin = self.write_sp1_stdin(&input);
        self.client.prove(&self.pk, sp1_stdin).plonk().run()
    }

    fn verify_proof(&self, proof: &SP1ProofWithPublicValues) -> Result<()> {
        log::info!("Verifying proof");
        self.client
            .verify(proof, &self.vk)
            .map_err(|err| anyhow!("Couldn't verify {:#?}", err))
    }

    fn execute(&self, input: ProgramInput) -> Result<(SP1PublicValues, ExecutionReport)> {
        let sp1_stdin = self.write_sp1_stdin(&input);
        self.client.execute(self.elf.as_slice(), sp1_stdin).run()
    }
}
