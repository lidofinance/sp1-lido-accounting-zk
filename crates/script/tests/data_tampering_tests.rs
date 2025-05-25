mod test_utils;

use std::collections::HashMap;

use anyhow::{anyhow, Result};
use sp1_lido_accounting_scripts::{
    beacon_state_reader::{BeaconStateReader, StateId},
    eth_client::{self, Sp1LidoAccountingReportContract::Sp1LidoAccountingReportContractErrors},
    scripts::{prelude::BeaconStateReaderEnum, shared as shared_logic},
    sp1_client_wrapper::SP1ClientWrapper,
};

use sp1_lido_accounting_zk_shared::{
    eth_consensus_layer::{BeaconBlockHeader, BeaconState, BlsPublicKey, Hash256, Validator},
    io::{
        eth_io::{BeaconChainSlot, HaveEpoch},
        program_io::WithdrawalVaultData,
    },
};
use test_utils::{env::IntegrationTestEnvironment, mark_as_refslot};
use tree_hash::TreeHash;

type BeaconStateMutator = dyn Fn(BeaconState) -> BeaconState;
type WithdrawalVaultDataMutator = dyn Fn(WithdrawalVaultData) -> WithdrawalVaultData;

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

pub struct TamperableBeaconStateReader<'a, T>
where
    T: BeaconStateReader,
{
    inner: &'a T,
    beacon_state_mutators: HashMap<StateId, Box<dyn Fn(BeaconState) -> BeaconState + Send + Sync>>,
    should_update_block_header: HashMap<StateId, bool>,
}

impl<'a, T> TamperableBeaconStateReader<'a, T>
where
    T: BeaconStateReader,
{
    pub fn new(inner: &'a T) -> Self {
        Self {
            inner,
            beacon_state_mutators: HashMap::new(),
            should_update_block_header: HashMap::new(),
        }
    }

    pub fn set_mutator<F>(&mut self, state_id: StateId, update_block_header: bool, mutator: F) -> &mut Self
    where
        F: Fn(BeaconState) -> BeaconState + Send + Sync + 'static,
    {
        self.beacon_state_mutators.insert(state_id.clone(), Box::new(mutator));
        self.should_update_block_header
            .insert(state_id.clone(), update_block_header);
        self
    }

    async fn read_beacon_state_and_header(
        &self,
        state_id: &StateId,
    ) -> anyhow::Result<(BeaconBlockHeader, BeaconState)> {
        let orig_bs = self.inner.read_beacon_state(state_id).await?;
        let orig_bh = self.inner.read_beacon_block_header(state_id).await?;

        let (result_bh, result_bs) = match self.beacon_state_mutators.get(state_id) {
            Some(mutator) => {
                let new_bs = (mutator)(orig_bs);
                let new_bh = match self.should_update_block_header.get(state_id) {
                    Some(true) => {
                        let mut new_bh = orig_bh.clone();
                        new_bh.state_root = new_bs.tree_hash_root();
                        new_bh
                    }
                    _ => orig_bh,
                };
                (new_bh, new_bs)
            }
            None => (orig_bh, orig_bs),
        };
        Ok((result_bh, result_bs))
    }
}

impl<'a, T> BeaconStateReader for TamperableBeaconStateReader<'a, T>
where
    T: BeaconStateReader + Sync,
{
    async fn read_beacon_state(&self, state_id: &StateId) -> anyhow::Result<BeaconState> {
        let (_, bs) = self.read_beacon_state_and_header(state_id).await?;
        Ok(bs)
    }

    async fn read_beacon_block_header(&self, state_id: &StateId) -> anyhow::Result<BeaconBlockHeader> {
        let (bh, _) = self.read_beacon_state_and_header(state_id).await?;
        Ok(bh)
    }
}

struct TestExecutor<'a> {
    pub env: &'a IntegrationTestEnvironment,
    tampered_bs_reader: TamperableBeaconStateReader<'a, BeaconStateReaderEnum>,
    withdrawal_vault_data_mutator: Box<WithdrawalVaultDataMutator>,
}

