mod test_utils;
use std::collections::HashSet;

use alloy_primitives::Address;

use anyhow::{anyhow, Result};

use rand::seq::IteratorRandom;
use sp1_lido_accounting_scripts::scripts::shared::{self as shared_logic, compute_validators_and_balances_test_public};
use sp1_lido_accounting_scripts::{
    beacon_state_reader::StateId,
    eth_client::{self, Sp1LidoAccountingReportContract::Sp1LidoAccountingReportContractErrors},
    sp1_client_wrapper::SP1ClientWrapper,
};

use sp1_lido_accounting_zk_shared::eth_consensus_layer::BeaconStateFields;
use sp1_lido_accounting_zk_shared::io::eth_io::{HaveEpoch, ReferenceSlot};
use sp1_lido_accounting_zk_shared::io::program_io::ExecutionPayloadHeaderData;
use sp1_lido_accounting_zk_shared::lido::{LidoValidatorState, ValidatorWithIndex};
use sp1_lido_accounting_zk_shared::util::usize_to_u64;
use sp1_lido_accounting_zk_shared::{
    eth_consensus_layer::{BeaconState, Hash256},
    eth_spec,
    io::{eth_io::BeaconChainSlot, program_io::ProgramInput},
};

use test_utils::set_bs_field;
use tree_hash::TreeHash;
use typenum::Unsigned;

use test_utils::{env::IntegrationTestEnvironment, make_validator, mark_as_refslot, varlists, vecs};

mod test_consts {
    use hex_literal::hex;
    pub const ANY_RANDOM_ADDRESS: [u8; 20] = hex!("042d31DE3feE857326efa774cbf29d37f487DF6c");
    // SOme other credentials on sepolia with 100 validators
    pub const NON_LIDO_CREDENTIALS: [u8; 32] = hex!("01000000000000000000000025c4a76e7d118705e7ea2e9b7d8c59930d8acd3b");
    pub const NON_LIDO_VALIDATOR_INDEX: u64 = 1;

    pub const MISSING_BLOCK_HASH: [u8; 32] = hex!("0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef");
    pub const BLOCK_HASH_6811538: [u8; 32] = hex!("d7e211537258f4c3c7b79724808ae5d0ad0c25ab4b082685a70844925554aad2");
}

#[derive(Debug)]
enum TestError {
    ContractRejected(Sp1LidoAccountingReportContractErrors),
    OtherRejection(eth_client::Error),
    ProofFailed(anyhow::Error),
    Other(anyhow::Error),
}

impl From<anyhow::Error> for TestError {
    fn from(value: anyhow::Error) -> Self {
        TestError::Other(value)
    }
}

impl From<eth_client::Error> for TestError {
    fn from(value: eth_client::Error) -> Self {
        match value {
            eth_client::Error::Rejection(e) => TestError::ContractRejected(e),
            other => TestError::OtherRejection(other),
        }
    }
}

struct TestExecutor {
    pub env: IntegrationTestEnvironment,
}

impl TestExecutor {
    async fn default() -> anyhow::Result<Self> {
        Self::new(test_utils::DEPLOY_SLOT).await
    }

    async fn new(deploy_slot: BeaconChainSlot) -> anyhow::Result<Self> {
        sp1_sdk::utils::setup_logger();
        let env = IntegrationTestEnvironment::new(test_utils::NETWORK.clone(), deploy_slot).await?;

        let instance = Self { env };

        Ok(instance)
    }

    pub async fn prepare_actual_input(&self, target_slot: BeaconChainSlot) -> anyhow::Result<ProgramInput> {
        let lido_withdrawal_credentials: Hash256 = self.env.network_config().lido_withdrawal_credentials.into();

        let reference_slot = mark_as_refslot(target_slot);
        let previous_slot = self.get_old_slot().await?;

        let target_bh = self.env.read_beacon_block_header(&StateId::Slot(target_slot)).await?;
        let target_bs = self.get_beacon_state(target_slot).await?;
        let old_bs = self.get_beacon_state(previous_slot).await?;

        let withdrawal_vault_data = self.env.get_balance_proof(&StateId::Slot(target_slot)).await?;

        tracing::info!("Preparing program input");
        let (program_input, _public_values) = shared_logic::prepare_program_input(
            reference_slot,
            &target_bs,
            &target_bh,
            &old_bs,
            &lido_withdrawal_credentials,
            withdrawal_vault_data,
            false,
        );
        Ok(program_input)
    }

    pub async fn get_old_slot(&self) -> anyhow::Result<BeaconChainSlot> {
        let res = self.env.contract.get_latest_validator_state_slot().await?;
        Ok(res)
    }

    pub async fn get_beacon_state(&self, slot: BeaconChainSlot) -> anyhow::Result<BeaconState> {
        let res = self.env.read_beacon_state(&StateId::Slot(slot)).await?;
        Ok(res)
    }

    pub async fn assert_fails_in_prover(&self, program_input: ProgramInput) -> anyhow::Result<()> {
        let result = self.env.sp1_client.execute(program_input);
        match result {
            Err(e) => {
                tracing::info!("Failed to create proof - as expected: {:?}", e);
                Ok(())
            }
            Ok(_) => Err(anyhow!("Executing proof succeeded")),
        }
    }

    pub async fn run(&self, program_input: ProgramInput) -> core::result::Result<(), TestError> {
        tracing::info!("Requesting proof");
        let try_proof = self.env.sp1_client.prove(program_input);

        if let Err(e) = try_proof {
            return Err(TestError::ProofFailed(e));
        }

        tracing::info!("Generated proof");
        let proof = try_proof.unwrap();

        tracing::info!("Sending report");
        let result = self
            .env
            .contract
            .submit_report_data(proof.bytes(), proof.public_values.to_vec())
            .await?;
        Ok(())
    }

    pub fn assert_failed_proof(&self, result: Result<(), TestError>) -> anyhow::Result<()> {
        match result {
            Err(TestError::ProofFailed(e)) => {
                tracing::info!("Failed to create proof - as expected: {:?}", e);
                Ok(())
            }
            Err(other_error) => Err(anyhow!("Other error {:#?}", other_error)),
            Ok(_) => Err(anyhow!("Report accepted")),
        }
    }

    pub fn assert_rejected(&self, result: Result<(), TestError>) -> anyhow::Result<()> {
        match result {
            Err(TestError::ContractRejected(err)) => {
                tracing::info!("As expected, contract rejected {:#?}", err);
                Ok(())
            }
            Err(other_error) => Err(anyhow!("Other error {:#?}", other_error)),
            Ok(_txhash) => Err(anyhow!("Report accepted")),
        }
    }

