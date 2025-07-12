mod test_utils;

use std::{collections::HashMap, sync::Arc};

use anyhow::Result;
use sp1_lido_accounting_scripts::{
    beacon_state_reader::{BeaconStateReader, StateId},
    eth_client::Sp1LidoAccountingReportContract::Sp1LidoAccountingReportContractErrors,
    scripts::{prelude::BeaconStateReaderEnum, shared as shared_logic},
    sp1_client_wrapper::SP1ClientWrapper,
};

use sp1_lido_accounting_zk_shared::{
    eth_consensus_layer::{BeaconBlockHeader, BeaconState, BlsPublicKey, Hash256, Validator},
    io::{
        eth_io::{BeaconChainSlot, HaveEpoch},
        program_io::WithdrawalVaultData,
    },
    lido::{ValidatorOps, ValidatorStatus},
};
use test_utils::{env::IntegrationTestEnvironment, mark_as_refslot, TestAssertions, TestError, DEPLOY_SLOT};
use tree_hash::TreeHash;

type WithdrawalVaultDataMutator = dyn Fn(WithdrawalVaultData) -> WithdrawalVaultData;

pub struct TamperableBeaconStateReader<T>
where
    T: BeaconStateReader,
{
    inner: Arc<T>,
    beacon_state_mutators: HashMap<StateId, Box<dyn Fn(BeaconState) -> BeaconState + Send + Sync>>,
    should_update_block_header: HashMap<StateId, bool>,
}

