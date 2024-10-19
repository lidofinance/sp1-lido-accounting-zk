use alloy::node_bindings::{Anvil, AnvilInstance};
use anyhow::{anyhow, Result};
use sp1_lido_accounting_scripts::{
    beacon_state_reader::{BeaconStateReader, BeaconStateReaderEnum, StateId},
    consts::{self, NetworkInfo},
    eth_client::{Contract, DefaultProvider, ProviderFactory, Sp1LidoAccountingReportContractWrapper},
    scripts::{self, shared as shared_logic},
    sp1_client_wrapper::{SP1ClientWrapper, SP1ClientWrapperImpl},
};

use sp1_lido_accounting_zk_shared::eth_consensus_layer::{BeaconState, BlsPublicKey, Hash256, Validator};
use sp1_sdk::ProverClient;
use std::env;
use test_utils::{TamperableBeaconStateReader, TestFiles};
mod test_utils;

type BeaconStateMutator = fn(BeaconState) -> BeaconState;

fn eyre_to_anyhow(err: eyre::Error) -> anyhow::Error {
    anyhow!("Eyre error: {:#?}", err)
}

struct TestExecutor<'a> {
    main_bs_reader: &'a BeaconStateReaderEnum,
    tampered_bs_reader: TamperableBeaconStateReader<'a, BeaconStateReaderEnum, BeaconStateMutator>,
    client: SP1ClientWrapperImpl,
}

impl<'a> TestExecutor<'a> {
    fn new(bs_reader: &'a BeaconStateReaderEnum) -> Self {
        let client = SP1ClientWrapperImpl::new(ProverClient::network(), consts::ELF);
        let tampered_bs: TamperableBeaconStateReader<BeaconStateReaderEnum, BeaconStateMutator> =
            TamperableBeaconStateReader::new(bs_reader);

        Self {
            main_bs_reader: bs_reader,
            tampered_bs_reader: tampered_bs,
            client,
        }
    }

    pub fn add_mutator(&mut self, state_id: StateId, mutator: BeaconStateMutator) -> &mut Self {
        self.tampered_bs_reader.add_mutator(state_id, mutator);
        self
    }

    async fn get_target_slot(&self) -> u64 {
        let finalized_block_header = self
            .main_bs_reader
            .read_beacon_block_header(&StateId::Finalized)
            .await
            .expect("Failed to read finalized block"); // todo: this should be just ?, but anyhow and eyre seems not to get along for some reason
        finalized_block_header.slot
    }

    async fn start_anvil(&self, target_slot: u64) -> Result<AnvilInstance> {
        let finalized_bs =
            test_utils::read_latest_bs_at_or_before(self.main_bs_reader, target_slot, test_utils::RETRIES)
                .await
                .map_err(eyre_to_anyhow)?;
        let fork_url = env::var("FORK_URL").expect("FORK_URL env var must be specified");
        let anvil = Anvil::new()
            .fork(fork_url)
            .fork_block_number(finalized_bs.latest_execution_payload_header.block_number + 2)
            .try_spawn()?;
        Ok(anvil)
    }

    async fn deploy_contract(&self, provider: DefaultProvider) -> Result<Contract> {
        let test_files = TestFiles::new_from_manifest_dir();
        let deploy_bs: BeaconState = test_files
            .read_beacon_state(&StateId::Slot(test_utils::DEPLOY_SLOT))
            .await
            .map_err(eyre_to_anyhow)?;
        let deploy_params =
            scripts::deploy::prepare_deploy_params(self.client.vk_bytes(), &deploy_bs, &test_utils::NETWORK);

        log::info!("Deploying contract with parameters {:?}", deploy_params);
        let contract = Sp1LidoAccountingReportContractWrapper::deploy(provider.clone(), &deploy_params)
            .await
            .map_err(eyre_to_anyhow)?;
        log::info!("Deployed contract at {}", contract.address());
        Ok(contract)
    }

    async fn run_test(&self) -> Result<()> {
        sp1_sdk::utils::setup_logger();
        let network = &test_utils::NETWORK;
        let target_slot = self.get_target_slot().await;
        let anvil = self.start_anvil(target_slot).await?;
        let provider = ProviderFactory::create_provider(anvil.keys()[0].clone(), anvil.endpoint().parse()?);
        let contract = self.deploy_contract(provider).await?;

        let previous_slot = contract.get_latest_report_slot().await?;

        let lido_withdrawal_credentials: Hash256 = network.get_config().lido_withdrawal_credentials.into();

        let target_bh = self
            .tampered_bs_reader
            .read_beacon_block_header(&StateId::Slot(target_slot))
            .await?;
        let target_bs = self
            .tampered_bs_reader
            .read_beacon_state(&StateId::Slot(target_slot))
            .await?;
        // Want to read old state from untampered reader, so the old state compute will match
        let old_bs = self
            .main_bs_reader
            .read_beacon_state(&StateId::Slot(previous_slot))
            .await?;

        let (program_input, public_values) =
            shared_logic::prepare_program_input(&target_bs, &target_bh, &old_bs, &lido_withdrawal_credentials, false);
        let proof = self.client.prove(program_input).expect("Failed to generate proof");
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
            Ok(_txhash) => Err(anyhow!("Sumbission should have failed, but succeeded")),
            Err(err) => {
                log::info!("As expected, submission failed with {:#?}", err);
                Ok(())
            }
        }
    }
}

fn find_nth_lido_validator_index(bs: &BeaconState, lido_withdrawal_credentials: &Hash256, n: usize) -> Option<usize> {
    bs.validators
        .iter()
        .enumerate()
        .filter_map(|pair| {
            let (index, validator) = pair;
            if validator.withdrawal_credentials == *lido_withdrawal_credentials {
                Some(index)
            } else {
                None
            }
        })
        .nth(n)
}

#[tokio::test(flavor = "multi_thread")]
async fn add_active_lido_validator_fails() -> Result<()> {
    let network = &test_utils::NETWORK;

    let bs_reader = BeaconStateReaderEnum::new_from_env(network);
    let mut executor = TestExecutor::new(&bs_reader);
    let target_slot = executor.get_target_slot().await;
    executor.add_mutator(StateId::Slot(target_slot), |beacon_state| {
        // TODO: capturing anything makes rust unhappy - cannot coerce closure into function pointer
        let network = &test_utils::NETWORK;
        let creds: Hash256 = network.get_config().lido_withdrawal_credentials.into();

        let balance: u64 = 32_000_000_000;
        let new_validator = Validator {
            pubkey: BlsPublicKey::from([0_u8; 48].to_vec()),
            withdrawal_credentials: creds,
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

#[tokio::test(flavor = "multi_thread")]
async fn make_non_lido_validator_fails() -> Result<()> {
    let network = &test_utils::NETWORK;

    let bs_reader = BeaconStateReaderEnum::new_from_env(network);
    let mut executor = TestExecutor::new(&bs_reader);
    let target_slot = executor.get_target_slot().await;
    executor.add_mutator(StateId::Slot(target_slot), |beacon_state| {
        let network = &test_utils::NETWORK;
        let creds: Hash256 = network.get_config().lido_withdrawal_credentials.into();
        let validator_idx = find_nth_lido_validator_index(&beacon_state, &creds, 0).unwrap();

        let mut new_bs = beacon_state.clone();

        new_bs.validators[validator_idx].withdrawal_credentials = [0u8; 32].into();
        new_bs
    });

    executor.run_test().await
}