    pub fn assert_accepted(&self, result: Result<(), TestError>) -> anyhow::Result<()> {
        match result {
            Err(other_error) => Err(anyhow!("Error {:#?}", other_error)),
            Ok(_) => Ok(()),
        }
    }
}

/*
Test scenarios:
# Overall
* Legit data for a different slot - contract rejects
* "Partially old" scenarios (pass correct data for everything, except listed):
** WithdrawalVaultData + ExecutionPayloadHeaderData - prover crash
** Outdated old state + legit delta - contract rejects
* Correct old state, different lido withdrawal credentials for delta, new = old + delta - prover crash


# ProgramInput
* Refslot > bc_slot - contract rejects
* bc_slot empty - cannot reliably replicate (yet), covered by contract tests
* bc_slot in future - contract rejects
* beacon_block_hash - arbitrary - prover crash
* any adjustment to beacon_block_header - prover crash
* any adjustment to beacon_state - prover crash
* latest_execution_header_data - see ExecutionPayloadHeaderData section
* validators_and_balances - see ValsAndBals section
* old_lido_validator_state - see Old LidoValidatorState section
* new_lido_validator_state_hash:
** Old_state + delta != new state - prover crash
* withdrawal_vault_data - see WithdrawalVaultData section
*/

/* #region Self-check */
// * Sending a valid input with no tampering should be accepted

#[ignore = "Hits external prover (slow, incurs costs)"]
#[tokio::test(flavor = "multi_thread")]
async fn program_input_tampering_no_tampering_should_pass() -> Result<()> {
    let executor = TestExecutor::default().await?;
    let target_slot = executor.env.get_finalized_slot().await?;

    let program_input = executor
        .prepare_actual_input(target_slot)
        .await
        .expect("Test should be able to prepare actual input");
    let result = executor.run(program_input).await;
    executor.assert_accepted(result)
}

/* #endregion Self-check */

/* #region multi-modifications */
fn update_program_input(
    program_input: &mut ProgramInput,
    new_bs: BeaconState,
    old_bs: BeaconState,
    withdrawal_credentials: &Hash256,
    bs_modifier: impl Fn(BeaconState) -> BeaconState,
) {
    let bs = bs_modifier(new_bs);
    let old_validator_state = LidoValidatorState::compute_from_beacon_state(&old_bs, withdrawal_credentials);
    let new_validator_state = LidoValidatorState::compute_from_beacon_state(&bs, withdrawal_credentials);
    let modified_vals_and_bals =
        compute_validators_and_balances_test_public(&bs, &old_bs, &old_validator_state, withdrawal_credentials, false);

    program_input.validators_and_balances = modified_vals_and_bals;
    program_input.old_lido_validator_state = old_validator_state;
    program_input.new_lido_validator_state_hash = new_validator_state.tree_hash_root();
}

#[ignore = "Hits external prover (slow, incurs costs)"]
#[tokio::test(flavor = "multi_thread")]
async fn program_input_tampering_multi_wrong_withdrawal_credentials_with_recompute() -> Result<()> {
    let executor = TestExecutor::default().await?;
    let target_slot = executor.env.get_finalized_slot().await?;

    let mut program_input = executor.prepare_actual_input(target_slot).await?;

    let bs = executor.get_beacon_state(target_slot).await?;
    let old_bs = executor.get_beacon_state(executor.get_old_slot().await?).await?;

    let other_credentials: Hash256 = test_consts::NON_LIDO_CREDENTIALS.into();
    update_program_input(&mut program_input, bs, old_bs, &other_credentials, |bs| bs);

    let result = executor.run(program_input).await;
    executor.assert_rejected(result)
}

#[tokio::test(flavor = "multi_thread")]
async fn program_input_tampering_multi_vals_and_bals_added_balance_with_recompute() -> Result<()> {
    let executor = TestExecutor::default().await?;
    let target_slot = executor.env.get_finalized_slot().await?;

    let mut program_input = executor.prepare_actual_input(target_slot).await?;

    let bs = executor.get_beacon_state(target_slot).await?;
    let old_bs = executor.get_beacon_state(executor.get_old_slot().await?).await?;

    let lido_credentials: Hash256 = executor.env.network_config().lido_withdrawal_credentials.into();

    update_program_input(&mut program_input, bs, old_bs, &lido_credentials, |mut bs| {
        let balance = 32000000123;
        bs.validators.push(make_validator(bs.epoch(), balance)).expect("...");
        bs.balances.push(balance).expect("...");
        bs
    });
    executor.assert_fails_in_prover(program_input).await
}

#[tokio::test(flavor = "multi_thread")]
async fn program_input_tampering_multi_vals_and_bals_modified_balance_with_recompute() -> Result<()> {
    let executor = TestExecutor::default().await?;
    let target_slot = executor.env.get_finalized_slot().await?;

    let mut program_input = executor.prepare_actual_input(target_slot).await?;

    let bs = executor.get_beacon_state(target_slot).await?;
    let old_bs = executor.get_beacon_state(executor.get_old_slot().await?).await?;

    let lido_credentials: Hash256 = executor.env.network_config().lido_withdrawal_credentials.into();

    update_program_input(&mut program_input, bs, old_bs, &lido_credentials, |mut bs| {
        let lido_validators: Vec<usize> = bs
            .validators
            .iter()
            .enumerate()
            .filter(|(_idx, val)| val.withdrawal_credentials == lido_credentials)
            .map(|(idx, _val)| idx)
            .collect();
        let modify_idx = lido_validators.iter().choose(&mut rand::thread_rng()).expect("...");
        bs.balances[*modify_idx] += 250;
        bs
    });
    executor.assert_fails_in_prover(program_input).await
}

fn equal_in_any_order<T>(a: &[T], b: &[T]) -> bool
where
    T: Eq + std::hash::Hash,
{
    let a: HashSet<_> = a.iter().collect();
    let b: HashSet<_> = b.iter().collect();

    a == b
}

fn is_sorted<T: PartialOrd>(val: &[T]) -> bool {
    val.windows(2).all(|w| w[0] < w[1])
}

