mod test_utils;

use anyhow::{anyhow, Result};
use sp1_lido_accounting_scripts::{
    beacon_state_reader::{BeaconStateReader, BeaconStateReaderEnum, StateId},
    eth_client::{self, Sp1LidoAccountingReportContract::Sp1LidoAccountingReportContractErrors},
    scripts::shared as shared_logic,
    sp1_client_wrapper::SP1ClientWrapper,
};

use sp1_lido_accounting_zk_shared::{
    eth_consensus_layer::{BeaconState, BlsPublicKey, Hash256, Validator},
    io::{
        eth_io::{BeaconChainSlot, HaveEpoch},
        program_io::WithdrawalVaultData,
    },
};
use test_utils::{env::IntegrationTestEnvironment, mark_as_refslot, tampering_bs::TamperableBeaconStateReader};

type BeaconStateMutator = fn(BeaconState) -> BeaconState;
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
struct TestExecutor {
    pub env: IntegrationTestEnvironment,
    tampered_bs_reader: TamperableBeaconStateReader<BeaconStateReaderEnum, BeaconStateMutator>,
    withdrawal_vault_data_mutator: Box<WithdrawalVaultDataMutator>,
}

impl TestExecutor {
    async fn new() -> anyhow::Result<Self> {
        let env = IntegrationTestEnvironment::default().await?;
        let tampered_bs_reader = TamperableBeaconStateReader::new(env.clone_reader());

        let instance = Self {
            env,
            tampered_bs_reader,
            withdrawal_vault_data_mutator: Box::new(|wvd| wvd),
        };

        Ok(instance)
    }

    pub fn set_bs_mutator(
        &mut self,
        state_id: StateId,
        update_block_header: bool,
        mutator: BeaconStateMutator,
    ) -> &mut Self {
        self.tampered_bs_reader
            .set_mutator(state_id, update_block_header, mutator);
        self
    }

    pub fn set_withdrawal_vault_mutator(&mut self, mutator: Box<WithdrawalVaultDataMutator>) {
        self.withdrawal_vault_data_mutator = mutator;
    }

