//! An end-to-end example of using the SP1 SDK to generate a proof of a program that can be verified
//! on-chain.
//!
//! You can run this script using the following command:
//! ```shell
//! RUST_LOG=info cargo run --package fibonacci-script --bin prove --release
//! ```

use alloy_sol_types::SolType;
use anyhow::anyhow;
use clap::Parser;
use hex;
use hex_literal::hex as h;
use serde::{Deserialize, Serialize};
use sp1_sdk::{
    ExecutionReport, HashableKey, ProverClient, SP1ProofWithPublicValues, SP1ProvingKey, SP1PublicValues, SP1Stdin,
    SP1VerifyingKey,
};
use std::fs;
use std::path::PathBuf;

use sp1_lido_accounting_zk_shared::{
    beacon_state_reader::{BeaconStateReader, FileBasedBeaconStateReader},
    eth_consensus_layer::{BeaconBlockHeaderPrecomputedHashes, BeaconStatePrecomputedHashes, Hash256},
    eth_spec,
    io::{
        eth_io::{PublicValuesRust, PublicValuesSolidity, ReportMetadataRust, ReportRust},
        program_io::{ProgramInput, ValsAndBals},
    },
    report::ReportData,
};

use sp1_lido_accounting_zk_shared::eth_consensus_layer::Unsigned;
use sp1_lido_accounting_zk_shared::verification::{FieldProof, MerkleTreeFieldLeaves};

use anyhow::Result;
use log;

use dotenv::dotenv;
use std::env;

use tree_hash::TreeHash;

const ELF: &[u8] = include_bytes!("../../../program/elf/riscv32im-succinct-zkvm-elf");

/// The arguments for the prove command.
#[derive(Parser, Debug)]
#[clap(author, version, about, long_about = None)]
struct ProveArgs {
    #[clap(long, default_value = "false")]
    evm: bool,
    #[clap(long, default_value = "false")]
    prove: bool,
    #[clap(long)]
    path: PathBuf,
    #[clap(long)]
    slot: u64,
}

trait ScriptSteps {
    fn execute(&self, input: SP1Stdin) -> Result<(SP1PublicValues, ExecutionReport)>;
    fn prove(&self, input: SP1Stdin) -> Result<SP1ProofWithPublicValues>;
    fn verify(&self, proof: &SP1ProofWithPublicValues) -> Result<()>;
    fn extract_public_values<'a>(&self, proof: &'a SP1ProofWithPublicValues) -> &'a SP1PublicValues;
    fn post_verify(&self, proof: &SP1ProofWithPublicValues);
}

struct EvmScript {
    client: ProverClient,
    pk: SP1ProvingKey,
    vk: SP1VerifyingKey,
}

impl EvmScript {
    fn new() -> Self {
        let client: ProverClient = ProverClient::network();
        let (pk, vk) = client.setup(ELF);
        Self { client, pk, vk }
    }
}

impl ScriptSteps for EvmScript {
    fn execute(&self, input: SP1Stdin) -> Result<(SP1PublicValues, ExecutionReport)> {
        self.client.execute(ELF, input).run()
    }

    fn prove(&self, input: SP1Stdin) -> Result<SP1ProofWithPublicValues> {
        self.client.prove(&self.pk, input).plonk().run()
    }

    fn verify(&self, proof: &SP1ProofWithPublicValues) -> Result<()> {
        self.client
            .verify(proof, &self.vk)
            .map_err(|err| anyhow!("Couldn't verify {:#?}", err))
    }

    fn extract_public_values<'a>(&self, proof: &'a SP1ProofWithPublicValues) -> &'a SP1PublicValues {
        &proof.public_values
    }

    fn post_verify(&self, proof: &SP1ProofWithPublicValues) {
        create_plonk_fixture(proof, &self.vk);
    }
}

struct LocalScript {
    client: ProverClient,
    pk: SP1ProvingKey,
    vk: SP1VerifyingKey,
}