#[tokio::test(flavor = "multi_thread")]
async fn program_input_tampering_multi_shuffle_added_with_recompute() -> Result<()> {
    let executor = TestExecutor::default().await?;
    let target_slot = executor.env.get_finalized_slot().await?;

    let mut program_input = executor.prepare_actual_input(target_slot).await?;

    let old_state = &program_input.old_lido_validator_state;

    let delta = &program_input.validators_and_balances.validators_delta;

    let modified_all_added = vecs::ensured_shuffle_keep_first(&delta.all_added);

    let mut modified_deposited = old_state.deposited_lido_validator_indices.to_vec().clone();
    let mut modified_new_indices: Vec<u64> = modified_all_added.iter().map(|v| v.index.to_owned()).collect();
    modified_deposited.append(&mut modified_new_indices);

    let mut new_state = program_input.compute_new_state();
    // Self-checks - old and new deposited indices should have the same elements
    assert!(equal_in_any_order(
        &new_state.deposited_lido_validator_indices,
        &modified_deposited
    ));
    new_state.deposited_lido_validator_indices = modified_deposited.into();
    // ... but the new one should be out of order
    assert!(!is_sorted(&new_state.deposited_lido_validator_indices));

    program_input.validators_and_balances.validators_delta.all_added = modified_all_added;
    program_input.new_lido_validator_state_hash = new_state.tree_hash_root();

    executor.assert_fails_in_prover(program_input).await
}
/* #endregion multi-modifications*/

/* #region ProgramInput */
#[ignore = "Hits external prover (slow, incurs costs)"]
#[tokio::test(flavor = "multi_thread")]
async fn program_input_tampering_input_refslot_gt_bc_slot() -> Result<()> {
    let executor = TestExecutor::default().await?;
    let target_slot = executor.env.get_finalized_slot().await?;

    let mut program_input = executor.prepare_actual_input(target_slot).await?;

    program_input.reference_slot = ReferenceSlot(program_input.bc_slot.0 + 1);
    let result = executor.run(program_input).await;
    executor.assert_rejected(result)
}

#[tokio::test(flavor = "multi_thread")]
async fn program_input_tampering_input_bc_slot_in_future() -> Result<()> {
    let executor = TestExecutor::default().await?;
    let target_slot = executor.env.get_finalized_slot().await?;
    let latest_header = executor.env.read_beacon_block_header(&StateId::Head).await?;

    let mut program_input = executor.prepare_actual_input(target_slot).await?;

    program_input.bc_slot = BeaconChainSlot(latest_header.slot + 10);
    executor.assert_fails_in_prover(program_input).await
}

#[ignore = "Hits external prover (slow, incurs costs)"]
#[tokio::test(flavor = "multi_thread")]
async fn program_input_tampering_input_bc_slot_in_future_with_new_state_hash_update() -> Result<()> {
    let executor = TestExecutor::default().await?;
    let target_slot = executor.env.get_finalized_slot().await?;
    let latest_header = executor.env.read_beacon_block_header(&StateId::Head).await?;

    let mut program_input = executor.prepare_actual_input(target_slot).await?;

    let mut new_validator_state = program_input.compute_new_state();

    // self-check - to ensure the new_validator_state is computed correctly
    assert_eq!(
        new_validator_state.tree_hash_root(),
        program_input.new_lido_validator_state_hash
    );

    let new_slot = BeaconChainSlot(latest_header.slot + 10);

    program_input.bc_slot = new_slot;
    new_validator_state.slot = new_slot;
    new_validator_state.epoch = new_slot.epoch();

    // self-check - now the new_validator_state should be different
    assert_ne!(
        new_validator_state.tree_hash_root(),
        program_input.new_lido_validator_state_hash
    );

    program_input.new_lido_validator_state_hash = new_validator_state.tree_hash_root();
    let result = executor.run(program_input).await;
    executor.assert_rejected(result)
}

#[tokio::test(flavor = "multi_thread")]
async fn program_input_tampering_input_beacon_block_hash_modified_missing() -> Result<()> {
    let executor = TestExecutor::default().await?;
    let target_slot = executor.env.get_finalized_slot().await?;

    let mut program_input = executor.prepare_actual_input(target_slot).await?;

    program_input.beacon_block_hash = test_consts::MISSING_BLOCK_HASH.into();
    executor.assert_fails_in_prover(program_input).await
}

#[tokio::test(flavor = "multi_thread")]
async fn program_input_tampering_input_beacon_block_hash_modified_existing() -> Result<()> {
    let executor = TestExecutor::default().await?;
    let target_slot = executor.env.get_finalized_slot().await?;

    let mut program_input = executor.prepare_actual_input(target_slot).await?;

    program_input.beacon_block_hash = test_consts::BLOCK_HASH_6811538.into();
    executor.assert_fails_in_prover(program_input).await
}

#[tokio::test(flavor = "multi_thread")]
async fn program_input_tampering_input_beacon_state_hashes() -> Result<()> {
    let executor = TestExecutor::default().await?;
    let target_slot = executor.env.get_finalized_slot().await?;

    let program_input = executor.prepare_actual_input(target_slot).await?;

    for field in BeaconStateFields::all() {
        let mut new_input = program_input.clone();
        set_bs_field(&mut new_input.beacon_state, &field, Hash256::random());
        executor.assert_fails_in_prover(new_input).await?;
    }
    Ok(())
}

#[tokio::test(flavor = "multi_thread")]
async fn program_input_tampering_input_new_lido_validator_state_hash_random() -> Result<()> {
    let executor = TestExecutor::default().await?;
    let target_slot = executor.env.get_finalized_slot().await?;

    let mut program_input = executor.prepare_actual_input(target_slot).await?;

    program_input.new_lido_validator_state_hash = Hash256::random();
    executor.assert_fails_in_prover(program_input).await
}
/* #endregion ProgramInput */

/* #region ValsAndBals: */
// * lido_withdrawal_credentials different credentials - contract rejects
// * Manipulated balances - prover crash
// ** Added (+arbitrary validator)
// ** Deleted (-corresponding validator)
// ** Modified
// * total_validators - prover crash
// * validators_delta - prover crash
// ** all_added - added, removed, duplicated, shuffled
// ** lido_changed - added, removed, duplicated, shuffled

#[tokio::test(flavor = "multi_thread")]
async fn program_input_tampering_vals_and_bals_wrong_withdrawal_credentials_naive() -> Result<()> {
    let executor = TestExecutor::default().await?;
    let target_slot = executor.env.get_finalized_slot().await?;

    let mut program_input = executor.prepare_actual_input(target_slot).await?;

    program_input.validators_and_balances.lido_withdrawal_credentials = test_consts::NON_LIDO_CREDENTIALS.into();
    // Fails in creating proof since validators passed in the lido validator state won't pass is_lido check
    executor.assert_fails_in_prover(program_input).await
}

#[tokio::test(flavor = "multi_thread")]
async fn program_input_tampering_vals_and_bals_wrong_withdrawal_credentials() -> Result<()> {
    let executor = TestExecutor::default().await?;
    let target_slot = executor.env.get_finalized_slot().await?;

    let mut program_input = executor.prepare_actual_input(target_slot).await?;

    let modified_credentials: Hash256 = test_consts::NON_LIDO_CREDENTIALS.into();

    program_input.validators_and_balances.lido_withdrawal_credentials = modified_credentials;
    for val_with_index in &mut program_input.validators_and_balances.validators_delta.lido_changed {
        val_with_index.validator.withdrawal_credentials = modified_credentials;
    }
    // Fails because changed multiproof fails to verify
    executor.assert_fails_in_prover(program_input).await
}

