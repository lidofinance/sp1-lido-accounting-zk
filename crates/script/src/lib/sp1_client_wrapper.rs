use sp1_sdk::{
    EnvProver, ExecutionReport, HashableKey, SP1ProofWithPublicValues, SP1ProvingKey, SP1PublicValues, SP1Stdin,
    SP1VerifyingKey,
};

use sp1_lido_accounting_zk_shared::io::program_io::ProgramInput;

use sp1_sdk::include_elf;

pub const ELF: &[u8] = include_elf!("sp1-lido-accounting-zk-program");

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("Failed to generate proof {0:?}")]
    Sp1ProveError(anyhow::Error), // prove.run uses anyhow::Result, so cannot be more precise

    #[error("Failed to execute program {0:?}")]
    Sp1ExecuteError(anyhow::Error), // execute.run uses anyhow::Result, so cannot be more precise

    #[error("Failed to verify proof {0:?}")]
    Sp1VerificationError(#[from] sp1_sdk::SP1VerificationError),

    #[error("Error decoding vkey from hex {0:?}")]
    HexDecodeErrro(#[from] hex::FromHexError),
}

pub type Result<T> = std::result::Result<T, Error>;

pub trait SP1ClientWrapper {
    fn vk(&self) -> &'_ SP1VerifyingKey;
    fn vk_bytes(&self) -> Result<[u8; 32]>;
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
        tracing::debug!("Writing program input to SP1Stdin");
        let mut stdin: SP1Stdin = SP1Stdin::new();
        stdin.write(&program_input);
        stdin
    }
}

impl SP1ClientWrapper for SP1ClientWrapperImpl {
    fn vk(&self) -> &'_ SP1VerifyingKey {
        &self.vk
    }

    fn vk_bytes(&self) -> Result<[u8; 32]> {
        let mut vk_bytes: [u8; 32] = [0; 32];
        let vk = self.vk.bytes32();
        let stripped_vk = vk.strip_prefix("0x").unwrap_or(&vk);
        hex::decode_to_slice(stripped_vk.as_bytes(), &mut vk_bytes)?;
        Ok(vk_bytes)
    }

    fn prove(&self, input: ProgramInput) -> Result<SP1ProofWithPublicValues> {
        let sp1_stdin = self.write_sp1_stdin(&input);
        let prove_spec = self.client.prove(&self.pk, &sp1_stdin);
        let result = prove_spec
            .plonk()
            .run()
            .map_err(Error::Sp1ProveError)
            .inspect(|_v| tracing::info!("Successfully obtained proof"))
            .inspect_err(|e| tracing::error!("Failed to generate proof: {e:?}"))?;
        Ok(result)
    }

    fn verify_proof(&self, proof: &SP1ProofWithPublicValues) -> Result<()> {
        tracing::info!("Verifying proof");
        self.client.verify(proof, &self.vk)?;
        Ok(())
    }

    fn execute(&self, input: ProgramInput) -> Result<(SP1PublicValues, ExecutionReport)> {
        let sp1_stdin = self.write_sp1_stdin(&input);
        let result = self
            .client
            .execute(self.elf.as_slice(), &sp1_stdin)
            .run()
            .map_err(Error::Sp1ExecuteError)
            .inspect(|_v| tracing::info!("Successfully executed program"))
            .inspect_err(|e| tracing::error!("Failed to execute program: {e:?}"))?;
        Ok(result)
    }
}