impl LocalScript {
    fn new() -> Self {
        let client: ProverClient = ProverClient::local();
        let (pk, vk) = client.setup(ELF);
        Self { client, pk, vk }
    }
}

impl ScriptSteps for LocalScript {
    fn execute(&self, input: SP1Stdin) -> Result<(SP1PublicValues, ExecutionReport)> {
        self.client.execute(ELF, input).run()
    }

    fn prove(&self, input: SP1Stdin) -> Result<SP1ProofWithPublicValues> {
        self.client.prove(&self.pk, input).run()
    }

    fn verify(&self, proof: &SP1ProofWithPublicValues) -> Result<()> {
        self.client
            .verify(proof, &self.vk)
            .map_err(|err| anyhow!("Couldn't verify {:#?}", err))
    }

    fn extract_public_values<'a>(&self, proof: &'a SP1ProofWithPublicValues) -> &'a SP1PublicValues {
        &proof.public_values
    }

    fn post_verify(&self, _proof: &SP1ProofWithPublicValues) {}
}

fn run_script(
    steps: impl ScriptSteps,
    prove: bool,
    program_input: &ProgramInput,
    expected_public_values: &PublicValuesRust,
) {
    let mut stdin: SP1Stdin = SP1Stdin::new();
    stdin.write(&program_input);
    // log::info!("Rereading");
    // let reread = stdin.read::<ProgramInput>();
    // log::info!("Validators {:?}", reread.validators_and_balances.validators);

    let public_values: &SP1PublicValues;

    if prove {
        let proof = steps.prove(stdin).expect("failed to generate proof");
        log::info!("Successfully generated proof!");
        steps.verify(&proof).expect("failed to verify proof");
        log::info!("Successfully verified proof!");
        public_values = steps.extract_public_values(&proof);

        verify_public_values(&public_values, expected_public_values);

        steps.post_verify(&proof);
    } else {
        // Only execute the program and get a `SP1PublicValues` object.
        let (public_values, execution_report) = steps.execute(stdin).unwrap();

        // Print the total number of cycles executed and the full execution report with a breakdown of
        // the RISC-V opcode and syscall counts.
        log::info!(
            "Executed program with {} cycles",
            execution_report.total_instruction_count() + execution_report.total_syscall_count()
        );
        log::debug!("Full execution report:\n{}", execution_report);

        verify_public_values(&public_values, expected_public_values);
    }
}

fn verify_public_values(public_values: &SP1PublicValues, expected_public_values: &PublicValuesRust) {
    let public_values_solidity: PublicValuesSolidity =
        PublicValuesSolidity::abi_decode(public_values.as_slice(), true).expect("Failed to parse public values");
    let public_values_rust: PublicValuesRust = public_values_solidity.into();

    assert!(public_values_rust == *expected_public_values);
    log::debug!(
        "Expected hash: {}",
        hex::encode(public_values_rust.metadata.beacon_block_hash)
    );
    log::debug!(
        "Computed hash: {}",
        hex::encode(public_values_rust.metadata.beacon_block_hash)
    );

    log::info!("Public values match!");
}