#[ignore = "Hits external prover (slow, incurs costs)"]
#[tokio::test(flavor = "multi_thread")]
async fn program_input_tampering_vals_and_bals_wrong_withdrawal_credentials_empty_changed() -> Result<()> {
    let executor = TestExecutor::default().await?;
    let target_slot = executor.env.get_finalized_slot().await?;

    let mut program_input = executor.prepare_actual_input(target_slot).await?;

    let modified_credentials: Hash256 = test_consts::NON_LIDO_CREDENTIALS.into();

    program_input.validators_and_balances.lido_withdrawal_credentials = modified_credentials;
    program_input.validators_and_balances.validators_delta.lido_changed = vec![];
    program_input.new_lido_validator_state_hash = program_input.compute_new_state().tree_hash_root();
    let result = executor.run(program_input).await;
    // With these manipulations, it successfully generates the proof, but got rejected in the contract
    executor.assert_rejected(result)
}

#[tokio::test(flavor = "multi_thread")]
async fn program_input_tampering_vals_and_bals_added_balance() -> Result<()> {
    let executor = TestExecutor::default().await?;
    let target_slot = executor.env.get_finalized_slot().await?;

    let mut program_input = executor.prepare_actual_input(target_slot).await?;

    program_input.validators_and_balances.balances =
        varlists::append(program_input.validators_and_balances.balances, 123454321);
    executor.assert_fails_in_prover(program_input).await
}

#[tokio::test(flavor = "multi_thread")]
async fn program_input_tampering_vals_and_bals_removed_balance() -> Result<()> {
    let executor = TestExecutor::default().await?;
    let target_slot = executor.env.get_finalized_slot().await?;

    let mut program_input = executor.prepare_actual_input(target_slot).await?;

    program_input.validators_and_balances.balances =
        varlists::remove_random(program_input.validators_and_balances.balances);
    executor.assert_fails_in_prover(program_input).await
}

#[tokio::test(flavor = "multi_thread")]
async fn program_input_tampering_vals_and_bals_modified_balance() -> Result<()> {
    let executor = TestExecutor::default().await?;
    let target_slot = executor.env.get_finalized_slot().await?;

    let mut program_input = executor.prepare_actual_input(target_slot).await?;

    program_input.validators_and_balances.balances =
        varlists::modify_random(program_input.validators_and_balances.balances, |bal| bal + 250000);

    executor.assert_fails_in_prover(program_input).await
}

#[tokio::test(flavor = "multi_thread")]
async fn program_input_tampering_vals_and_bals_total_validators_higher() -> Result<()> {
    let executor = TestExecutor::default().await?;
    let target_slot = executor.env.get_finalized_slot().await?;

    let mut program_input = executor.prepare_actual_input(target_slot).await?;

    program_input.validators_and_balances.total_validators += 1;
    executor.assert_fails_in_prover(program_input).await
}

#[tokio::test(flavor = "multi_thread")]
async fn program_input_tampering_vals_and_bals_total_validators_lower() -> Result<()> {
    let executor = TestExecutor::default().await?;
    let target_slot = executor.env.get_finalized_slot().await?;

    let mut program_input = executor.prepare_actual_input(target_slot).await?;

    program_input.validators_and_balances.total_validators -= 1;
    executor.assert_fails_in_prover(program_input).await
}

#[tokio::test(flavor = "multi_thread")]
async fn program_input_tampering_vals_and_bals_delta_all_added_extra() -> Result<()> {
    let executor = TestExecutor::default().await?;
    let target_slot = executor.env.get_finalized_slot().await?;

    let mut program_input = executor.prepare_actual_input(target_slot).await?;

    program_input.validators_and_balances.validators_delta.all_added = vecs::append(
        program_input.validators_and_balances.validators_delta.all_added,
        ValidatorWithIndex {
            index: program_input.validators_and_balances.total_validators,
            validator: make_validator(target_slot.epoch(), 1234554321),
        },
    );

    executor.assert_fails_in_prover(program_input).await
}

#[tokio::test(flavor = "multi_thread")]
async fn program_input_tampering_vals_and_bals_delta_all_added_existing_as_added() -> Result<()> {
    let executor = TestExecutor::default().await?;
    let target_slot = executor.env.get_finalized_slot().await?;

    let mut program_input = executor.prepare_actual_input(target_slot).await?;
    let bs = executor.get_beacon_state(target_slot).await?;
    let target_validator_index = 67;
    let validator = bs.validators[target_validator_index].clone();

    program_input.validators_and_balances.validators_delta.all_added = vecs::append(
        program_input.validators_and_balances.validators_delta.all_added,
        ValidatorWithIndex {
            index: usize_to_u64(target_validator_index),
            validator,
        },
    );

    executor.assert_fails_in_prover(program_input).await
}

#[tokio::test(flavor = "multi_thread")]
async fn program_input_tampering_vals_and_bals_delta_all_added_duplicated() -> Result<()> {
    let executor = TestExecutor::default().await?;
    let target_slot = executor.env.get_finalized_slot().await?;

    let mut program_input = executor.prepare_actual_input(target_slot).await?;

    program_input.validators_and_balances.validators_delta.all_added =
        vecs::duplicate_random(program_input.validators_and_balances.validators_delta.all_added);
    executor.assert_fails_in_prover(program_input).await
}

#[tokio::test(flavor = "multi_thread")]
async fn program_input_tampering_vals_and_bals_delta_all_added_modified_creds() -> Result<()> {
    let executor = TestExecutor::default().await?;
    let target_slot = executor.env.get_finalized_slot().await?;

    let mut program_input = executor.prepare_actual_input(target_slot).await?;
    let lido_creds: Hash256 = executor.env.network_config().lido_withdrawal_credentials.into();

    program_input.validators_and_balances.validators_delta.all_added = vecs::modify_random(
        program_input.validators_and_balances.validators_delta.all_added,
        |val_index| {
            let mut copy = val_index.clone();
            if copy.validator.withdrawal_credentials == lido_creds {
                copy.validator.withdrawal_credentials = test_consts::NON_LIDO_CREDENTIALS.into();
            } else {
                copy.validator.withdrawal_credentials = lido_creds;
            }
            copy
        },
    );
    executor.assert_fails_in_prover(program_input).await
}

