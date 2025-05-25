use anyhow::anyhow;

use sp1_sdk::{
    EnvProver, ExecutionReport, HashableKey, SP1ProofWithPublicValues, SP1ProvingKey, SP1PublicValues, SP1Stdin,
    SP1VerifyingKey,
};

use anyhow::Result;
use sp1_lido_accounting_zk_shared::io::program_io::ProgramInput;

use crate::consts::{self, sp1_verifier::VerificationMode};

use sp1_sdk::include_elf;

pub const ELF: &[u8] = include_elf!("sp1-lido-accounting-zk-program");

pub trait SP1ClientWrapper {
    fn vk(&self) -> &'_ SP1VerifyingKey;
    fn vk_bytes(&self) -> [u8; 32];
    fn prove(&self, input: ProgramInput) -> Result<SP1ProofWithPublicValues>;
    fn verify_proof(&self, proof: &SP1ProofWithPublicValues) -> Result<()>;
    fn execute(&self, input: ProgramInput) -> Result<(SP1PublicValues, ExecutionReport)>;
}

pub struct SP1ClientWrapperImpl {
    client: EnvProver,
    elf: Vec<u8>,
    pk: SP1ProvingKey,
    vk: SP1VerifyingKey,
}

impl SP1ClientWrapperImpl {
    pub fn new(client: EnvProver) -> Self {
        let (pk, vk) = client.setup(ELF);
        Self {
            client,
            elf: ELF.to_owned(),
            pk,
            vk,
        }
    }

    fn write_sp1_stdin(&self, program_input: &ProgramInput) -> SP1Stdin {
        tracing::info!("Writing program input to SP1Stdin");
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
        let prove_spec = self.client.prove(&self.pk, &sp1_stdin);
        let prove_mode = match consts::sp1_verifier::VERIFICATION_MODE {
            VerificationMode::Groth16 => prove_spec.groth16(),
            VerificationMode::Plonk => prove_spec.plonk(),
        };
        prove_mode.run()
    }

    fn verify_proof(&self, proof: &SP1ProofWithPublicValues) -> Result<()> {
        tracing::info!("Verifying proof");
        self.client
            .verify(proof, &self.vk)
            .map_err(|err| anyhow!("Couldn't verify {:#?}", err))
    }

    fn execute(&self, input: ProgramInput) -> Result<(SP1PublicValues, ExecutionReport)> {
        let sp1_stdin = self.write_sp1_stdin(&input);
        self.client.execute(self.elf.as_slice(), &sp1_stdin).run()
    }
}
