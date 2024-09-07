use alloy_sol_types::SolType;
use anyhow::anyhow;
use clap::Parser;
use hex;
use serde::{Deserialize, Serialize};
use sp1_lido_accounting_scripts::beacon_state_reader_enum::BeaconStateReaderEnum;
use sp1_lido_accounting_scripts::ELF;
use sp1_lido_accounting_zk_shared::circuit_logic;
use sp1_lido_accounting_zk_shared::consts::Network;
use sp1_sdk::{
    ExecutionReport, HashableKey, ProverClient, SP1ProofWithPublicValues, SP1ProvingKey, SP1PublicValues, SP1Stdin,
    SP1VerifyingKey,
};

use std::collections::HashSet;
use std::path::PathBuf;

use sp1_lido_accounting_zk_shared::beacon_state_reader::BeaconStateReader;
use sp1_lido_accounting_zk_shared::circuit_logic::input_verification::{InputVerifier, LogCycleTracker};
use sp1_lido_accounting_zk_shared::circuit_logic::report::ReportData;
use sp1_lido_accounting_zk_shared::eth_consensus_layer::{
    epoch, Balances, BeaconBlockHeader, BeaconState, Hash256, Slot, ValidatorIndex, Validators,
};
use sp1_lido_accounting_zk_shared::io::eth_io::{
    LidoValidatorStateRust, PublicValuesRust, PublicValuesSolidity, ReportMetadataRust, ReportRust,
};
use sp1_lido_accounting_zk_shared::io::program_io::{ProgramInput, ValsAndBals};
use sp1_lido_accounting_zk_shared::lido::{LidoValidatorState, ValidatorDelta, ValidatorWithIndex};
use sp1_lido_accounting_zk_shared::merkle_proof::{FieldProof, MerkleTreeFieldLeaves};
use sp1_lido_accounting_zk_shared::util::{u64_to_usize, usize_to_u64};

use anyhow::Result;
use log;

use std::env;

use tree_hash::TreeHash;

#[derive(Parser, Debug)]
#[clap(author, version, about, long_about = None)]
struct ProveArgs {
    #[clap(long, default_value = "false")]
    local_prover: bool,
    #[clap(long, default_value = "false")]
    verify_input: bool,
    #[clap(long, default_value = "false")]
    verify_public_values: bool,
    #[clap(long, default_value = "false")]
    verify_proof_locally: bool,
    #[clap(long, default_value = "false")]
    dry_run: bool,
    #[clap(long, default_value = "false")]
    prove: bool,
    #[clap(long, default_value = "2100000")]
    current_slot: u64,
    #[clap(long, default_value = "2000000")]
    previous_slot: u64,
    #[clap(long, default_value = "false")]
    print_vkey: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ProofFixture {
    vkey: String,
    report: ReportRust,
    metadata: ReportMetadataRust,
    public_values: String,
    proof: String,
}

struct ScriptConfig {
    verify_proof: bool,
    verify_public_values: bool,
}

struct ScriptSteps {
    client: ProverClient,
    pk: SP1ProvingKey,
    vk: SP1VerifyingKey,
    config: ScriptConfig,
}

impl ScriptSteps {
    pub fn new(client: ProverClient, config: ScriptConfig) -> Self {
        let (pk, vk) = client.setup(ELF);
        Self { client, pk, vk, config }
    }

    pub fn vkey(&self) -> &'_ SP1VerifyingKey {
        return &self.vk;
    }

    pub fn execute(&self, input: SP1Stdin) -> Result<(SP1PublicValues, ExecutionReport)> {
        self.client.execute(ELF, input).run()
    }

    pub fn prove(&self, input: SP1Stdin) -> Result<SP1ProofWithPublicValues> {
        self.client.prove(&self.pk, input).plonk().run()
    }

    pub fn verify_proof(&self, proof: &SP1ProofWithPublicValues) -> Result<()> {
        if !self.config.verify_proof {
            log::info!("Skipping verifying proof");
            return Ok(());
        }
        log::info!("Verifying proof");
        self.client
            .verify(proof, &self.vk)
            .map_err(|err| anyhow!("Couldn't verify {:#?}", err))
    }

    fn verify_public_values(
        &self,
        public_values: &SP1PublicValues,
        expected_public_values: &PublicValuesRust,
    ) -> Result<()> {
        if !self.config.verify_public_values {
            log::info!("Skipping verifying proof");
            return Ok(());
        }

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

        Ok(())
    }