#[tokio::test(flavor = "multi_thread")]
async fn program_input_tampering_vals_and_bals_delta_all_added_modified_index() -> Result<()> {
    let executor = TestExecutor::default().await?;
    let target_slot = executor.env.get_finalized_slot().await?;

    let mut program_input = executor.prepare_actual_input(target_slot).await?;

    program_input.validators_and_balances.validators_delta.all_added = vecs::modify_random(
        program_input.validators_and_balances.validators_delta.all_added,
        |val_index| {
            let mut copy = val_index.clone();
            copy.index += 1;
            copy
        },
    );
    executor.assert_fails_in_prover(program_input).await
}

#[tokio::test(flavor = "multi_thread")]
async fn program_input_tampering_vals_and_bals_delta_all_added_modified_activaion_epoch_past() -> Result<()> {
    let executor = TestExecutor::default().await?;
    let target_slot = executor.env.get_finalized_slot().await?;
    let current_epoch = target_slot.epoch();

    let mut program_input = executor.prepare_actual_input(target_slot).await?;

    program_input.validators_and_balances.validators_delta.all_added = vecs::modify_random(
        program_input.validators_and_balances.validators_delta.all_added,
        |val_index| {
            let mut copy = val_index.clone();
            copy.validator.activation_epoch = current_epoch - 10;
            copy
        },
    );
    executor.assert_fails_in_prover(program_input).await
}

#[tokio::test(flavor = "multi_thread")]
async fn program_input_tampering_vals_and_bals_delta_all_added_modified_activaion_epoch_future() -> Result<()> {
    let executor = TestExecutor::default().await?;
    let target_slot = executor.env.get_finalized_slot().await?;
    let current_epoch = target_slot.epoch();

    let mut program_input = executor.prepare_actual_input(target_slot).await?;

    program_input.validators_and_balances.validators_delta.all_added = vecs::modify_random(
        program_input.validators_and_balances.validators_delta.all_added,
        |val_index| {
            let mut copy = val_index.clone();
            copy.validator.activation_epoch = current_epoch + 10;
            copy
        },
    );
    executor.assert_fails_in_prover(program_input).await
}

#[tokio::test(flavor = "multi_thread")]
async fn program_input_tampering_vals_and_bals_delta_all_added_removed() -> Result<()> {
    let executor = TestExecutor::default().await?;
    let target_slot = executor.env.get_finalized_slot().await?;

    let mut program_input = executor.prepare_actual_input(target_slot).await?;

    program_input.validators_and_balances.validators_delta.all_added =
        vecs::remove_random(program_input.validators_and_balances.validators_delta.all_added);
    executor.assert_fails_in_prover(program_input).await
}

#[tokio::test(flavor = "multi_thread")]
async fn program_input_tampering_vals_and_bals_delta_all_added_shuffled() -> Result<()> {
    let executor = TestExecutor::default().await?;
    let target_slot = executor.env.get_finalized_slot().await?;

    let mut program_input = executor.prepare_actual_input(target_slot).await?;

    program_input.validators_and_balances.validators_delta.all_added =
        vecs::ensured_shuffle_keep_first(&program_input.validators_and_balances.validators_delta.all_added);
    executor.assert_fails_in_prover(program_input).await
}

/** This is a "fixed point" test - particular shuffle order that reliably fail the previous test */
#[tokio::test(flavor = "multi_thread")]
async fn program_input_tampering_vals_and_bals_delta_all_added_shuffled_failing_case_1() -> Result<()> {
    let executor = TestExecutor::default().await?;
    let target_slot = executor.env.get_finalized_slot().await?;

    let mut program_input = executor.prepare_actual_input(target_slot).await?;
    let old_added = program_input.validators_and_balances.validators_delta.all_added;

    program_input.validators_and_balances.validators_delta.all_added =
        [0, 3, 4, 1, 2].iter().map(|idx| old_added[*idx].clone()).collect();
    executor.assert_fails_in_prover(program_input).await
}

/* #region ValidatorDelta.lido_changed */
#[tokio::test(flavor = "multi_thread")]
async fn program_input_tampering_vals_and_bals_delta_lido_changed_extra() -> Result<()> {
    let executor = TestExecutor::default().await?;
    let target_slot = executor.env.get_finalized_slot().await?;

    let mut program_input = executor.prepare_actual_input(target_slot).await?;
    let bs = executor.get_beacon_state(target_slot).await?;
    let target_validator_index = 50;
    let validator = bs.validators[target_validator_index].clone();

    program_input.validators_and_balances.validators_delta.lido_changed = vecs::append(
        program_input.validators_and_balances.validators_delta.lido_changed,
        ValidatorWithIndex {
            index: usize_to_u64(target_validator_index),
            validator,
        },
    );

    executor.assert_fails_in_prover(program_input).await
}

#[tokio::test(flavor = "multi_thread")]
async fn program_input_tampering_vals_and_bals_delta_lido_changed_modified_creds() -> Result<()> {
    let executor = TestExecutor::default().await?;
    let target_slot = executor.env.get_finalized_slot().await?;

    let mut program_input = executor.prepare_actual_input(target_slot).await?;
    let lido_creds: Hash256 = executor.env.network_config().lido_withdrawal_credentials.into();

    program_input.validators_and_balances.validators_delta.lido_changed = vecs::modify_random(
        program_input.validators_and_balances.validators_delta.lido_changed,
        |val_index| {
            let mut copy = val_index.clone();
            if copy.validator.withdrawal_credentials == lido_creds {
                copy.validator.withdrawal_credentials = test_consts::NON_LIDO_CREDENTIALS.into();
            } else {
                copy.validator.withdrawal_credentials = lido_creds;
            }
            copy
        },
    );
    executor.assert_fails_in_prover(program_input).await
}

#[tokio::test(flavor = "multi_thread")]
async fn program_input_tampering_vals_and_bals_delta_lido_changed_modified_index() -> Result<()> {
    let executor = TestExecutor::default().await?;
    let target_slot = executor.env.get_finalized_slot().await?;

    let mut program_input = executor.prepare_actual_input(target_slot).await?;

    program_input.validators_and_balances.validators_delta.lido_changed = vecs::modify_random(
        program_input.validators_and_balances.validators_delta.lido_changed,
        |val_index| {
            let mut copy = val_index.clone();
            copy.index += 1;
            copy
        },
    );
    executor.assert_fails_in_prover(program_input).await
}