impl<'a> TestExecutor<'a> {
    async fn new(env: &'a IntegrationTestEnvironment) -> anyhow::Result<Self> {
        let tampered_bs_reader = TamperableBeaconStateReader::new(&env.script_runtime.eth_infra.beacon_state_reader);

        let instance = Self {
            env,
            tampered_bs_reader,
            withdrawal_vault_data_mutator: Box::new(|wvd| wvd),
        };

        Ok(instance)
    }

    pub fn set_bs_mutator<F>(&mut self, state_id: StateId, update_block_header: bool, mutator: F) -> &mut Self
    where
        F: Fn(BeaconState) -> BeaconState + Send + Sync + 'static,
    {
        self.tampered_bs_reader
            .set_mutator(state_id, update_block_header, mutator);
        self
    }

    pub fn set_withdrawal_vault_mutator(&mut self, mutator: Box<WithdrawalVaultDataMutator>) {
        self.withdrawal_vault_data_mutator = mutator;
    }

    async fn run_test(&self, target_slot: BeaconChainSlot) -> core::result::Result<(), TestError> {
        sp1_sdk::utils::setup_logger();
        let lido_withdrawal_credentials: Hash256 = self.env.script_runtime.lido_settings.withdrawal_credentials.into();

        let reference_slot = mark_as_refslot(target_slot);
        let previous_slot = self
            .env
            .script_runtime
            .lido_infra
            .report_contract
            .get_latest_validator_state_slot()
            .await?;

        let target_bh = self
            .tampered_bs_reader
            .read_beacon_block_header(&StateId::Slot(target_slot))
            .await?;
        let target_bs = self
            .tampered_bs_reader
            .read_beacon_state(&StateId::Slot(target_slot))
            .await?;
        // Should read old state from untampered reader, so the old state compute will match
        let old_bs = self.env.read_beacon_state(&StateId::Slot(previous_slot)).await?;

        let withdrawal_vault_data = self.env.get_balance_proof(&StateId::Slot(target_slot)).await?;
        let tampered_withdrawal_vault_data = (self.withdrawal_vault_data_mutator)(withdrawal_vault_data);

        tracing::info!("Preparing program input");
        let (program_input, _public_values) = shared_logic::prepare_program_input(
            reference_slot,
            &target_bs,
            &target_bh,
            &old_bs,
            &lido_withdrawal_credentials,
            tampered_withdrawal_vault_data,
            false,
        )
        .expect("Failed to prepare program input");
        tracing::info!("Requesting proof");
        let try_proof = self.env.script_runtime.sp1_infra.sp1_client.prove(program_input);

        if let Err(e) = try_proof {
            return Err(TestError::ProofFailed(e));
        }

        tracing::info!("Generated proof");
        let proof = try_proof.unwrap();

        tracing::info!("Sending report");
        let _result = self
            .env
            .script_runtime
            .lido_infra
            .report_contract
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

fn validator_indices<P>(bs: &BeaconState, positions: &[usize], predicate: P) -> Vec<usize>
where
    P: Fn(&Validator) -> bool,
{
    let filtered_validator_indices: Vec<usize> = bs
        .validators
        .iter()
        .enumerate()
        .filter_map(
            |(index, validator)| {
                if predicate(validator) {
                    Some(index)
                } else {
                    None
                }
            },
        )
        .collect();
    positions.iter().map(|idx| filtered_validator_indices[*idx]).collect()
}

fn is_lido(withdrawal_credentials: Hash256) -> impl Fn(&Validator) -> bool {
    move |validator: &Validator| validator.withdrawal_credentials == withdrawal_credentials
}

fn is_non_lido(withdrawal_credentials: Hash256) -> impl Fn(&Validator) -> bool {
    move |validator: &Validator| validator.withdrawal_credentials != withdrawal_credentials
}

/*
Test scenarios:
Adding:
* Add lido validator - active state
* Add lido validator - pending activation state
* Add lido validator - exited state
* Add non-Lido validator - any state
Removing
* Remove Lido validator - single
* Remove Lido validator - multiple
Modifying
* Change Lido validator to have non-Lido withdrawal credentials
* Change non-Lido validator to have Lido withdrawal credentials
* Make Lido validator exited
Omitting
* Omit added Lido validator
* Omit exited lido validator
* Omit activated Lido validator - not implemented, see comment on tampering_omit_activated_lido_validator
Balance
* Change single Lido validator balance
* Change mulitple Lido validator balance
* Change two Lido validator balances to cancel each other out (sum is the same)

Withdrawal vault
* Real proof, tampered balance
* Tampered proof, real balance
* Real balance and proof for wrong slot
*/

// The attacker might approach data tampering from two angles:
// 1. Tamper only beacon block state, leaving beacon block header alone
// 2. Tamper both state and header
// The first scenario leads to beacon_state.tree_hash_root != beacon_block_header.state_root
// and is rejected by the program (i.e. it won't even get to generating the report)
// Hence setting this to true is the only option to actually test end-to-end
// But this is kept here for an easy check that this is the case in all the scenarios listed below
// Flipping this to false should cause all tests to fail generating proof
const MODIFY_BEACON_BLOCK_HASH: bool = true;

// Note: these tests will hit the prover network - will have relatively longer run
// time (1-2 minutes) and also incur proving costs.
#[ignore]
#[tokio::test(flavor = "multi_thread")]
async fn data_tampering_no_tampering_should_pass() -> Result<()> {
    let env = IntegrationTestEnvironment::default().await?;
    let executor = TestExecutor::new(&env).await?;
    let target_slot = executor.env.get_finalized_slot().await?;

    let result = executor.run_test(target_slot).await;
    executor.assert_accepted(result)
}

#[ignore]
#[tokio::test(flavor = "multi_thread")]
async fn data_tampering_add_active_lido_validator() -> Result<()> {
    let env = IntegrationTestEnvironment::default().await?;
    let mut executor = TestExecutor::new(&env).await?;
    let target_slot = executor.env.get_finalized_slot().await?;
    executor.set_bs_mutator(
        StateId::Slot(target_slot),
        MODIFY_BEACON_BLOCK_HASH,
        move |beacon_state| {
            let balance: u64 = 32_000_000_000;
            let new_validator = Validator {
                pubkey: BlsPublicKey::from([0_u8; 48].to_vec()),
                withdrawal_credentials: env.script_runtime.lido_settings.withdrawal_credentials,
                effective_balance: balance,
                slashed: false,
                activation_eligibility_epoch: beacon_state.epoch() - 10,
                activation_epoch: beacon_state.epoch() - 5,
                exit_epoch: u64::MAX,
                withdrawable_epoch: beacon_state.epoch() - 1,
            };
            let mut new_bs = beacon_state.clone();
            new_bs.validators.push(new_validator).expect("Failed to add balance");
            new_bs.balances.push(balance).expect("Failed to add validator");
            new_bs
        },
    );

    let result = executor.run_test(target_slot).await;
    executor.assert_rejected(result)
}

#[ignore]
#[tokio::test(flavor = "multi_thread")]
async fn data_tampering_add_pending_lido_validator() -> Result<()> {
    let env = IntegrationTestEnvironment::default().await?;
    let mut executor = TestExecutor::new(&env).await?;
    let target_slot = executor.env.get_finalized_slot().await?;
    executor.set_bs_mutator(
        StateId::Slot(target_slot),
        MODIFY_BEACON_BLOCK_HASH,
        move |beacon_state| {
            let balance: u64 = 1_000_000_000;
            let new_validator = Validator {
                pubkey: BlsPublicKey::from([0_u8; 48].to_vec()),
                withdrawal_credentials: env.script_runtime.lido_settings.withdrawal_credentials,
                effective_balance: balance,
                slashed: false,
                activation_eligibility_epoch: beacon_state.epoch() + 10,
                activation_epoch: u64::MAX,
                exit_epoch: u64::MAX,
                withdrawable_epoch: u64::MAX,
            };
            let mut new_bs = beacon_state.clone();
            new_bs.validators.push(new_validator).expect("Failed to add balance");
            new_bs.balances.push(balance).expect("Failed to add validator");
            new_bs
        },
    );

    let result = executor.run_test(target_slot).await;
    executor.assert_rejected(result)
}

#[ignore]
#[tokio::test(flavor = "multi_thread")]
async fn data_tampering_add_exited_lido_validator() -> Result<()> {
    let env = IntegrationTestEnvironment::default().await?;
    let mut executor = TestExecutor::new(&env).await?;
    let target_slot = executor.env.get_finalized_slot().await?;
    executor.set_bs_mutator(
        StateId::Slot(target_slot),
        MODIFY_BEACON_BLOCK_HASH,
        move |beacon_state| {
            let balance: u64 = 1_000_000_000;
            let new_validator = Validator {
                pubkey: BlsPublicKey::from([0_u8; 48].to_vec()),
                withdrawal_credentials: env.script_runtime.lido_settings.withdrawal_credentials,
                effective_balance: balance,
                slashed: false,
                activation_eligibility_epoch: beacon_state.epoch() - 10,
                activation_epoch: beacon_state.epoch() - 6,
                exit_epoch: beacon_state.epoch() - 1,
                withdrawable_epoch: beacon_state.epoch() - 3,
            };
            let mut new_bs = beacon_state.clone();
            new_bs.validators.push(new_validator).expect("Failed to add balance");
            new_bs.balances.push(balance).expect("Failed to add validator");
            new_bs
        },
    );

    let result = executor.run_test(target_slot).await;
    executor.assert_rejected(result)
}

#[ignore]
#[tokio::test(flavor = "multi_thread")]
async fn data_tampering_add_active_non_lido_validator() -> Result<()> {
    let env = IntegrationTestEnvironment::default().await?;
    let mut executor = TestExecutor::new(&env).await?;
    let target_slot = executor.env.get_finalized_slot().await?;
    executor.set_bs_mutator(StateId::Slot(target_slot), MODIFY_BEACON_BLOCK_HASH, |beacon_state| {
        let balance: u64 = 32_000_000_000;
        let new_validator = Validator {
            pubkey: BlsPublicKey::from([0_u8; 48].to_vec()),
            withdrawal_credentials: [0u8; 32].into(),
            effective_balance: balance,
            slashed: false,
            activation_eligibility_epoch: beacon_state.epoch() - 10,
            activation_epoch: beacon_state.epoch() - 5,
            exit_epoch: u64::MAX,
            withdrawable_epoch: beacon_state.epoch() - 1,
        };
        let mut new_bs = beacon_state.clone();
        new_bs.validators.push(new_validator).expect("Failed to add balance");
        new_bs.balances.push(balance).expect("Failed to add validator");
        new_bs
    });

    let result = executor.run_test(target_slot).await;
    executor.assert_rejected(result)
}

#[ignore]
#[tokio::test(flavor = "multi_thread")]
async fn data_tampering_remove_lido_validator() -> Result<()> {
    let env = IntegrationTestEnvironment::default().await?;
    let mut executor = TestExecutor::new(&env).await?;
    let target_slot = executor.env.get_finalized_slot().await?;
    executor.set_bs_mutator(
        StateId::Slot(target_slot),
        MODIFY_BEACON_BLOCK_HASH,
        move |beacon_state| {
            let mut new_bs = beacon_state.clone();
            let is_lido_pred = is_lido(env.script_runtime.lido_settings.withdrawal_credentials);

            let validator_idx = validator_indices(&beacon_state, &[0], is_lido_pred)[0];
            let mut new_validators: Vec<Validator> = new_bs.validators.to_vec();
            new_validators.remove(validator_idx);
            new_bs.validators = new_validators.into();
            let mut new_balances: Vec<u64> = new_bs.balances.to_vec();
            new_balances.remove(validator_idx);
            new_bs.balances = new_balances.into();
            new_bs
        },
    );

    let result = executor.run_test(target_slot).await;
    executor.assert_failed_proof(result)
}

#[ignore]
#[tokio::test(flavor = "multi_thread")]
async fn data_tampering_remove_multi_lido_validator() -> Result<()> {
    let env = IntegrationTestEnvironment::default().await?;
    let mut executor = TestExecutor::new(&env).await?;
    let target_slot = executor.env.get_finalized_slot().await?;
    executor.set_bs_mutator(
        StateId::Slot(target_slot),
        MODIFY_BEACON_BLOCK_HASH,
        move |beacon_state| {
            let mut new_bs = beacon_state.clone();
            let is_lido_pred = is_lido(env.script_runtime.lido_settings.withdrawal_credentials);

            let remove_idxs = validator_indices(&beacon_state, &[0, 1, 3], is_lido_pred);
            let new_validators: Vec<Validator> = new_bs
                .validators
                .to_vec()
                .iter()
                .enumerate()
                .filter_map(|(idx, validator)| {
                    if remove_idxs.contains(&idx) {
                        None
                    } else {
                        Some(validator)
                    }
                })
                .cloned()
                .collect();
            let new_balances: Vec<u64> = new_bs
                .balances
                .to_vec()
                .iter()
                .enumerate()
                .filter_map(|(idx, balance)| {
                    if remove_idxs.contains(&idx) {
                        None
                    } else {
                        Some(balance)
                    }
                })
                .cloned()
                .collect();
            new_bs.validators = new_validators.into();
            new_bs.balances = new_balances.into();
            new_bs
        },
    );

    let result = executor.run_test(target_slot).await;
    executor.assert_failed_proof(result)
}

#[ignore]
#[tokio::test(flavor = "multi_thread")]
async fn data_tampering_change_lido_to_non_lido_validator() -> Result<()> {
    let env = IntegrationTestEnvironment::default().await?;
    let mut executor = TestExecutor::new(&env).await?;
    let target_slot = executor.env.get_finalized_slot().await?;
    executor.set_bs_mutator(
        StateId::Slot(target_slot),
        MODIFY_BEACON_BLOCK_HASH,
        move |beacon_state| {
            let mut new_bs = beacon_state.clone();
            let is_lido_pred = is_lido(env.script_runtime.lido_settings.withdrawal_credentials);

            let validator_idx = validator_indices(&beacon_state, &[0], is_lido_pred)[0];
            new_bs.validators[validator_idx].withdrawal_credentials = [0u8; 32].into();
            new_bs
        },
    );

    let result = executor.run_test(target_slot).await;
    executor.assert_failed_proof(result)
}

#[ignore]
#[tokio::test(flavor = "multi_thread")]
async fn data_tampering_change_non_lido_to_lido_validator() -> Result<()> {
    let env = IntegrationTestEnvironment::default().await?;
    let mut executor = TestExecutor::new(&env).await?;
    let target_slot = executor.env.get_finalized_slot().await?;
    executor.set_bs_mutator(
        StateId::Slot(target_slot),
        MODIFY_BEACON_BLOCK_HASH,
        move |beacon_state| {
            let mut new_bs = beacon_state.clone();
            let is_non_lido_pred = is_non_lido(env.script_runtime.lido_settings.withdrawal_credentials);

            let validator_idx = validator_indices(&beacon_state, &[0], is_non_lido_pred)[0];
            new_bs.validators[validator_idx].withdrawal_credentials =
                env.script_runtime.lido_settings.withdrawal_credentials;
            new_bs
        },
    );

    let result = executor.run_test(target_slot).await;
    executor.assert_failed_proof(result)
}

#[ignore]
#[tokio::test(flavor = "multi_thread")]
async fn data_tampering_change_lido_make_exited() -> Result<()> {
    let env = IntegrationTestEnvironment::default().await?;
    let mut executor = TestExecutor::new(&env).await?;
    let target_slot = executor.env.get_finalized_slot().await?;
    executor.set_bs_mutator(
        StateId::Slot(target_slot),
        MODIFY_BEACON_BLOCK_HASH,
        move |beacon_state| {
            let mut new_bs = beacon_state.clone();
            let is_lido_pred = is_lido(env.script_runtime.lido_settings.withdrawal_credentials);

            let validator_idx = validator_indices(&beacon_state, &[0], is_lido_pred)[0];
            new_bs.validators[validator_idx].exit_epoch = new_bs.epoch() - 10;
            new_bs
        },
    );

    let result = executor.run_test(target_slot).await;
    executor.assert_rejected(result)
}

#[ignore]
#[tokio::test(flavor = "multi_thread")]
async fn data_tampering_omit_new_deposited_lido_validator() -> Result<()> {
    let env = IntegrationTestEnvironment::default().await?;
    let mut executor = TestExecutor::new(&env).await?;
    let target_slot = executor.env.get_finalized_slot().await?;
    executor.set_bs_mutator(
        StateId::Slot(target_slot),
        MODIFY_BEACON_BLOCK_HASH,
        move |beacon_state| {
            let mut new_bs = beacon_state.clone();
            let is_lido_pred = is_lido(env.script_runtime.lido_settings.withdrawal_credentials);

            // old state https://sepolia.beaconcha.in/slot/5832096 had only 1 validator - all others are now "added"
            let added_deposited_idx = 3;
            let validator_idx = validator_indices(&beacon_state, &[added_deposited_idx], is_lido_pred)[0];
            let mut new_validators: Vec<Validator> = new_bs.validators.to_vec();
            new_validators.remove(validator_idx);
            new_bs.validators = new_validators.into();
            let mut new_balances: Vec<u64> = new_bs.balances.to_vec();
            new_balances.remove(validator_idx);
            new_bs.balances = new_balances.into();
            new_bs
        },
    );

    let result = executor.run_test(target_slot).await;
    executor.assert_rejected(result)
}

#[ignore]
#[tokio::test(flavor = "multi_thread")]
async fn data_tampering_omit_exited_lido_validator() -> Result<()> {
    let env = IntegrationTestEnvironment::default().await?;
    let mut executor = TestExecutor::new(&env).await?;
    let target_slot = executor.env.get_finalized_slot().await?;
    executor.set_bs_mutator(
        StateId::Slot(target_slot),
        MODIFY_BEACON_BLOCK_HASH,
        move |beacon_state| {
            let mut new_bs = beacon_state.clone();
            let is_lido_pred = is_lido(env.script_runtime.lido_settings.withdrawal_credentials);
            // old state https://sepolia.beaconcha.in/slot/5832096 had only 1 validator - all others are now "added"
            let added_exited_idx = 3;
            let validator_idx = validator_indices(&beacon_state, &[added_exited_idx], is_lido_pred)[0];
            let mut new_validators: Vec<Validator> = new_bs.validators.to_vec();
            new_validators.remove(validator_idx);
            new_bs.validators = new_validators.into();
            let mut new_balances: Vec<u64> = new_bs.balances.to_vec();
            new_balances.remove(validator_idx);
            new_bs.balances = new_balances.into();
            new_bs
        },
    );

    let result = executor.run_test(target_slot).await;
    executor.assert_rejected(result)
}

// this one is currently impossible as the base state (at test_utils::DEPLOY_SLOT)
// had no validators in pending state
#[tokio::test(flavor = "multi_thread")]
#[ignore]
async fn data_tampering_omit_pending_lido_validator_STUB() -> Result<()> {
    //TODO: implement test with pending validators
    Ok(())
}

#[ignore]
#[tokio::test(flavor = "multi_thread")]
async fn data_tampering_balance_change_lido_validator_balance() -> Result<()> {
    let env = IntegrationTestEnvironment::default().await?;
    let mut executor = TestExecutor::new(&env).await?;
    let target_slot = executor.env.get_finalized_slot().await?;
    executor.set_bs_mutator(
        StateId::Slot(target_slot),
        MODIFY_BEACON_BLOCK_HASH,
        move |beacon_state| {
            let mut new_bs = beacon_state.clone();
            let is_lido_pred = is_lido(env.script_runtime.lido_settings.withdrawal_credentials);

            let validator_idx = validator_indices(&beacon_state, &[0], is_lido_pred)[0];
            new_bs.balances[validator_idx] += 10;
            new_bs
        },
    );

    let result = executor.run_test(target_slot).await;
    executor.assert_rejected(result)
}

#[ignore]
#[tokio::test(flavor = "multi_thread")]
async fn data_tampering_balance_change_multi_lido_validator_balance() -> Result<()> {
    let env = IntegrationTestEnvironment::default().await?;
    let mut executor = TestExecutor::new(&env).await?;
    let target_slot = executor.env.get_finalized_slot().await?;
    executor.set_bs_mutator(
        StateId::Slot(target_slot),
        MODIFY_BEACON_BLOCK_HASH,
        move |beacon_state| {
            let mut new_bs = beacon_state.clone();
            let is_lido_pred = is_lido(env.script_runtime.lido_settings.withdrawal_credentials);

            let _adjust_idxs = validator_indices(&beacon_state, &[0, 1, 3], is_lido_pred);
            for idx in _adjust_idxs {
                new_bs.balances[idx] = 0;
            }
            new_bs
        },
    );

    let result = executor.run_test(target_slot).await;
    executor.assert_rejected(result)
}

#[ignore]
#[tokio::test(flavor = "multi_thread")]
async fn data_tampering_balance_change_lido_validator_balance_cancel_out() -> Result<()> {
    let env = IntegrationTestEnvironment::default().await?;
    let mut executor = TestExecutor::new(&env).await?;
    let target_slot = executor.env.get_finalized_slot().await?;
    executor.set_bs_mutator(
        StateId::Slot(target_slot),
        MODIFY_BEACON_BLOCK_HASH,
        move |beacon_state| {
            let mut new_bs = beacon_state.clone();
            let is_lido_pred = is_lido(env.script_runtime.lido_settings.withdrawal_credentials);

            let indices_to_adjust = validator_indices(&beacon_state, &[1, 3], is_lido_pred);
            let source = indices_to_adjust[0];
            let dest = indices_to_adjust[1];
            print!(
                "Source idx={}, balance={}; dest idx={}, balance={}",
                source, new_bs.balances[source], dest, new_bs.balances[dest]
            );
            let amount: u64 = 5_000_000_000;
            new_bs.balances[source] -= amount;
            new_bs.balances[dest] += amount;
            new_bs
        },
    );

    let result = executor.run_test(target_slot).await;
    executor.assert_rejected(result)
}

#[ignore]
#[tokio::test(flavor = "multi_thread")]
async fn data_tampering_withdrawal_vault_tampered_balance() -> Result<()> {
    let env = IntegrationTestEnvironment::default().await?;
    let mut executor = TestExecutor::new(&env).await?;
    let target_slot = executor.env.get_finalized_slot().await?;
    executor.set_withdrawal_vault_mutator(Box::new(|withdrawal_vault_data: WithdrawalVaultData| {
        let mut tampered_wvd = withdrawal_vault_data.clone();
        tampered_wvd.balance = tampered_wvd
            .balance
            .saturating_add(alloy_primitives::U256::from(10000000));
        tampered_wvd
    }));
    let result = executor.run_test(target_slot).await;
    executor.assert_failed_proof(result)
}

#[ignore]
#[tokio::test(flavor = "multi_thread")]
async fn data_tampering_withdrawal_vault_tampered_proof() -> Result<()> {
    let env = IntegrationTestEnvironment::default().await?;
    let mut executor = TestExecutor::new(&env).await?;
    let target_slot = executor.env.get_finalized_slot().await?;
    let other_slot = target_slot - 100;

    let other_slot_wv_data = executor.env.get_balance_proof(&StateId::Slot(other_slot)).await?;

    executor.set_withdrawal_vault_mutator(Box::new(move |wvd| {
        let mut tampered_wvd = wvd.clone();
        let other_account_proof = other_slot_wv_data.account_proof.clone();
        tampered_wvd.account_proof = other_account_proof;
        tampered_wvd
    }));
    let result = executor.run_test(target_slot).await;
    executor.assert_failed_proof(result)
}

#[ignore]
#[tokio::test(flavor = "multi_thread")]
async fn data_tampering_withdrawal_vault_different_slot() -> Result<()> {
    let env = IntegrationTestEnvironment::default().await?;
    let mut executor = TestExecutor::new(&env).await?;
    let target_slot = executor.env.get_finalized_slot().await?;
    let other_slot = target_slot - 100;

    let other_slot_wv_data = executor.env.get_balance_proof(&StateId::Slot(other_slot)).await?;

    executor.set_withdrawal_vault_mutator(Box::new(move |_wvd| other_slot_wv_data.clone()));
    let result = executor.run_test(target_slot).await;
    executor.assert_failed_proof(result)
}