    pub fn write_test_fixture(&self, proof: &SP1ProofWithPublicValues) {
        let bytes = proof.public_values.as_slice();
        let public_values: PublicValuesSolidity = PublicValuesSolidity::abi_decode(bytes, false).unwrap();

        let fixture = ProofFixture {
            vkey: self.vk.bytes32().to_string(),
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
        let fixture_file = fixture_path.join("fixture.json");
        std::fs::create_dir_all(&fixture_path).expect("failed to create fixture path");
        std::fs::write(fixture_file.clone(), serde_json::to_string_pretty(&fixture).unwrap())
            .expect("failed to write fixture");
        log::info!("Successfully written test fixture to {fixture_file:?}");
    }
}

fn write_sp1_stdin(program_input: &ProgramInput) -> SP1Stdin {
    log::info!("Writing program input to SP1Stdin");
    let mut stdin: SP1Stdin = SP1Stdin::new();
    stdin.write(&program_input);
    stdin
}

fn prove(steps: ScriptSteps, program_input: &ProgramInput, expected_public_values: &PublicValuesRust) {
    log::info!("Proving program");
    let stdin = write_sp1_stdin(program_input);

    let proof = steps.prove(stdin).expect("Failed to generate proof");
    log::info!("Generated proof");

    steps.verify_proof(&proof).expect("Failed to verify proof");
    log::info!("Verified proof");

    steps
        .verify_public_values(&proof.public_values, expected_public_values)
        .expect("Failed to verify public inputs");
    log::info!("Verified public values");

    steps.write_test_fixture(&proof);
}

fn execute(steps: ScriptSteps, program_input: &ProgramInput, expected_public_values: &PublicValuesRust) {
    log::info!("Executing program");
    let stdin = write_sp1_stdin(program_input);

    let (public_values, execution_report) = steps.execute(stdin).unwrap();

    log::info!(
        "Executed program with {} cycles",
        execution_report.total_instruction_count() + execution_report.total_syscall_count()
    );
    log::debug!("Full execution report:\n{}", execution_report);

    steps
        .verify_public_values(&public_values, expected_public_values)
        .expect("Failed to verify public inputs");
    log::info!("Successfully verified public values!");
}

fn compute_report_and_public_values(
    slot: Slot,
    old_validator_state: &LidoValidatorState,
    new_validator_state: &LidoValidatorState,
    validators: &Validators,
    balances: &Balances,
    beacon_block_hash: &Hash256,
    lido_withdrawal_credentials: &Hash256,
) -> (ReportData, PublicValuesRust) {
    let report = ReportData::compute(
        slot,
        epoch(slot).unwrap(),
        &validators,
        &balances,
        &lido_withdrawal_credentials,
    );

    let public_values: PublicValuesRust = PublicValuesRust {
        report: ReportRust {
            slot: report.slot,
            deposited_lido_validators: report.deposited_lido_validators,
            exited_lido_validators: report.exited_lido_validators,
            lido_cl_valance: report.lido_cl_balance,
        },
        metadata: ReportMetadataRust {
            slot: report.slot,
            epoch: report.epoch,
            lido_withdrawal_credentials: lido_withdrawal_credentials.to_fixed_bytes(),
            beacon_block_hash: beacon_block_hash.to_fixed_bytes(),
            state_for_previous_report: LidoValidatorStateRust {
                slot: old_validator_state.slot,
                merkle_root: old_validator_state.tree_hash_root().to_fixed_bytes(),
            },
            new_state: LidoValidatorStateRust {
                slot: new_validator_state.slot,
                merkle_root: new_validator_state.tree_hash_root().to_fixed_bytes(),
            },
        },
    };

    return (report, public_values);
}

async fn read_beacon_states(
    bs_reader: impl BeaconStateReader,
    target_slot: u64,
    previous_slot: u64,
) -> (BeaconState, BeaconBlockHeader, BeaconState) {
    let bs = bs_reader
        .read_beacon_state(target_slot)
        .await
        .expect("Failed to read beacon state");
    let bh = bs_reader
        .read_beacon_block_header(target_slot)
        .await
        .expect("Failed to read beacon block header");

    let old_bs = bs_reader
        .read_beacon_state(previous_slot)
        .await
        .expect("Failed to read previous beacon state");

    assert_eq!(bs.slot, target_slot);
    assert_eq!(bh.slot, target_slot);
    assert_eq!(old_bs.slot, previous_slot);

    return (bs, bh, old_bs);
}

struct ValidatorDeltaCompute<'a> {
    old_bs: &'a BeaconState,
    old_state: &'a LidoValidatorState,
    new_bs: &'a BeaconState,
}