    async fn run_test(&self, target_slot: BeaconChainSlot) -> core::result::Result<(), TestError> {
        sp1_sdk::utils::setup_logger();
        let lido_withdrawal_credentials: Hash256 = self.env.network_config().lido_withdrawal_credentials.into();

        let reference_slot = mark_as_refslot(target_slot);
        let previous_slot = self.env.contract.get_latest_validator_state_slot().await?;

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

        log::info!("Preparing program input");
        let (program_input, _public_values) = shared_logic::prepare_program_input(
            reference_slot,
            &target_bs,
            &target_bh,
            &old_bs,
            &lido_withdrawal_credentials,
            tampered_withdrawal_vault_data,
            false,
        );
        log::info!("Requesting proof");
        let try_proof = self.env.sp1_client.prove(program_input);

        if let Err(e) = try_proof {
            return Err(TestError::ProofFailed(e));
        }

        log::info!("Generated proof");
        let proof = try_proof.unwrap();

        log::info!("Sending report");
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
                log::info!("Failed to create proof - as expected: {:?}", e);
                Ok(())
            }
            Err(other_error) => Err(anyhow!("Other error {:#?}", other_error)),
            Ok(_) => Err(anyhow!("Report accepted")),
        }
    }

    pub fn assert_rejected(&self, result: Result<(), TestError>) -> anyhow::Result<()> {
        match result {
            Err(TestError::ContractRejected(err)) => {
                log::info!("As expected, contract rejected {:#?}", err);
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

fn is_lido(validator: &Validator) -> bool {
    validator.withdrawal_credentials == *test_utils::LIDO_CREDS
}

fn is_non_lido(validator: &Validator) -> bool {
    !is_lido(validator)
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
    let executor = TestExecutor::new().await?;
    let target_slot = executor.env.get_finalized_slot().await?;

    let result = executor.run_test(target_slot).await;
    executor.assert_accepted(result)
}

#[ignore]
#[tokio::test(flavor = "multi_thread")]
async fn data_tampering_add_active_lido_validator() -> Result<()> {
    let mut executor = TestExecutor::new().await?;
    let target_slot = executor.env.get_finalized_slot().await?;
    executor.set_bs_mutator(StateId::Slot(target_slot), MODIFY_BEACON_BLOCK_HASH, |beacon_state| {
        let balance: u64 = 32_000_000_000;
        let new_validator = Validator {
            pubkey: BlsPublicKey::from([0_u8; 48].to_vec()),
            withdrawal_credentials: *test_utils::LIDO_CREDS,
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
async fn data_tampering_add_pending_lido_validator() -> Result<()> {
    let mut executor = TestExecutor::new().await?;
    let target_slot = executor.env.get_finalized_slot().await?;
    executor.set_bs_mutator(StateId::Slot(target_slot), MODIFY_BEACON_BLOCK_HASH, |beacon_state| {
        let balance: u64 = 1_000_000_000;
        let new_validator = Validator {
            pubkey: BlsPublicKey::from([0_u8; 48].to_vec()),
            withdrawal_credentials: *test_utils::LIDO_CREDS,
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
    });

    let result = executor.run_test(target_slot).await;
    executor.assert_rejected(result)
}

#[ignore]
#[tokio::test(flavor = "multi_thread")]
async fn data_tampering_add_exited_lido_validator() -> Result<()> {
    let mut executor = TestExecutor::new().await?;
    let target_slot = executor.env.get_finalized_slot().await?;
    executor.set_bs_mutator(StateId::Slot(target_slot), MODIFY_BEACON_BLOCK_HASH, |beacon_state| {
        let balance: u64 = 1_000_000_000;
        let new_validator = Validator {
            pubkey: BlsPublicKey::from([0_u8; 48].to_vec()),
            withdrawal_credentials: *test_utils::LIDO_CREDS,
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
    });

    let result = executor.run_test(target_slot).await;
    executor.assert_rejected(result)
}

#[ignore]
#[tokio::test(flavor = "multi_thread")]
async fn data_tampering_add_active_non_lido_validator() -> Result<()> {
    let mut executor = TestExecutor::new().await?;
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
    let mut executor = TestExecutor::new().await?;
    let target_slot = executor.env.get_finalized_slot().await?;
    executor.set_bs_mutator(StateId::Slot(target_slot), MODIFY_BEACON_BLOCK_HASH, |beacon_state| {
        let mut new_bs = beacon_state.clone();

        let validator_idx = validator_indices(&beacon_state, &[0], is_lido)[0];
        let mut new_validators: Vec<Validator> = new_bs.validators.to_vec();
        new_validators.remove(validator_idx);
        new_bs.validators = new_validators.into();
        let mut new_balances: Vec<u64> = new_bs.balances.to_vec();
        new_balances.remove(validator_idx);
        new_bs.balances = new_balances.into();
        new_bs
    });

    let result = executor.run_test(target_slot).await;
    executor.assert_failed_proof(result)
}

#[ignore]
#[tokio::test(flavor = "multi_thread")]
async fn data_tampering_remove_multi_lido_validator() -> Result<()> {
    let mut executor = TestExecutor::new().await?;
    let target_slot = executor.env.get_finalized_slot().await?;
    executor.set_bs_mutator(StateId::Slot(target_slot), MODIFY_BEACON_BLOCK_HASH, |beacon_state| {
        let mut new_bs = beacon_state.clone();

        let remove_idxs = validator_indices(&beacon_state, &[0, 1, 3], is_lido);
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
    });

    let result = executor.run_test(target_slot).await;
    executor.assert_failed_proof(result)
}

#[ignore]
#[tokio::test(flavor = "multi_thread")]
async fn data_tampering_change_lido_to_non_lido_validator() -> Result<()> {
    let mut executor = TestExecutor::new().await?;
    let target_slot = executor.env.get_finalized_slot().await?;
    executor.set_bs_mutator(StateId::Slot(target_slot), MODIFY_BEACON_BLOCK_HASH, |beacon_state| {
        let mut new_bs = beacon_state.clone();

        let validator_idx = validator_indices(&beacon_state, &[0], is_lido)[0];
        new_bs.validators[validator_idx].withdrawal_credentials = [0u8; 32].into();
        new_bs
    });

    let result = executor.run_test(target_slot).await;
    executor.assert_failed_proof(result)
}

#[ignore]
#[tokio::test(flavor = "multi_thread")]
async fn data_tampering_change_non_lido_to_lido_validator() -> Result<()> {
    let mut executor = TestExecutor::new().await?;
    let target_slot = executor.env.get_finalized_slot().await?;
    executor.set_bs_mutator(StateId::Slot(target_slot), MODIFY_BEACON_BLOCK_HASH, |beacon_state| {
        let mut new_bs = beacon_state.clone();

        let validator_idx = validator_indices(&beacon_state, &[0], is_non_lido)[0];
        new_bs.validators[validator_idx].withdrawal_credentials = *test_utils::LIDO_CREDS;
        new_bs
    });

    let result = executor.run_test(target_slot).await;
    executor.assert_failed_proof(result)
}

#[ignore]
#[tokio::test(flavor = "multi_thread")]
async fn data_tampering_change_lido_make_exited() -> Result<()> {
    let mut executor = TestExecutor::new().await?;
    let target_slot = executor.env.get_finalized_slot().await?;
    executor.set_bs_mutator(StateId::Slot(target_slot), MODIFY_BEACON_BLOCK_HASH, |beacon_state| {
        let mut new_bs = beacon_state.clone();

        let validator_idx = validator_indices(&beacon_state, &[0], is_lido)[0];
        new_bs.validators[validator_idx].exit_epoch = new_bs.epoch() - 10;
        new_bs
    });

    let result = executor.run_test(target_slot).await;
    executor.assert_rejected(result)
}

#[ignore]
#[tokio::test(flavor = "multi_thread")]
async fn data_tampering_omit_new_deposited_lido_validator() -> Result<()> {
    let mut executor = TestExecutor::new().await?;
    let target_slot = executor.env.get_finalized_slot().await?;
    executor.set_bs_mutator(StateId::Slot(target_slot), MODIFY_BEACON_BLOCK_HASH, |beacon_state| {
        let mut new_bs = beacon_state.clone();
        // old state https://sepolia.beaconcha.in/slot/5832096 had only 1 validator - all others are now "added"
        let added_deposited_idx = 3;
        let validator_idx = validator_indices(&beacon_state, &[added_deposited_idx], is_lido)[0];
        let mut new_validators: Vec<Validator> = new_bs.validators.to_vec();
        new_validators.remove(validator_idx);
        new_bs.validators = new_validators.into();
        let mut new_balances: Vec<u64> = new_bs.balances.to_vec();
        new_balances.remove(validator_idx);
        new_bs.balances = new_balances.into();
        new_bs
    });

    let result = executor.run_test(target_slot).await;
    executor.assert_rejected(result)
}

#[ignore]
#[tokio::test(flavor = "multi_thread")]
async fn data_tampering_omit_exited_lido_validator() -> Result<()> {
    let mut executor = TestExecutor::new().await?;
    let target_slot = executor.env.get_finalized_slot().await?;
    executor.set_bs_mutator(StateId::Slot(target_slot), MODIFY_BEACON_BLOCK_HASH, |beacon_state| {
        let mut new_bs = beacon_state.clone();
        // old state https://sepolia.beaconcha.in/slot/5832096 had only 1 validator - all others are now "added"
        let added_exited_idx = 3;
        let validator_idx = validator_indices(&beacon_state, &[added_exited_idx], is_lido)[0];
        let mut new_validators: Vec<Validator> = new_bs.validators.to_vec();
        new_validators.remove(validator_idx);
        new_bs.validators = new_validators.into();
        let mut new_balances: Vec<u64> = new_bs.balances.to_vec();
        new_balances.remove(validator_idx);
        new_bs.balances = new_balances.into();
        new_bs
    });

    let result = executor.run_test(target_slot).await;
    executor.assert_rejected(result)
}

// this one is currently impossible as the base state (at test_utils::DEPLOY_SLOT)
// had no validators in pending state
#[tokio::test(flavor = "multi_thread")]
#[ignore]
async fn data_tampering_omit_pending_lido_validator() -> Result<()> {
    Ok(())
}

#[ignore]
#[tokio::test(flavor = "multi_thread")]
async fn data_tampering_balance_change_lido_validator_balance() -> Result<()> {
    let mut executor = TestExecutor::new().await?;
    let target_slot = executor.env.get_finalized_slot().await?;
    executor.set_bs_mutator(StateId::Slot(target_slot), MODIFY_BEACON_BLOCK_HASH, |beacon_state| {
        let mut new_bs = beacon_state.clone();

        let validator_idx = validator_indices(&beacon_state, &[0], is_lido)[0];
        new_bs.balances[validator_idx] += 10;
        new_bs
    });

    let result = executor.run_test(target_slot).await;
    executor.assert_rejected(result)
}

#[ignore]
#[tokio::test(flavor = "multi_thread")]
async fn data_tampering_balance_change_multi_lido_validator_balance() -> Result<()> {
    let mut executor = TestExecutor::new().await?;
    let target_slot = executor.env.get_finalized_slot().await?;
    executor.set_bs_mutator(StateId::Slot(target_slot), MODIFY_BEACON_BLOCK_HASH, |beacon_state| {
        let mut new_bs = beacon_state.clone();

        let _adjust_idxs = validator_indices(&beacon_state, &[0, 1, 3], is_lido);
        for idx in _adjust_idxs {
            new_bs.balances[idx] = 0;
        }
        new_bs
    });

    let result = executor.run_test(target_slot).await;
    executor.assert_rejected(result)
}

#[ignore]
#[tokio::test(flavor = "multi_thread")]
async fn data_tampering_balance_change_lido_validator_balance_cancel_out() -> Result<()> {
    let mut executor = TestExecutor::new().await?;
    let target_slot = executor.env.get_finalized_slot().await?;
    executor.set_bs_mutator(StateId::Slot(target_slot), MODIFY_BEACON_BLOCK_HASH, |beacon_state| {
        let mut new_bs = beacon_state.clone();

        let indices_to_adjust = validator_indices(&beacon_state, &[1, 3], is_lido);
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
    });

    let result = executor.run_test(target_slot).await;
    executor.assert_rejected(result)
}

#[ignore]
#[tokio::test(flavor = "multi_thread")]
async fn data_tampering_withdrawal_vault_tampered_balance() -> Result<()> {
    let mut executor = TestExecutor::new().await?;
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
    let mut executor = TestExecutor::new().await?;
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
    let mut executor = TestExecutor::new().await?;
    let target_slot = executor.env.get_finalized_slot().await?;
    let other_slot = target_slot - 100;

    let other_slot_wv_data = executor.env.get_balance_proof(&StateId::Slot(other_slot)).await?;

    executor.set_withdrawal_vault_mutator(Box::new(move |_wvd| other_slot_wv_data.clone()));
    let result = executor.run_test(target_slot).await;
    executor.assert_failed_proof(result)
}
