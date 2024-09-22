use alloy::primitives::{Address, U256};
use alloy_sol_types::SolType;
use anyhow::anyhow;
use clap::Parser;
use sp1_core_machine::io::SP1PublicValues; // TODO: remove when Sp1PublicValues are exported from sp1_sdk
use sp1_lido_accounting_scripts::beacon_state_reader::{BeaconStateReader, BeaconStateReaderEnum};
use sp1_lido_accounting_scripts::consts::Network;
use sp1_lido_accounting_scripts::eth_client::{ProviderFactory, Sp1LidoAccountingReportContract};
use sp1_lido_accounting_scripts::validator_delta::ValidatorDeltaCompute;
use sp1_sdk::{ProverClient, SP1ProofWithPublicValues, SP1ProvingKey, SP1Stdin, SP1VerifyingKey};

use sp1_lido_accounting_zk_shared::circuit_logic::input_verification::{InputVerifier, LogCycleTracker};
use sp1_lido_accounting_zk_shared::circuit_logic::report::ReportData;
use sp1_lido_accounting_zk_shared::eth_consensus_layer::{epoch, BeaconBlockHeader, BeaconState, Hash256, Slot};
use sp1_lido_accounting_zk_shared::io::eth_io::{
    LidoValidatorStateRust, PublicValuesRust, PublicValuesSolidity, ReportMetadataRust, ReportRust,
};
use sp1_lido_accounting_zk_shared::io::program_io::{ProgramInput, ValsAndBals};
use sp1_lido_accounting_zk_shared::lido::LidoValidatorState;
use sp1_lido_accounting_zk_shared::merkle_proof::{FieldProof, MerkleTreeFieldLeaves};
use sp1_lido_accounting_zk_shared::util::{u64_to_usize, usize_to_u64};

use anyhow::Result;
use log;

use std::env;
use std::path::PathBuf;

use tree_hash::TreeHash;

#[derive(Parser, Debug)]
#[clap(author, version, about, long_about = None)]
struct ProveArgs {
    #[clap(long, default_value = "5800000")]
    target_slot: u64,
    #[clap(long, required = false)]
    previous_slot: Option<u64>,
    #[clap(long, required = false)]
    store: bool,
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
        let (pk, vk) = client.setup(sp1_lido_accounting_scripts::ELF);
        Self { client, pk, vk, config }
    }

    pub fn vk(&self) -> &'_ SP1VerifyingKey {
        &self.vk
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
}

fn write_sp1_stdin(program_input: &ProgramInput) -> SP1Stdin {
    log::info!("Writing program input to SP1Stdin");
    let mut stdin: SP1Stdin = SP1Stdin::new();
    stdin.write(&program_input);
    stdin
}

fn verify_input_correctness(
    slot: Slot,
    program_input: &ProgramInput,
    old_state: &LidoValidatorState,
    new_state: &LidoValidatorState,
    lido_withdrawal_credentials: &Hash256,
) -> Result<()> {
    log::debug!("Verifying inputs");
    let cycle_tracker = LogCycleTracker {};
    let input_verifier = InputVerifier::new(&cycle_tracker);
    input_verifier.prove_input(program_input);
    log::debug!("Inputs verified");

    log::debug!("Verifying old_state + validator_delta = new_state");
    let delta = &program_input.validators_and_balances.validators_delta;
    let computed_new_state = old_state.merge_validator_delta(slot, delta, lido_withdrawal_credentials);
    assert_eq!(computed_new_state, *new_state);
    assert_eq!(
        computed_new_state.tree_hash_root(),
        program_input.new_lido_validator_state_hash
    );
    log::debug!("New state verified");
    Ok(())
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

    (bs, bh, old_bs)
}