impl<'a> ValidatorDeltaCompute<'a> {
    pub fn new(old_bs: &'a BeaconState, old_state: &'a LidoValidatorState, new_bs: &'a BeaconState) -> Self {
        Self {
            old_bs,
            old_state,
            new_bs,
        }
    }

    fn compute_changed(&self) -> HashSet<ValidatorIndex> {
        let mut lido_changed_indices: HashSet<ValidatorIndex> = self
            .old_state
            .pending_deposit_lido_validator_indices
            .iter()
            .map(|v: &u64| v.clone())
            .collect();

        // ballpark estimating ~32000 validators changed per oracle report should waaaay more than enough
        // Better estimate could be (new_slot - old_slot) * avg_changes_per_slot, but the impact is likely marginal
        // If underestimated, the vec will transparently resize and reallocate more memory, so the only
        // effect is slightly slower run time - which is ok, unless (again) this gets into shared and used in the ZK part
        lido_changed_indices.reserve(32000);

        for index in &self.old_state.deposited_lido_validator_indices {
            // for already deposited validators, we want to check if something material have changed:
            // this can only be activation epoch or exist epoch. Theoretically "slashed" can also be
            // relevant, but for now we have no use for it
            let index_usize = u64_to_usize(*index);
            let old_validator = &self.old_bs.validators[index_usize];
            let new_validator = &self.new_bs.validators[index_usize];

            assert!(
                old_validator.pubkey == new_validator.pubkey,
                "Validators at index {} in old and new beacon state have different pubkeys",
                index
            );
            if (old_validator.exit_epoch != new_validator.exit_epoch)
                || (old_validator.activation_epoch != new_validator.activation_epoch)
            {
                lido_changed_indices.insert(index.clone());
            }
        }

        lido_changed_indices
    }

    fn read_validators(&self, indices: Vec<ValidatorIndex>) -> Vec<ValidatorWithIndex> {
        indices
            .iter()
            .filter_map(|index| {
                self.new_bs
                    .validators
                    .get(u64_to_usize(*index))
                    .map(|v| ValidatorWithIndex {
                        index: index.clone(),
                        validator: v.clone(),
                    })
            })
            .collect()
    }

    pub fn compute(&self) -> ValidatorDelta {
        log::debug!(
            "Validator count: old {}, new {}",
            self.old_bs.validators.len(),
            self.new_bs.validators.len()
        );

        let added_count = self.new_bs.validators.len() - self.old_bs.validators.len();
        let added = self.old_state.indices_for_adjacent_delta(added_count).collect();
        let changed: Vec<u64> = self.compute_changed().into_iter().collect();

        ValidatorDelta {
            all_added: self.read_validators(added),
            lido_changed: self.read_validators(changed),
        }
    }
}

fn read_network() -> Network {
    let chain = env::var("EVM_CHAIN").expect("Couldn't read EVM_CHAIN env var");
    Network::from_str(&chain).unwrap()
}