#[tokio::test(flavor = "multi_thread")]
async fn program_input_tampering_vals_and_bals_delta_lido_changed_modified_activaion_epoch_past() -> Result<()> {
    let executor = TestExecutor::default().await?;
    let target_slot = executor.env.get_finalized_slot().await?;
    let current_epoch = target_slot.epoch();

    let mut program_input = executor.prepare_actual_input(target_slot).await?;

    program_input.validators_and_balances.validators_delta.lido_changed = vecs::modify_random(
        program_input.validators_and_balances.validators_delta.lido_changed,
        |val_index| {
            let mut copy = val_index.clone();
            copy.validator.activation_epoch = current_epoch - 10;
            copy
        },
    );
    executor.assert_fails_in_prover(program_input).await
}

#[tokio::test(flavor = "multi_thread")]
async fn program_input_tampering_vals_and_bals_delta_lido_changed_modified_activaion_epoch_future() -> Result<()> {
    let executor = TestExecutor::default().await?;
    let target_slot = executor.env.get_finalized_slot().await?;
    let current_epoch = target_slot.epoch();

    let mut program_input = executor.prepare_actual_input(target_slot).await?;

    program_input.validators_and_balances.validators_delta.lido_changed = vecs::modify_random(
        program_input.validators_and_balances.validators_delta.lido_changed,
        |val_index| {
            let mut copy = val_index.clone();
            copy.validator.activation_epoch = current_epoch + 10;
            copy
        },
    );
    executor.assert_fails_in_prover(program_input).await
}

#[tokio::test(flavor = "multi_thread")]
async fn program_input_tampering_vals_and_bals_delta_lido_changed_modified_exit_epoch_past() -> Result<()> {
    let executor = TestExecutor::default().await?;
    let target_slot = executor.env.get_finalized_slot().await?;
    let current_epoch = target_slot.epoch();

    let mut program_input = executor.prepare_actual_input(target_slot).await?;

    program_input.validators_and_balances.validators_delta.lido_changed = vecs::modify_random(
        program_input.validators_and_balances.validators_delta.lido_changed,
        |val_index| {
            let mut copy = val_index.clone();
            copy.validator.exit_epoch = current_epoch - 10;
            copy
        },
    );
    executor.assert_fails_in_prover(program_input).await
}

#[tokio::test(flavor = "multi_thread")]
async fn program_input_tampering_vals_and_bals_delta_lido_changed_modified_exit_epoch_future() -> Result<()> {
    let executor = TestExecutor::default().await?;
    let target_slot = executor.env.get_finalized_slot().await?;
    let current_epoch = target_slot.epoch();

    let mut program_input = executor.prepare_actual_input(target_slot).await?;

    program_input.validators_and_balances.validators_delta.lido_changed = vecs::modify_random(
        program_input.validators_and_balances.validators_delta.lido_changed,
        |val_index| {
            let mut copy = val_index.clone();
            copy.validator.exit_epoch = current_epoch + 10;
            copy
        },
    );
    executor.assert_fails_in_prover(program_input).await
}

#[tokio::test(flavor = "multi_thread")]
async fn program_input_tampering_vals_and_bals_delta_lido_changed_duplicated() -> Result<()> {
    let executor = TestExecutor::default().await?;
    let target_slot = executor.env.get_finalized_slot().await?;

    let mut program_input = executor.prepare_actual_input(target_slot).await?;

    program_input.validators_and_balances.validators_delta.lido_changed =
        vecs::duplicate_random(program_input.validators_and_balances.validators_delta.lido_changed);
    executor.assert_fails_in_prover(program_input).await
}

#[ignore = "Hits external prover (slow, incurs costs)"]
#[tokio::test(flavor = "multi_thread")]
async fn program_input_tampering_vals_and_bals_delta_lido_changed_emptied_old_has_pending_fails_STUB() -> Result<()> {
    // TODO: no old state with pending deposits exists yet.
    Ok(())
    // let executor = TestExecutor::default().await?;
    // let target_slot = executor.env.get_finalized_slot().await?;
    // let intermediate_slot = target_slot - eth_spec::SlotsPerEpoch::to_u64();

    // let mut program_input = executor.prepare_actual_input(intermediate_slot).await?;

    // program_input.validators_and_balances.validators_delta.lido_changed = vec![];

    // let result = executor.run(program_input).await;
    // executor.assert_accepted(result)?;

    // let program_input = executor.prepare_actual_input(target_slot).await?;
    // let result = executor.run(program_input).await;
    // executor.assert_accepted(result)
}

#[ignore = "Hits external prover (slow, incurs costs)"]
#[tokio::test(flavor = "multi_thread")]
async fn program_input_tampering_vals_and_bals_delta_lido_changed_emptied_old_no_pending_succeeds_subsequent_correct_succeeds(
) -> Result<()> {
    let executor = TestExecutor::default().await?;
    let target_slot = executor.env.get_finalized_slot().await?;
    let intermediate_slot = target_slot - eth_spec::SlotsPerEpoch::to_u64();

    let mut program_input = executor.prepare_actual_input(intermediate_slot).await?;

    program_input.validators_and_balances.validators_delta.lido_changed = vec![];

    let result = executor.run(program_input).await;
    executor.assert_accepted(result)?;

    let program_input = executor.prepare_actual_input(target_slot).await?;
    let result = executor.run(program_input).await;
    executor.assert_accepted(result)
}

#[tokio::test(flavor = "multi_thread")]
async fn program_input_tampering_vals_and_bals_delta_lido_changed_shuffled() -> Result<()> {
    let executor = TestExecutor::new(test_utils::DEPLOY_SLOT).await?;
    let target_slot = executor.env.get_finalized_slot().await?;

    let mut program_input = executor.prepare_actual_input(target_slot).await?;

    program_input.validators_and_balances.validators_delta.lido_changed =
        vecs::ensured_shuffle(&program_input.validators_and_balances.validators_delta.lido_changed);
    executor.assert_fails_in_prover(program_input).await
}
/* #region ValidatorDelta.lido_changed */

/* #endregion ValsAndBals */

/* #region Old LidoValidatorState */

// * Slot mismatch - contract rejects
// * Wrong Epoch - prover crash
// * max_validator_index:
// ** < actual validator count - prover crash
// ** > actual validator count - prover crash
// * deposited_lido_validator_indices - prover crash
// ** Remove one (or more) Lido
// ** Add arbitrary non-lido
// ** Duplicate one (or more)
// ** Shuffle
// * pending_deposit_lido_validator_indices - prover crash
// ** Remove one (or more) Lido
// ** Add arbitrary non-lido
// ** Duplicate one (or more)
// ** Shuffle
// * exited_lido_validator_indices - no enforcement;
// ** adding, removing, duplicating, shuffling should pass
// ** subsequent report without manipulation should succeed