#[tokio::main]
async fn main() {
    dotenv().ok();
    // Setup the logger.
    sp1_sdk::utils::setup_logger();

    // Parse the command line arguments.
    let args = ProveArgs::parse();

    println!("evm: {}", args.evm);

    let file_path = fs::canonicalize(args.path).expect("Couldn't canonicalize path");
    let bs_reader = FileBasedBeaconStateReader::for_slot(&file_path, args.slot);
    let bs = bs_reader
        .read_beacon_state(args.slot) // File reader ignores slot; TODO: refactor readers
        .await
        .expect("Failed to read beacon state");
    let bh = bs_reader
        .read_beacon_block_header(args.slot) // File reader ignores slot; TODO: refactor readers
        .await
        .expect("Failed to read beacon block header");

    assert_eq!(bs.slot, args.slot);
    assert_eq!(bh.slot, args.slot);

    let beacon_block_hash = bh.tree_hash_root();

    log::info!(
        "Processing BeaconState. Slot: {}, Block Hash: {}, Validator count:{}",
        bs.slot,
        hex::encode(beacon_block_hash),
        bs.validators.len()
    );

    let bs_with_precomputed: BeaconStatePrecomputedHashes = (&bs).into();
    let bh_with_precomputed: BeaconBlockHeaderPrecomputedHashes = (&bh).into();
    let bs_indices = bs
        .get_leafs_indices(["validators", "balances"])
        .expect("Failed to get BeaconState field indices");

    let validators_and_balances_proof: Vec<u8> = bs.get_serialized_multiproof(&bs_indices);

    let program_input = ProgramInput {
        slot: bs.slot,
        beacon_block_hash: beacon_block_hash.to_fixed_bytes(),
        // beacon_block_hash: h!("0000000000000000000000000000000000000000000000000000000000000000"),
        beacon_block_header: bh_with_precomputed,
        beacon_state: bs_with_precomputed,
        validators_and_balances_proof: validators_and_balances_proof,
        validators_and_balances: ValsAndBals {
            balances: bs.balances,
            validators: bs.validators,
        },
    };

    let epoch = bs.slot.checked_div(eth_spec::SlotsPerEpoch::to_u64()).unwrap();
    let lido_withdrawal_creds: Hash256 = sp1_lido_accounting_zk_shared::consts::LIDO_WITHDRAWAL_CREDENTIALS.into();

    let expected_report = ReportData::compute(
        bs.slot,
        epoch,
        &program_input.validators_and_balances.validators,
        &program_input.validators_and_balances.balances,
        &lido_withdrawal_creds,
    );

    let expected_public_values: PublicValuesRust = PublicValuesRust {
        report: ReportRust {
            slot: expected_report.slot,
            all_lido_validators: expected_report.all_lido_validators,
            exited_lido_validators: expected_report.exited_lido_validators,
            lido_cl_valance: expected_report.lido_cl_valance,
        },
        metadata: ReportMetadataRust {
            slot: expected_report.slot,
            epoch: expected_report.epoch,
            lido_withdrawal_credentials: expected_report.lido_withdrawal_credentials.into(),
            beacon_block_hash: beacon_block_hash.into(),
        },
    };

    if args.evm {
        run_script(EvmScript::new(), args.prove, &program_input, &expected_public_values)
    } else {
        run_script(LocalScript::new(), args.prove, &program_input, &expected_public_values)
    }
}

/// A fixture that can be used to test the verification of SP1 zkVM proofs inside Solidity.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ProofFixture {
    vkey: String,
    report: ReportRust,
    metadata: ReportMetadataRust,
    public_values: String,
    proof: String,
}

/// Create a fixture for the given proof.
fn create_plonk_fixture(proof: &SP1ProofWithPublicValues, vk: &SP1VerifyingKey) {
    let bytes = proof.public_values.as_slice();
    let public_values: PublicValuesSolidity = PublicValuesSolidity::abi_decode(bytes, false).unwrap();

    let fixture = ProofFixture {
        vkey: vk.bytes32().to_string(),
        report: public_values.report.into(),
        metadata: public_values.metadata.into(),
        public_values: format!("0x{}", hex::encode(bytes)),
        proof: format!("0x{}", hex::encode(proof.bytes())),
    };

    log::debug!("Verification Key: {}", fixture.vkey);
    log::debug!("Public Values: {}", fixture.public_values);
    log::debug!("Proof Bytes: {}", fixture.proof);

    // Save the fixture to a file.
    let fixture_path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../contracts/src/fixtures");
    std::fs::create_dir_all(&fixture_path).expect("failed to create fixture path");
    std::fs::write(
        fixture_path.join("fixture.json"),
        serde_json::to_string_pretty(&fixture).unwrap(),
    )
    .expect("failed to write fixture");
    log::info!("Successfully written test fixture");
}