#[tokio::main]
async fn main() {
    sp1_sdk::utils::setup_logger();
    let args = ProveArgs::parse();
    log::debug!("Args: {:?}", args);

    let network = read_network();
    let network_config = network.get_config();
    log::info!(
        "Running for network {:?}, slot: {}, previous_slot: {}",
        network,
        args.current_slot,
        args.previous_slot
    );

    let bs_reader = BeaconStateReaderEnum::new_from_env();

    let lido_withdrawal_credentials: Hash256 = network_config.lido_withdrawal_credentials.into();
    let (bs, bh, old_bs) = read_beacon_states(bs_reader, args.current_slot, args.previous_slot).await;
    let beacon_block_hash = bh.tree_hash_root();

    log::info!(
        "Processing BeaconState. Current slot: {}, Previous Slot: {}, Block Hash: {}, Validator count:{}",
        bs.slot,
        old_bs.slot,
        hex::encode(beacon_block_hash),
        bs.validators.len()
    );
    let old_lido_validator_state = LidoValidatorState::compute_from_beacon_state(&old_bs, &lido_withdrawal_credentials);
    let new_lido_validator_state = LidoValidatorState::compute_from_beacon_state(&bs, &lido_withdrawal_credentials);

    log::info!(
        "Computed validator states. Old: deposited {}, pending {}, exited {}. New: deposited {}, pending {}, exited {}",
        old_lido_validator_state.deposited_lido_validator_indices.len(),
        old_lido_validator_state.pending_deposit_lido_validator_indices.len(),
        old_lido_validator_state.exited_lido_validator_indices.len(),
        new_lido_validator_state.deposited_lido_validator_indices.len(),
        new_lido_validator_state.pending_deposit_lido_validator_indices.len(),
        new_lido_validator_state.exited_lido_validator_indices.len(),
    );

    let (report, public_values) = compute_report_and_public_values(
        // TODO: could've just passed bs, but bs.balances and bs.validators are moved into program_input
        bs.slot,
        &old_lido_validator_state,
        &new_lido_validator_state,
        &bs.validators,
        &bs.balances,
        &beacon_block_hash,
        &lido_withdrawal_credentials,
    );

    log::info!("Computed report and public values");
    log::debug!("Report {report:?}");
    log::debug!("Public values {public_values:?}");

    let delta_compute = ValidatorDeltaCompute::new(&old_bs, &old_lido_validator_state, &bs);
    let validator_delta = delta_compute.compute();
    log::info!(
        "Computed validator delta. Added: {}, lido changed: {}",
        validator_delta.all_added.len(),
        validator_delta.lido_changed.len(),
    );
    let added_indices: Vec<usize> = validator_delta.added_indices().map(|v| u64_to_usize(*v)).collect();
    let changed_indices: Vec<usize> = validator_delta
        .lido_changed_indices()
        .map(|v| u64_to_usize(*v))
        .collect();

    let added_validators_proof = bs.validators.get_serialized_multiproof(added_indices.as_slice());
    let changed_validators_proof = bs.validators.get_serialized_multiproof(changed_indices.as_slice());
    log::info!("Obtained added and changed validators multiproofs");

    let bs_indices = bs
        .get_leafs_indices(["validators", "balances"])
        .expect("Failed to get BeaconState field indices");
    let validators_and_balances_proof = bs.get_serialized_multiproof(bs_indices.as_slice());
    log::info!("Obtained validators and balances fields multiproof");

    log::info!("Creating program input");
    let program_input = ProgramInput {
        slot: bs.slot,
        beacon_block_hash,
        // beacon_block_hash: h!("0000000000000000000000000000000000000000000000000000000000000000"),
        beacon_block_header: (&bh).into(),
        beacon_state: (&bs).into(),
        validators_and_balances: ValsAndBals {
            validators_and_balances_proof: validators_and_balances_proof,

            lido_withdrawal_credentials,
            total_validators: usize_to_u64(bs.validators.len()),
            validators_delta: validator_delta,
            added_validators_inclusion_proof: added_validators_proof,
            changed_validators_inclusion_proof: changed_validators_proof,

            balances: bs.balances,
        },
        old_lido_validator_state: old_lido_validator_state.clone(),
        new_lido_validator_state_hash: new_lido_validator_state.tree_hash_root(),
    };

    if args.verify_input {
        log::debug!("Verifying inputs");
        let cycle_tracker = LogCycleTracker {};
        let input_verifier = InputVerifier::new(&cycle_tracker);
        input_verifier.prove_input(&program_input);
        log::debug!("Inputs verified");

        log::debug!("Verifying old_state + validator_delta = new_state");
        let delta = &program_input.validators_and_balances.validators_delta;
        let new_state = old_lido_validator_state.merge_validator_delta(bs.slot, delta, &lido_withdrawal_credentials);
        assert_eq!(new_state, new_lido_validator_state);
        assert_eq!(new_state.tree_hash_root(), program_input.new_lido_validator_state_hash);
        log::debug!("New state verified");

        log::debug!("Verifying public values construction");
        let public_values_from_circuit = circuit_logic::io::create_public_values(
            &report,
            &beacon_block_hash,
            old_lido_validator_state.slot,
            &old_lido_validator_state.tree_hash_root(),
            new_state.slot,
            &new_state.tree_hash_root(),
        );
        assert_eq!(public_values, public_values_from_circuit.into());
        log::debug!("Public values verified");
    }

    let prover_client = if args.local_prover {
        ProverClient::local()
    } else {
        ProverClient::network()
    };
    let script_config = ScriptConfig {
        verify_proof: args.verify_proof_locally,
        verify_public_values: args.verify_public_values,
    };
    let script_steps = ScriptSteps::new(prover_client, script_config);

    if args.print_vkey {
        log::info!("Verification key {}", script_steps.vkey().bytes32());
    }

    if args.dry_run {
        return;
    }

    if args.prove {
        prove(script_steps, &program_input, &public_values);
    } else {
        execute(script_steps, &program_input, &public_values);
    }
}