fn prepare_program_input(
    bs: &BeaconState,
    bh: &BeaconBlockHeader,
    old_bs: &BeaconState,
    lido_withdrawal_credentials: &Hash256,
) -> (ProgramInput, PublicValuesRust) {
    let beacon_block_hash = bh.tree_hash_root();

    log::info!(
        "Processing BeaconState. Current slot: {}, Previous Slot: {}, Block Hash: {}, Validator count:{}",
        bs.slot,
        old_bs.slot,
        hex::encode(beacon_block_hash),
        bs.validators.len()
    );
    let old_validator_state = LidoValidatorState::compute_from_beacon_state(old_bs, lido_withdrawal_credentials);
    let new_validator_state = LidoValidatorState::compute_from_beacon_state(bs, lido_withdrawal_credentials);

    log::info!(
        "Computed validator states. Old: deposited {}, pending {}, exited {}. New: deposited {}, pending {}, exited {}",
        old_validator_state.deposited_lido_validator_indices.len(),
        old_validator_state.pending_deposit_lido_validator_indices.len(),
        old_validator_state.exited_lido_validator_indices.len(),
        new_validator_state.deposited_lido_validator_indices.len(),
        new_validator_state.pending_deposit_lido_validator_indices.len(),
        new_validator_state.exited_lido_validator_indices.len(),
    );

    let report = ReportData::compute(
        bs.slot,
        epoch(bs.slot).unwrap(),
        &bs.validators,
        &bs.balances,
        lido_withdrawal_credentials,
    );

    let public_values: PublicValuesRust = PublicValuesRust {
        report: ReportRust {
            slot: report.slot,
            deposited_lido_validators: report.deposited_lido_validators,
            exited_lido_validators: report.exited_lido_validators,
            lido_cl_balance: report.lido_cl_balance,
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

    log::info!("Computed report and public values");
    log::debug!("Report {report:?}");
    log::debug!("Public values {public_values:?}");

    let validator_delta = ValidatorDeltaCompute::new(&old_bs, &old_validator_state, &bs).compute();
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
        beacon_block_header: bh.into(),
        beacon_state: bs.into(),
        validators_and_balances: ValsAndBals {
            validators_and_balances_proof,
            lido_withdrawal_credentials: *lido_withdrawal_credentials,
            total_validators: usize_to_u64(bs.validators.len()),
            validators_delta: validator_delta,
            added_validators_inclusion_proof: added_validators_proof,
            changed_validators_inclusion_proof: changed_validators_proof,
            balances: bs.balances.clone(),
        },
        old_lido_validator_state: old_validator_state.clone(),
        new_lido_validator_state_hash: new_validator_state.tree_hash_root(),
    };

    verify_input_correctness(
        bs.slot,
        &program_input,
        &old_validator_state,
        &new_validator_state,
        lido_withdrawal_credentials,
    )
    .expect("Failed to verify input correctness");

    (program_input, public_values)
}

#[tokio::main]
async fn main() {
    sp1_sdk::utils::setup_logger();
    let args = ProveArgs::parse();
    log::debug!("Args: {:?}", args);

    let chain = env::var("EVM_CHAIN").expect("Couldn't read EVM_CHAIN env var");
    let network = Network::from_str(&chain).unwrap();
    let network_config = network.get_config();

    let lido_withdrawal_credentials: Hash256 = network_config.lido_withdrawal_credentials.into();
    let bs_reader = BeaconStateReaderEnum::new_from_env(&network);
    let provider = ProviderFactory::create_from_env().expect("Failed to create HTTP provider");
    let address: Address = env::var("CONTRACT_ADDRESS")
        .expect("Failed to read CONTRACT_ADDRESS env var")
        .parse()
        .expect("Failed to parse CONTRACT_ADDRESS into URL");
    let contract = Sp1LidoAccountingReportContract::new(address, provider);

    let previous_slot = if let Some(prev) = args.previous_slot {
        prev
    } else {
        let latest_report_response = contract
            .getLatestLidoValidatorStateSlot()
            .call()
            .await
            .expect("Failed to read latest report slot from contract");
        let latest_report_slot = latest_report_response._0;
        latest_report_slot.to::<u64>()
    };

    log::info!(
        "Submitting report for network {:?}, slot: {}, previous_slot: {}",
        network,
        args.target_slot,
        previous_slot,
    );

    let (bs, bh, old_bs) = read_beacon_states(bs_reader, args.target_slot, previous_slot).await;
    let (program_input, public_values) = prepare_program_input(&bs, &bh, &old_bs, &lido_withdrawal_credentials);

    let prover_client = ProverClient::network();
    let script_config = ScriptConfig {
        verify_proof: false,
        verify_public_values: true,
    };
    let steps = ScriptSteps::new(prover_client, script_config);

    log::info!("Proving program");
    let stdin = write_sp1_stdin(&program_input);

    let proof = steps.prove(stdin).expect("Failed to generate proof");
    log::info!("Generated proof");

    steps.verify_proof(&proof).expect("Failed to verify proof");
    log::info!("Verified proof");

    steps
        .verify_public_values(&proof.public_values, &public_values)
        .expect("Failed to verify public inputs");
    log::info!("Verified public values");

    if args.store {
        let file_name = format!("proof_{}_{}.json", network.as_str(), args.target_slot);
        let proof_file = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../temp/proofs")
            .join(file_name);
        sp1_lido_accounting_scripts::store_proof_and_metadata(&proof, steps.vk(), proof_file.as_path());
    }

    log::info!("Sending report");
    let tx_builder = contract.submitReportData(
        U256::from(bs.slot),
        public_values.report.into(),
        public_values.metadata.into(),
        proof.bytes().into(),
        proof.public_values.to_vec().into(),
    );
    let tx_call = tx_builder.send().await;

    if let Err(alloy::contract::Error::TransportError(alloy::transports::RpcError::ErrorResp(error_payload))) = tx_call
    {
        if let Some(revert_bytes) = error_payload.as_revert_data() {
            let err = sp1_lido_accounting_scripts::eth_client::Error::parse_rejection(revert_bytes.to_vec());
            panic!("Failed to submit report {:#?}", err);
        } else {
            panic!("Error payload {:#?}", error_payload);
        }
    } else if let Ok(tx) = tx_call {
        log::info!("Waiting for report transaction");
        let tx_result = tx.watch().await.expect("Failed to wait for confirmation");
        log::info!("Report transaction complete {}", hex::encode(tx_result.0));
    }
}