impl<T> TamperableBeaconStateReader<T>
where
    T: BeaconStateReader,
{
    pub fn new(inner: Arc<T>) -> Self {
        Self {
            inner: Arc::clone(&inner),
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

impl<T> BeaconStateReader for TamperableBeaconStateReader<T>
where
    T: BeaconStateReader + Sync + Send,
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

struct TestExecutor {
    pub env: IntegrationTestEnvironment,
    tampered_bs_reader: TamperableBeaconStateReader<BeaconStateReaderEnum>,
    withdrawal_vault_data_mutator: Box<WithdrawalVaultDataMutator>,
}

impl TestExecutor {
    fn new(env: IntegrationTestEnvironment) -> Self {
        let bs_reader = Arc::clone(&env.script_runtime.eth_infra.beacon_state_reader);
        let tampered_bs_reader = TamperableBeaconStateReader::new(bs_reader);

        Self {
            env,
            tampered_bs_reader,
            withdrawal_vault_data_mutator: Box::new(|wvd| wvd),
        }
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
        let lido_withdrawal_credentials: Hash256 = self.env.script_runtime.lido_settings.withdrawal_credentials;

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

        let old_bs = self
            .tampered_bs_reader
            .read_beacon_state(&StateId::Slot(previous_slot))
            .await?;

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

    pub fn lido_withdrawal_credentials(&self) -> Hash256 {
        self.env.script_runtime.lido_settings.withdrawal_credentials
    }
}

fn all_validator_indices<P>(bs: &BeaconState, predicate: P) -> Vec<usize>
where
    P: Fn(&Validator) -> bool,
{
    bs.validators
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
        .collect()
}

fn positional_validator_indices<P>(bs: &BeaconState, positions: &[usize], predicate: P) -> Vec<usize>
where
    P: Fn(&Validator) -> bool,
{
    let filtered_validator_indices: Vec<usize> = all_validator_indices(bs, predicate);
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

async fn setup_executor() -> Result<(TestExecutor, BeaconChainSlot)> {
    let mut env = IntegrationTestEnvironment::default().await?;
    let target_slot = env.get_finalized_slot().await?;
    env.apply_standard_adjustments(&target_slot).await?;

    Ok((TestExecutor::new(env), target_slot))
}

// Note: these tests will hit the prover network - will have relatively longer run
// time (1-2 minutes) and also incur proving costs.
#[ignore]
#[tokio::test(flavor = "multi_thread")]
async fn data_tampering_no_tampering_should_pass() -> Result<()> {
    let (executor, target_slot) = setup_executor().await?;

    let result = executor.run_test(target_slot).await;
    TestAssertions::assert_accepted(result)
}

#[ignore]
#[tokio::test(flavor = "multi_thread")]
async fn data_tampering_add_active_lido_validator() -> Result<()> {
    let (mut executor, target_slot) = setup_executor().await?;
    let lido_creds = executor.lido_withdrawal_credentials();

    executor.set_bs_mutator(
        StateId::Slot(target_slot),
        MODIFY_BEACON_BLOCK_HASH,
        move |beacon_state| {
            let balance: u64 = 32_000_000_000;
            let new_validator = Validator {
                pubkey: BlsPublicKey::from([0_u8; 48].to_vec()),
                withdrawal_credentials: lido_creds,
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
    TestAssertions::assert_rejected_with(result, |e| {
        matches!(e, Sp1LidoAccountingReportContractErrors::BeaconBlockHashMismatch(_))
    })
}

#[ignore]
#[tokio::test(flavor = "multi_thread")]
async fn data_tampering_add_pending_lido_validator() -> Result<()> {
    let (mut executor, target_slot) = setup_executor().await?;
    let lido_creds = executor.lido_withdrawal_credentials();

    executor.set_bs_mutator(
        StateId::Slot(target_slot),
        MODIFY_BEACON_BLOCK_HASH,
        move |beacon_state| {
            let balance: u64 = 1_000_000_000;
            let new_validator = Validator {
                pubkey: BlsPublicKey::from([0_u8; 48].to_vec()),
                withdrawal_credentials: lido_creds,
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
    TestAssertions::assert_rejected_with(result, |e| {
        matches!(e, Sp1LidoAccountingReportContractErrors::BeaconBlockHashMismatch(_))
    })
}

#[ignore]
#[tokio::test(flavor = "multi_thread")]
async fn data_tampering_add_exited_lido_validator() -> Result<()> {
    let (mut executor, target_slot) = setup_executor().await?;
    let lido_creds = executor.lido_withdrawal_credentials();

    executor.set_bs_mutator(
        StateId::Slot(target_slot),
        MODIFY_BEACON_BLOCK_HASH,
        move |beacon_state| {
            let balance: u64 = 1_000_000_000;
            let new_validator = Validator {
                pubkey: BlsPublicKey::from([0_u8; 48].to_vec()),
                withdrawal_credentials: lido_creds,
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
    TestAssertions::assert_rejected_with(result, |e| {
        matches!(e, Sp1LidoAccountingReportContractErrors::BeaconBlockHashMismatch(_))
    })
}

#[ignore]
#[tokio::test(flavor = "multi_thread")]
async fn data_tampering_add_active_non_lido_validator() -> Result<()> {
    let (mut executor, target_slot) = setup_executor().await?;

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
    TestAssertions::assert_rejected_with(result, |e| {
        matches!(e, Sp1LidoAccountingReportContractErrors::BeaconBlockHashMismatch(_))
    })
}

#[tokio::test(flavor = "multi_thread")]
async fn data_tampering_remove_lido_validator() -> Result<()> {
    let (mut executor, target_slot) = setup_executor().await?;
    let lido_creds = executor.lido_withdrawal_credentials();

    executor.set_bs_mutator(
        StateId::Slot(target_slot),
        MODIFY_BEACON_BLOCK_HASH,
        move |beacon_state| {
            let mut new_bs = beacon_state.clone();
            let is_lido_pred = is_lido(lido_creds);

            let validator_idx = positional_validator_indices(&beacon_state, &[0], is_lido_pred)[0];
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
    TestAssertions::assert_failed_proof(result)
}

#[tokio::test(flavor = "multi_thread")]
async fn data_tampering_remove_multi_lido_validator() -> Result<()> {
    let (mut executor, target_slot) = setup_executor().await?;
    let lido_creds = executor.lido_withdrawal_credentials();

    executor.set_bs_mutator(
        StateId::Slot(target_slot),
        MODIFY_BEACON_BLOCK_HASH,
        move |beacon_state| {
            let mut new_bs = beacon_state.clone();
            let is_lido_pred = is_lido(lido_creds);

            let remove_idxs = positional_validator_indices(&beacon_state, &[0, 1, 3], is_lido_pred);
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
    TestAssertions::assert_failed_proof(result)
}

#[tokio::test(flavor = "multi_thread")]
async fn data_tampering_change_lido_to_non_lido_validator() -> Result<()> {
    let (mut executor, target_slot) = setup_executor().await?;
    let lido_creds = executor.lido_withdrawal_credentials();

    executor.set_bs_mutator(
        StateId::Slot(target_slot),
        MODIFY_BEACON_BLOCK_HASH,
        move |beacon_state| {
            let mut new_bs = beacon_state.clone();
            let is_lido_pred = is_lido(lido_creds);

            let validator_idx = positional_validator_indices(&beacon_state, &[0], is_lido_pred)[0];
            new_bs.validators[validator_idx].withdrawal_credentials = [0u8; 32].into();
            new_bs
        },
    );

    let result = executor.run_test(target_slot).await;
    TestAssertions::assert_failed_proof(result)
}

#[tokio::test(flavor = "multi_thread")]
async fn data_tampering_change_non_lido_to_lido_validator() -> Result<()> {
    let (mut executor, target_slot) = setup_executor().await?;
    let lido_creds = executor.lido_withdrawal_credentials();

    executor.set_bs_mutator(
        StateId::Slot(target_slot),
        MODIFY_BEACON_BLOCK_HASH,
        move |beacon_state| {
            let mut new_bs = beacon_state.clone();
            let is_non_lido_pred = is_non_lido(lido_creds);

            let validator_idx = positional_validator_indices(&beacon_state, &[0], is_non_lido_pred)[0];
            new_bs.validators[validator_idx].withdrawal_credentials =
                executor.env.script_runtime.lido_settings.withdrawal_credentials;
            new_bs
        },
    );

    let result = executor.run_test(target_slot).await;
    TestAssertions::assert_failed_proof(result)
}

#[ignore]
#[tokio::test(flavor = "multi_thread")]
async fn data_tampering_change_lido_make_exited() -> Result<()> {
    let (mut executor, target_slot) = setup_executor().await?;
    let lido_creds = executor.lido_withdrawal_credentials();

    executor.set_bs_mutator(
        StateId::Slot(target_slot),
        MODIFY_BEACON_BLOCK_HASH,
        move |beacon_state| {
            let mut new_bs = beacon_state.clone();
            let is_lido_pred = is_lido(lido_creds);

            let validator_idx = positional_validator_indices(&beacon_state, &[0], is_lido_pred)[0];
            new_bs.validators[validator_idx].exit_epoch = new_bs.epoch() - 10;
            new_bs
        },
    );

    let result = executor.run_test(target_slot).await;
    TestAssertions::assert_rejected_with(result, |e| {
        matches!(e, Sp1LidoAccountingReportContractErrors::BeaconBlockHashMismatch(_))
    })
}

#[ignore]
#[tokio::test(flavor = "multi_thread")]
async fn data_tampering_omit_new_deposited_lido_validator() -> Result<()> {
    let (mut executor, target_slot) = setup_executor().await?;
    let lido_creds = executor.lido_withdrawal_credentials();

    let old_bs = executor.env.read_beacon_state(&StateId::Slot(DEPLOY_SLOT)).await?;
    let max_old_validator_index = old_bs.validators.len() - 1;

    executor.set_bs_mutator(
        StateId::Slot(target_slot),
        MODIFY_BEACON_BLOCK_HASH,
        move |beacon_state| {
            let mut new_bs = beacon_state.clone();
            let epoch = new_bs.epoch();

            let all_lido_deposited: Vec<usize> = all_validator_indices(&beacon_state, |validator| {
                validator.withdrawal_credentials == lido_creds && validator.status(epoch) == ValidatorStatus::Exited
            })
            .into_iter()
            .filter(|&idx| idx > max_old_validator_index)
            .collect();
            let validator_idx = all_lido_deposited[0];
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
    TestAssertions::assert_rejected_with(result, |e| {
        matches!(e, Sp1LidoAccountingReportContractErrors::BeaconBlockHashMismatch(_))
    })
}

#[ignore]
#[tokio::test(flavor = "multi_thread")]
async fn data_tampering_omit_change_to_exited_state_lido_validator_naive() -> Result<()> {
    let (mut executor, target_slot) = setup_executor().await?;
    let lido_creds = executor.lido_withdrawal_credentials();

    executor.set_bs_mutator(
        StateId::Slot(target_slot),
        MODIFY_BEACON_BLOCK_HASH,
        move |beacon_state| {
            let mut new_bs = beacon_state.clone();
            let epoch = new_bs.epoch();
            let all_lido_exited = all_validator_indices(&beacon_state, |validator| {
                validator.withdrawal_credentials == lido_creds && validator.status(epoch) == ValidatorStatus::Exited
            });

            let validator_idx = all_lido_exited[1]; // picking the second exited validator
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
    TestAssertions::assert_failed_proof(result)
}

#[ignore]
#[tokio::test(flavor = "multi_thread")]
async fn data_tampering_omit_change_to_exited_state_lido_validator_adjust_old_state() -> Result<()> {
    let (mut executor, target_slot) = setup_executor().await?;
    let lido_creds = executor.lido_withdrawal_credentials();

    const EXITED_VALIDATOR_TO_REMOVE: usize = 1;

    let mutator = move |beacon_state: BeaconState| {
        let mut new_bs = beacon_state.clone();
        let epoch = new_bs.epoch();
        let all_lido_exited = all_validator_indices(&beacon_state, |validator| {
            validator.withdrawal_credentials == lido_creds && validator.status(epoch) == ValidatorStatus::Exited
        });

        let validator_idx = all_lido_exited[EXITED_VALIDATOR_TO_REMOVE]; // picking the second exited validator
        let mut new_validators: Vec<Validator> = new_bs.validators.to_vec();
        new_validators.remove(validator_idx);
        new_bs.validators = new_validators.into();
        let mut new_balances: Vec<u64> = new_bs.balances.to_vec();
        new_balances.remove(validator_idx);
        new_bs.balances = new_balances.into();
        new_bs
    };

    executor.set_bs_mutator(StateId::Slot(DEPLOY_SLOT), MODIFY_BEACON_BLOCK_HASH, mutator);
    executor.set_bs_mutator(StateId::Slot(target_slot), MODIFY_BEACON_BLOCK_HASH, mutator);

    let result = executor.run_test(target_slot).await;
    TestAssertions::assert_rejected_with(result, |e| {
        matches!(e, Sp1LidoAccountingReportContractErrors::BeaconBlockHashMismatch(_))
    })
}

// this one is currently impossible as the base state (at test_utils::DEPLOY_SLOT)
// had no validators in pending state
#[tokio::test(flavor = "multi_thread")]
#[ignore]
async fn data_tampering_omit_pending_lido_validator() -> Result<()> {
    let (mut executor, target_slot) = setup_executor().await?;
    let lido_creds = executor.lido_withdrawal_credentials();

    executor.set_bs_mutator(
        StateId::Slot(target_slot),
        MODIFY_BEACON_BLOCK_HASH,
        move |beacon_state| {
            let mut new_bs = beacon_state.clone();
            let epoch = new_bs.epoch();
            let all_lido_pending = all_validator_indices(&beacon_state, |validator| {
                validator.withdrawal_credentials == lido_creds
                    && validator.status(epoch) == ValidatorStatus::FutureDeposit
            });

            let validator_idx = all_lido_pending[1]; // picking the second pending validator
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
    TestAssertions::assert_rejected_with(result, |e| {
        matches!(e, Sp1LidoAccountingReportContractErrors::BeaconBlockHashMismatch(_))
    })
}

#[ignore]
#[tokio::test(flavor = "multi_thread")]
async fn data_tampering_balance_change_lido_validator_balance() -> Result<()> {
    let (mut executor, target_slot) = setup_executor().await?;
    let lido_creds = executor.lido_withdrawal_credentials();

    executor.set_bs_mutator(
        StateId::Slot(target_slot),
        MODIFY_BEACON_BLOCK_HASH,
        move |beacon_state| {
            let mut new_bs = beacon_state.clone();
            let is_lido_pred = is_lido(lido_creds);

            let validator_idx = positional_validator_indices(&beacon_state, &[0], is_lido_pred)[0];
            new_bs.balances[validator_idx] += 10;
            new_bs
        },
    );

    let result = executor.run_test(target_slot).await;
    TestAssertions::assert_rejected_with(result, |e| {
        matches!(e, Sp1LidoAccountingReportContractErrors::BeaconBlockHashMismatch(_))
    })
}

#[ignore]
#[tokio::test(flavor = "multi_thread")]
async fn data_tampering_balance_change_multi_lido_validator_balance() -> Result<()> {
    let (mut executor, target_slot) = setup_executor().await?;
    let lido_creds = executor.lido_withdrawal_credentials();

    executor.set_bs_mutator(
        StateId::Slot(target_slot),
        MODIFY_BEACON_BLOCK_HASH,
        move |beacon_state| {
            let mut new_bs = beacon_state.clone();
            let is_lido_pred = is_lido(lido_creds);

            let _adjust_idxs = positional_validator_indices(&beacon_state, &[0, 1, 3], is_lido_pred);
            for idx in _adjust_idxs {
                new_bs.balances[idx] = 0;
            }
            new_bs
        },
    );

    let result = executor.run_test(target_slot).await;
    TestAssertions::assert_rejected_with(result, |e| {
        matches!(e, Sp1LidoAccountingReportContractErrors::BeaconBlockHashMismatch(_))
    })
}

#[ignore]
#[tokio::test(flavor = "multi_thread")]
async fn data_tampering_balance_change_lido_validator_balance_cancel_out() -> Result<()> {
    let (mut executor, target_slot) = setup_executor().await?;
    let lido_creds = executor.lido_withdrawal_credentials();

    executor.set_bs_mutator(
        StateId::Slot(target_slot),
        MODIFY_BEACON_BLOCK_HASH,
        move |beacon_state| {
            let mut new_bs = beacon_state.clone();
            let is_lido_pred = is_lido(lido_creds);

            let indices_to_adjust = positional_validator_indices(&beacon_state, &[1, 3], is_lido_pred);
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
    TestAssertions::assert_rejected_with(result, |e| {
        matches!(e, Sp1LidoAccountingReportContractErrors::BeaconBlockHashMismatch(_))
    })
}

#[tokio::test(flavor = "multi_thread")]
async fn data_tampering_withdrawal_vault_tampered_balance() -> Result<()> {
    let (mut executor, target_slot) = setup_executor().await?;

    executor.set_withdrawal_vault_mutator(Box::new(|withdrawal_vault_data: WithdrawalVaultData| {
        let mut tampered_wvd = withdrawal_vault_data.clone();
        tampered_wvd.balance = tampered_wvd
            .balance
            .saturating_add(alloy_primitives::U256::from(10000000));
        tampered_wvd
    }));
    let result = executor.run_test(target_slot).await;
    TestAssertions::assert_failed_proof(result)
}

#[tokio::test(flavor = "multi_thread")]
async fn data_tampering_withdrawal_vault_tampered_proof() -> Result<()> {
    let (mut executor, target_slot) = setup_executor().await?;

    let other_slot = target_slot - 100;

    let other_slot_wv_data = executor.env.get_balance_proof(&StateId::Slot(other_slot)).await?;

    executor.set_withdrawal_vault_mutator(Box::new(move |wvd| {
        let mut tampered_wvd = wvd.clone();
        let other_account_proof = other_slot_wv_data.account_proof.clone();
        tampered_wvd.account_proof = other_account_proof;
        tampered_wvd
    }));
    let result = executor.run_test(target_slot).await;
    TestAssertions::assert_failed_proof(result)
}

#[tokio::test(flavor = "multi_thread")]
async fn data_tampering_withdrawal_vault_different_slot() -> Result<()> {
    let (mut executor, target_slot) = setup_executor().await?;

    let other_slot = target_slot - 100;

    let other_slot_wv_data = executor.env.get_balance_proof(&StateId::Slot(other_slot)).await?;

    executor.set_withdrawal_vault_mutator(Box::new(move |_wvd| other_slot_wv_data.clone()));
    let result = executor.run_test(target_slot).await;
    TestAssertions::assert_failed_proof(result)
}