#[ignore = "Hits external prover (slow, incurs costs)"]
#[tokio::test(flavor = "multi_thread")]
async fn program_input_tampering_old_state_mismatch_slot() -> Result<()> {
    let executor = TestExecutor::default().await?;
    let target_slot = executor.env.get_finalized_slot().await?;

    let mut program_input = executor.prepare_actual_input(target_slot).await?;

    program_input.old_lido_validator_state.slot -= eth_spec::SlotsPerEpoch::to_u64();
    program_input.old_lido_validator_state.epoch -= 1;
    let result = executor.run(program_input).await;
    executor.assert_rejected(result)
}

#[tokio::test(flavor = "multi_thread")]
async fn program_input_tampering_old_state_slot_epoch_diverge() -> Result<()> {
    let executor = TestExecutor::default().await?;
    let target_slot = executor.env.get_finalized_slot().await?;

    let mut program_input = executor.prepare_actual_input(target_slot).await?;

    program_input.old_lido_validator_state.slot -= eth_spec::SlotsPerEpoch::to_u64();
    executor.assert_fails_in_prover(program_input).await
}

#[tokio::test(flavor = "multi_thread")]
async fn program_input_tampering_old_state_max_validator_lower() -> Result<()> {
    let executor = TestExecutor::default().await?;
    let target_slot = executor.env.get_finalized_slot().await?;

    let mut program_input = executor.prepare_actual_input(target_slot).await?;

    program_input.old_lido_validator_state.max_validator_index -= 1;
    executor.assert_fails_in_prover(program_input).await
}

#[tokio::test(flavor = "multi_thread")]
async fn program_input_tampering_old_state_max_validator_higher() -> Result<()> {
    let executor = TestExecutor::default().await?;
    let target_slot = executor.env.get_finalized_slot().await?;

    let mut program_input = executor.prepare_actual_input(target_slot).await?;

    program_input.old_lido_validator_state.max_validator_index += 1;
    executor.assert_fails_in_prover(program_input).await
}

#[tokio::test(flavor = "multi_thread")]
async fn program_input_tampering_old_state_deposited_add_new() -> Result<()> {
    let executor = TestExecutor::default().await?;
    let target_slot = executor.env.get_finalized_slot().await?;

    let mut program_input = executor.prepare_actual_input(target_slot).await?;

    program_input
        .old_lido_validator_state
        .deposited_lido_validator_indices
        .push(test_consts::NON_LIDO_VALIDATOR_INDEX)
        .expect("Known to not exceed the capacity");

    executor.assert_fails_in_prover(program_input).await
}

#[tokio::test(flavor = "multi_thread")]
async fn program_input_tampering_old_state_deposited_duplicate() -> Result<()> {
    let executor = TestExecutor::default().await?;
    let target_slot = executor.env.get_finalized_slot().await?;

    let mut program_input = executor.prepare_actual_input(target_slot).await?;

    program_input.old_lido_validator_state.deposited_lido_validator_indices =
        varlists::duplicate_random(program_input.old_lido_validator_state.deposited_lido_validator_indices);

    executor.assert_fails_in_prover(program_input).await
}

#[tokio::test(flavor = "multi_thread")]
async fn program_input_tampering_old_state_deposited_remove_existing() -> Result<()> {
    let executor = TestExecutor::default().await?;
    let target_slot = executor.env.get_finalized_slot().await?;

    let mut program_input = executor.prepare_actual_input(target_slot).await?;

    program_input.old_lido_validator_state.deposited_lido_validator_indices =
        varlists::remove_random(program_input.old_lido_validator_state.deposited_lido_validator_indices);

    executor.assert_fails_in_prover(program_input).await
}

#[tokio::test(flavor = "multi_thread")]
async fn program_input_tampering_old_state_deposited_shuffle() -> Result<()> {
    // Using alt deploy slot since it has 5 validators
    let executor = TestExecutor::new(test_utils::DEPLOY_SLOT).await?;
    let target_slot = executor.env.get_finalized_slot().await?;

    let mut program_input = executor.prepare_actual_input(target_slot).await?;

    program_input.old_lido_validator_state.deposited_lido_validator_indices =
        varlists::ensured_shuffle(program_input.old_lido_validator_state.deposited_lido_validator_indices);
    executor.assert_fails_in_prover(program_input).await
}

/* TODO: add tests for pending - there are no pending validators in sepolia currently */

#[ignore = "Hits external prover (slow, incurs costs)"]
#[tokio::test(flavor = "multi_thread")]
async fn program_input_tampering_old_state_exited_add_new_accepted_subsequent_report_accepted() -> Result<()> {
    let executor = TestExecutor::default().await?;
    let target_slot = executor.env.get_finalized_slot().await?;
    let intermediate_slot = target_slot - eth_spec::SlotsPerEpoch::to_u64();

    let mut program_input = executor.prepare_actual_input(intermediate_slot).await?;

    program_input.old_lido_validator_state.exited_lido_validator_indices = varlists::append(
        program_input.old_lido_validator_state.exited_lido_validator_indices,
        test_consts::NON_LIDO_VALIDATOR_INDEX,
    );

    let result = executor.run(program_input).await;
    executor.assert_accepted(result)?;

    let program_input = executor.prepare_actual_input(target_slot).await?;
    let result = executor.run(program_input).await;
    executor.assert_accepted(result)
}

#[ignore = "Hits external prover (slow, incurs costs)"]
#[tokio::test(flavor = "multi_thread")]
async fn program_input_tampering_old_state_exited_duplicated_accepted_subsequent_report_accepted() -> Result<()> {
    let executor = TestExecutor::new(test_utils::DEPLOY_SLOT).await?;
    let target_slot = executor.env.get_finalized_slot().await?;
    let intermediate_slot = target_slot - eth_spec::SlotsPerEpoch::to_u64();

    let mut program_input = executor.prepare_actual_input(intermediate_slot).await?;

    program_input.old_lido_validator_state.exited_lido_validator_indices =
        varlists::duplicate_random(program_input.old_lido_validator_state.exited_lido_validator_indices);

    let result = executor.run(program_input).await;
    executor.assert_accepted(result)?;

    let program_input = executor.prepare_actual_input(target_slot).await?;
    let result = executor.run(program_input).await;
    executor.assert_accepted(result)
}

#[ignore = "Hits external prover (slow, incurs costs)"]
#[tokio::test(flavor = "multi_thread")]
async fn program_input_tampering_old_state_exited_removed_accepted_subsequent_report_accepted() -> Result<()> {
    let executor = TestExecutor::new(test_utils::DEPLOY_SLOT).await?;
    let target_slot = executor.env.get_finalized_slot().await?;
    let intermediate_slot = target_slot - eth_spec::SlotsPerEpoch::to_u64();

    let mut program_input = executor.prepare_actual_input(intermediate_slot).await?;

    program_input.old_lido_validator_state.exited_lido_validator_indices =
        varlists::remove_random(program_input.old_lido_validator_state.exited_lido_validator_indices);

    let result = executor.run(program_input).await;
    executor.assert_accepted(result)?;

    let program_input = executor.prepare_actual_input(target_slot).await?;
    let result = executor.run(program_input).await;
    executor.assert_accepted(result)
}

