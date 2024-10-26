use alloy::node_bindings::{Anvil, AnvilInstance};
use anyhow::{anyhow, Result};
use sp1_lido_accounting_scripts::{
    beacon_state_reader::{BeaconStateReader, BeaconStateReaderEnum, StateId},
    consts::NetworkInfo,
    eth_client::{self, Contract, ProviderFactory, Sp1LidoAccountingReportContractWrapper},
    scripts::{self, shared as shared_logic},
    sp1_client_wrapper::{SP1ClientWrapper, SP1ClientWrapperImpl},
};

use sp1_lido_accounting_zk_shared::eth_consensus_layer::{BeaconState, BlsPublicKey, Hash256, Validator};
use std::env;
use test_utils::{eyre_to_anyhow, TamperableBeaconStateReader, TestFiles};
mod test_utils;

type BeaconStateMutator = fn(BeaconState) -> BeaconState;

struct TestExecutor<'a> {
    main_bs_reader: &'a BeaconStateReaderEnum,
    tampered_bs_reader: TamperableBeaconStateReader<'a, BeaconStateReaderEnum, BeaconStateMutator>,
    client: &'static SP1ClientWrapperImpl,
}

impl<'a> TestExecutor<'a> {
    fn new(bs_reader: &'a BeaconStateReaderEnum) -> Self {
        let tampered_bs: TamperableBeaconStateReader<BeaconStateReaderEnum, BeaconStateMutator> =
            TamperableBeaconStateReader::new(bs_reader);

        Self {
            main_bs_reader: bs_reader,
            tampered_bs_reader: tampered_bs,
            client: &test_utils::SP1_CLIENT,
        }
    }

    pub fn set_mutator(
        &mut self,
        state_id: StateId,
        update_block_header: bool,
        mutator: BeaconStateMutator,
    ) -> &mut Self {
        self.tampered_bs_reader
            .set_mutator(state_id, update_block_header, mutator);
        self
    }

    async fn get_target_slot(&self) -> Result<u64> {
        let finalized_block_header = self
            .main_bs_reader
            // .read_beacon_block_header(&StateId::Finalized)
            .read_beacon_block_header(&StateId::Slot(6138176))
            .await?;
        Ok(finalized_block_header.slot)
    }

    async fn start_anvil(&self, target_slot: u64) -> Result<AnvilInstance> {
        let finalized_bs =
            test_utils::read_latest_bs_at_or_before(self.main_bs_reader, target_slot, test_utils::RETRIES)
                .await
                .map_err(eyre_to_anyhow)?;
        let fork_url =
            env::var("INTEGRATION_TEST_FORK_URL").expect("INTEGRATION_TEST_FORK_URL env var must be specified");
        let fork_block_number = finalized_bs.latest_execution_payload_header.block_number + 2;
        log::debug!(
            "Starting anvil: fork_block_number={}, fork_url={}",
            fork_block_number,
            fork_url
        );
        let anvil = Anvil::new()
            .fork(fork_url)
            .fork_block_number(fork_block_number)
            .try_spawn()?;
        Ok(anvil)
    }

    async fn deploy_contract(&self, network: &impl NetworkInfo, anvil: &AnvilInstance) -> Result<Contract> {
        let provider = ProviderFactory::create_provider(anvil.keys()[0].clone(), anvil.endpoint().parse()?);

        let test_files = TestFiles::new_from_manifest_dir();
        let deploy_bs: BeaconState = test_files
            .read_beacon_state(&StateId::Slot(test_utils::DEPLOY_SLOT))
            .await
            .map_err(eyre_to_anyhow)?;
        let deploy_params = scripts::deploy::prepare_deploy_params(self.client.vk_bytes(), &deploy_bs, network);

        log::info!("Deploying contract with parameters {:?}", deploy_params);
        let contract = Sp1LidoAccountingReportContractWrapper::deploy(provider, &deploy_params)
            .await
            .map_err(eyre_to_anyhow)?;
        log::info!("Deployed contract at {}", contract.address());
        Ok(contract)
    }