#[ignore = "Hits external prover (slow, incurs costs)"]
#[tokio::test(flavor = "multi_thread")]
async fn program_input_tampering_old_state_exited_shuffled_accepted_subsequent_report_accepted() -> Result<()> {
    let executor = TestExecutor::new(test_utils::DEPLOY_SLOT).await?;
    let target_slot = executor.env.get_finalized_slot().await?;
    let intermediate_slot = target_slot - eth_spec::SlotsPerEpoch::to_u64();

    let mut program_input = executor.prepare_actual_input(intermediate_slot).await?;

    program_input.old_lido_validator_state.exited_lido_validator_indices =
        varlists::ensured_shuffle(program_input.old_lido_validator_state.exited_lido_validator_indices);
    let result = executor.run(program_input).await;
    executor.assert_accepted(result)?;

    let program_input = executor.prepare_actual_input(target_slot).await?;
    let result = executor.run(program_input).await;
    executor.assert_accepted(result)
}

/* #endregion Old LidoValidatorState */

/* #region WithdrawalVaultData */
// * Correct address and proof, arbitrary balance - prover crash
// * Different address (with actual balance for that address), actual contract address proof - prover crash
// * Correct address, but balance and proof for a wrong slot - prover crash
// * Address + balance + proof for a wrong address - contract rejects

#[tokio::test(flavor = "multi_thread")]
async fn program_input_tampering_withdrawal_vault_wrong_balance() -> Result<()> {
    let executor = TestExecutor::default().await?;
    let target_slot = executor.env.get_finalized_slot().await?;

    let mut program_input = executor.prepare_actual_input(target_slot).await?;

    program_input.withdrawal_vault_data.balance += alloy_primitives::U256::from(150);
    executor.assert_fails_in_prover(program_input).await
}

#[tokio::test(flavor = "multi_thread")]
async fn program_input_tampering_withdrawal_vault_right_data_wrong_address() -> Result<()> {
    let executor = TestExecutor::default().await?;
    let target_slot = executor.env.get_finalized_slot().await?;

    let mut program_input = executor.prepare_actual_input(target_slot).await?;

    program_input.withdrawal_vault_data.vault_address = test_consts::ANY_RANDOM_ADDRESS.into();
    executor.assert_fails_in_prover(program_input).await
}

#[tokio::test(flavor = "multi_thread")]
async fn program_input_tampering_withdrawal_vault_outdated_state() -> Result<()> {
    let executor = TestExecutor::default().await?;
    let target_slot = executor.env.get_finalized_slot().await?;

    let mut program_input = executor.prepare_actual_input(target_slot).await?;

    let old_slot = target_slot - eth_spec::SlotsPerEpoch::to_u64();
    let old_bs = executor.env.get_beacon_state(&StateId::Slot(old_slot)).await?;
    let withdrawal_vault_address: Address = executor.env.network_config().lido_withdrwawal_vault_address.into();

    program_input.withdrawal_vault_data = executor
        .env
        .eth_el_client
        .get_withdrawal_vault_data(
            withdrawal_vault_address,
            old_bs.latest_execution_payload_header.block_hash,
        )
        .await?;
    executor.assert_fails_in_prover(program_input).await
}

#[ignore = "Hits external prover (slow, incurs costs)"]
#[tokio::test(flavor = "multi_thread")]
async fn program_input_tampering_withdrawal_vault_data_for_wrong_address() -> Result<()> {
    let executor = TestExecutor::default().await?;
    let target_slot = executor.env.get_finalized_slot().await?;

    let mut program_input = executor.prepare_actual_input(target_slot).await?;

    let bs = executor.get_beacon_state(target_slot).await?;

    let updated_vault_data = executor
        .env
        .eth_el_client
        .get_withdrawal_vault_data(
            test_consts::ANY_RANDOM_ADDRESS.into(),
            bs.latest_execution_payload_header.block_hash,
        )
        .await?;

    program_input.withdrawal_vault_data = updated_vault_data;
    let result = executor.run(program_input).await;
    executor.assert_rejected(result)
}
/* #endregion WithdrawalVaultData */

/* #region ExecutionPayloadHeaderData */
// * Malformed inclusion proof - prover crash
// * Mismatching state_root and it's inclusion proof - prover crash
// * Matching state_root and inclusion proof from a different block - prover crash

#[tokio::test(flavor = "multi_thread")]
async fn program_input_tampering_malformed_exec_header_inclusion_proof() -> Result<()> {
    let executor = TestExecutor::default().await?;
    let target_slot = executor.env.get_finalized_slot().await?;

    let mut program_input = executor.prepare_actual_input(target_slot).await?;

    program_input.latest_execution_header_data.state_root_inclusion_proof = vec![0, 1, 2, 3, 4, 5, 6];
    executor.assert_fails_in_prover(program_input).await
}

#[tokio::test(flavor = "multi_thread")]
async fn program_input_tampering_mismatched_exec_header_inclusion_proof() -> Result<()> {
    let executor = TestExecutor::default().await?;
    let target_slot = executor.env.get_finalized_slot().await?;

    let mut program_input = executor.prepare_actual_input(target_slot).await?;

    let old_slot = target_slot - eth_spec::SlotsPerEpoch::to_u64();
    let old_bs = executor.env.get_beacon_state(&StateId::Slot(old_slot)).await?;

    program_input.latest_execution_header_data.state_root = old_bs.latest_execution_payload_header.state_root;
    executor.assert_fails_in_prover(program_input).await
}

#[tokio::test(flavor = "multi_thread")]
async fn program_input_tampering_exec_header_from_different_state() -> Result<()> {
    let executor = TestExecutor::default().await?;
    let target_slot = executor.env.get_finalized_slot().await?;

    let mut program_input = executor.prepare_actual_input(target_slot).await?;

    let old_slot = target_slot - eth_spec::SlotsPerEpoch::to_u64();
    let old_bs = executor.env.get_beacon_state(&StateId::Slot(old_slot)).await?;

    program_input.latest_execution_header_data =
        ExecutionPayloadHeaderData::new(&old_bs.latest_execution_payload_header);
    executor.assert_fails_in_prover(program_input).await
}

/* #endregion ExecutionPayloadHeaderData */