    async fn run_test(&self) -> Result<()> {
        sp1_sdk::utils::setup_logger();
        let lido_withdrawal_credentials: Hash256 = test_utils::NETWORK.get_config().lido_withdrawal_credentials.into();

        let target_slot = self.get_target_slot().await?;
        // // Anvil needs to be here in scope for the duration of the test, otherwise it terminates
        // // Hence creating it here (i.e. owner is this function) and passing down to deploy conract
        let anvil = self.start_anvil(target_slot).await?;
        let contract = self.deploy_contract(&test_utils::NETWORK, &anvil).await?;
        let previous_slot = contract.get_latest_report_slot().await?;

        let target_bh = self
            .tampered_bs_reader
            .read_beacon_block_header(&StateId::Slot(target_slot))
            .await?;
        let target_bs = self
            .tampered_bs_reader
            .read_beacon_state(&StateId::Slot(target_slot))
            .await?;
        // Should read old state from untampered reader, so the old state compute will match
        let old_bs = self
            .main_bs_reader
            .read_beacon_state(&StateId::Slot(previous_slot))
            .await?;
        log::info!("Preparing program input");
        let (program_input, public_values) =
            shared_logic::prepare_program_input(&target_bs, &target_bh, &old_bs, &lido_withdrawal_credentials, false);
        log::info!("Requesting proof");
        let try_proof = self.client.prove(program_input);
        match try_proof {
            Err(_) => {
                log::info!("Failed to create proof - this is equivalent to failing verification");
                Ok(())
            }
            Ok(proof) => {
                log::info!("Generated proof");

                log::info!("Sending report");
                let result = contract
                    .submit_report_data(
                        target_bs.slot,
                        public_values.report,
                        public_values.metadata,
                        proof.bytes(),
                        proof.public_values.to_vec(),
                    )
                    .await;

                match result {
                    Err(eth_client::Error::Rejection(err)) => {
                        log::info!("As expected, contract rejected {:#?}", err);
                        Ok(())
                    }
                    Err(other_err) => Err(anyhow!(
                        "Submission failed due to technical reasons - inconclusive outcome {:#?}",
                        other_err
                    )),
                    Ok(_txhash) => Err(anyhow!("Report accepted")),
                }
            }
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
*/

// The attacker might approach data tampering from two angles:
// 1. Tamper only beacon block state, leaving beacon block header alone
// 2. Tamper both state and header
// The first scenario leads to beacon_state.tree_hash_root != beacon_block_header.state_root
// and is rejected by the program (i.e. it won't even get to generating the report)
// Hence setting this to true is the only option to actually test end-to-end
// But this is kept here for an easy check that this is the case in all the scenarios listed below
// Flipping this to false should cause all tests to panic
const MODIFY_BEACON_BLOCK_HASH: bool = true;

// Note: these tests will hit the prover network - will have relatively longer run
// time (1-2 minutes) and also incur proving costs.
#[ignore]
#[tokio::test(flavor = "multi_thread")]
async fn tampering_add_active_lido_validator() -> Result<()> {
    let bs_reader = BeaconStateReaderEnum::new_from_env(&test_utils::NETWORK);
    let mut executor = TestExecutor::new(&bs_reader);
    let target_slot = executor.get_target_slot().await?;
    executor.set_mutator(StateId::Slot(target_slot), MODIFY_BEACON_BLOCK_HASH, |beacon_state| {
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

    executor.run_test().await
}

#[ignore]
#[tokio::test(flavor = "multi_thread")]
async fn tampering_add_pending_lido_validator() -> Result<()> {
    let network = &test_utils::NETWORK;

    let bs_reader = BeaconStateReaderEnum::new_from_env(network);
    let mut executor = TestExecutor::new(&bs_reader);
    let target_slot = executor.get_target_slot().await?;
    executor.set_mutator(StateId::Slot(target_slot), MODIFY_BEACON_BLOCK_HASH, |beacon_state| {
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

    executor.run_test().await
}

#[ignore]
#[tokio::test(flavor = "multi_thread")]
async fn tampering_add_exited_lido_validator() -> Result<()> {
    let network = &test_utils::NETWORK;

    let bs_reader = BeaconStateReaderEnum::new_from_env(network);
    let mut executor = TestExecutor::new(&bs_reader);
    let target_slot = executor.get_target_slot().await?;
    executor.set_mutator(StateId::Slot(target_slot), MODIFY_BEACON_BLOCK_HASH, |beacon_state| {
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

    executor.run_test().await
}

#[ignore]
#[tokio::test(flavor = "multi_thread")]
async fn tampering_add_active_non_lido_validator() -> Result<()> {
    let bs_reader = BeaconStateReaderEnum::new_from_env(&test_utils::NETWORK);
    let mut executor = TestExecutor::new(&bs_reader);
    let target_slot = executor.get_target_slot().await?;
    executor.set_mutator(StateId::Slot(target_slot), MODIFY_BEACON_BLOCK_HASH, |beacon_state| {
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

    executor.run_test().await
}

#[ignore]
#[tokio::test(flavor = "multi_thread")]
async fn tampering_remove_lido_validator() -> Result<()> {
    let bs_reader = BeaconStateReaderEnum::new_from_env(&test_utils::NETWORK);
    let mut executor = TestExecutor::new(&bs_reader);
    let target_slot = executor.get_target_slot().await?;
    executor.set_mutator(StateId::Slot(target_slot), MODIFY_BEACON_BLOCK_HASH, |beacon_state| {
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

    executor.run_test().await
}

#[ignore]
#[tokio::test(flavor = "multi_thread")]
async fn tampering_remove_multi_lido_validator() -> Result<()> {
    let bs_reader = BeaconStateReaderEnum::new_from_env(&test_utils::NETWORK);
    let mut executor = TestExecutor::new(&bs_reader);
    let target_slot = executor.get_target_slot().await?;
    executor.set_mutator(StateId::Slot(target_slot), MODIFY_BEACON_BLOCK_HASH, |beacon_state| {
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

    executor.run_test().await
}

#[ignore]
#[tokio::test(flavor = "multi_thread")]
async fn tampering_change_lido_to_non_lido_validator() -> Result<()> {
    let bs_reader = BeaconStateReaderEnum::new_from_env(&test_utils::NETWORK);
    let mut executor = TestExecutor::new(&bs_reader);
    let target_slot = executor.get_target_slot().await?;
    executor.set_mutator(StateId::Slot(target_slot), MODIFY_BEACON_BLOCK_HASH, |beacon_state| {
        let mut new_bs = beacon_state.clone();

        let validator_idx = validator_indices(&beacon_state, &[0], is_lido)[0];
        new_bs.validators[validator_idx].withdrawal_credentials = [0u8; 32].into();
        new_bs
    });

    executor.run_test().await
}

#[ignore]
#[tokio::test(flavor = "multi_thread")]
async fn tampering_change_non_lido_to_lido_validator() -> Result<()> {
    let bs_reader = BeaconStateReaderEnum::new_from_env(&test_utils::NETWORK);
    let mut executor = TestExecutor::new(&bs_reader);
    let target_slot = executor.get_target_slot().await?;
    executor.set_mutator(StateId::Slot(target_slot), MODIFY_BEACON_BLOCK_HASH, |beacon_state| {
        let mut new_bs = beacon_state.clone();

        let validator_idx = validator_indices(&beacon_state, &[0], is_non_lido)[0];
        new_bs.validators[validator_idx].withdrawal_credentials = *test_utils::LIDO_CREDS;
        new_bs
    });

    executor.run_test().await
}

#[ignore]
#[tokio::test(flavor = "multi_thread")]
async fn tampering_change_lido_make_exited() -> Result<()> {
    let bs_reader = BeaconStateReaderEnum::new_from_env(&test_utils::NETWORK);
    let mut executor = TestExecutor::new(&bs_reader);
    let target_slot = executor.get_target_slot().await?;
    executor.set_mutator(StateId::Slot(target_slot), MODIFY_BEACON_BLOCK_HASH, |beacon_state| {
        let mut new_bs = beacon_state.clone();

        let validator_idx = validator_indices(&beacon_state, &[0], is_lido)[0];
        new_bs.validators[validator_idx].exit_epoch = new_bs.epoch() - 10;
        new_bs
    });

    executor.run_test().await
}

#[ignore]
#[tokio::test(flavor = "multi_thread")]
async fn tampering_omit_added_in_deposited_state_lido_validator() -> Result<()> {
    let bs_reader = BeaconStateReaderEnum::new_from_env(&test_utils::NETWORK);
    let mut executor = TestExecutor::new(&bs_reader);
    let target_slot = executor.get_target_slot().await?;
    executor.set_mutator(StateId::Slot(target_slot), MODIFY_BEACON_BLOCK_HASH, |beacon_state| {
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

    executor.run_test().await
}

#[ignore]
#[tokio::test(flavor = "multi_thread")]
async fn tampering_omit_exited_lido_validator() -> Result<()> {
    let bs_reader = BeaconStateReaderEnum::new_from_env(&test_utils::NETWORK);
    let mut executor = TestExecutor::new(&bs_reader);
    let target_slot = executor.get_target_slot().await?;
    executor.set_mutator(StateId::Slot(target_slot), MODIFY_BEACON_BLOCK_HASH, |beacon_state| {
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

    executor.run_test().await
}

// this one is currently impossible as the base state (at test_utils::DEPLOY_SLOT)
// had no validators in "deposited, but not active" state
#[tokio::test(flavor = "multi_thread")]
#[ignore]
async fn tampering_omit_activated_lido_validator() -> Result<()> {
    Ok(())
}

#[ignore]
#[tokio::test(flavor = "multi_thread")]
async fn tampering_balance_change_lido_validator_balance() -> Result<()> {
    let bs_reader = BeaconStateReaderEnum::new_from_env(&test_utils::NETWORK);
    let mut executor = TestExecutor::new(&bs_reader);
    let target_slot = executor.get_target_slot().await?;
    executor.set_mutator(StateId::Slot(target_slot), MODIFY_BEACON_BLOCK_HASH, |beacon_state| {
        let mut new_bs = beacon_state.clone();

        let validator_idx = validator_indices(&beacon_state, &[0], is_lido)[0];
        new_bs.balances[validator_idx] += 10;
        new_bs
    });

    executor.run_test().await
}

#[ignore]
#[tokio::test(flavor = "multi_thread")]
async fn tampering_balance_change_multi_lido_validator_balance() -> Result<()> {
    let bs_reader = BeaconStateReaderEnum::new_from_env(&test_utils::NETWORK);
    let mut executor = TestExecutor::new(&bs_reader);
    let target_slot = executor.get_target_slot().await?;
    executor.set_mutator(StateId::Slot(target_slot), MODIFY_BEACON_BLOCK_HASH, |beacon_state| {
        let mut new_bs = beacon_state.clone();

        let _adjust_idxs = validator_indices(&beacon_state, &[0, 1, 3], is_lido);
        for idx in _adjust_idxs {
            new_bs.balances[idx] = 0;
        }
        new_bs
    });

    executor.run_test().await
}

#[ignore]
#[tokio::test(flavor = "multi_thread")]
async fn tampering_balance_change_lido_validator_balance_cancel_out() -> Result<()> {
    let bs_reader = BeaconStateReaderEnum::new_from_env(&test_utils::NETWORK);
    let mut executor = TestExecutor::new(&bs_reader);
    let target_slot = executor.get_target_slot().await?;
    executor.set_mutator(StateId::Slot(target_slot), MODIFY_BEACON_BLOCK_HASH, |beacon_state| {
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

    executor.run_test().await
}
