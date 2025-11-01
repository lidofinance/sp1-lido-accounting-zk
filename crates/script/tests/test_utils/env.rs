#![allow(dead_code)]
use alloy::{
    eips::eip4788::{BEACON_ROOTS_ADDRESS, BEACON_ROOTS_CODE},
    node_bindings::{Anvil, AnvilInstance},
    providers::Provider,
    sol,
};
use alloy_primitives::{Address, U256};
use anyhow::anyhow;
use sp1_lido_accounting_scripts::{
    beacon_state_reader::{
        file::FileBeaconStateWriter, reqwest::CachedReqwestBeaconStateReader, BeaconStateReader, StateId,
    },
    consts::{NetworkConfig, NetworkInfo, WrappedNetwork},
    deploy::prepare_deploy_params,
    eth_client::{
        DefaultProvider, EthELClient, HashConsensusContractWrapper, ProviderFactory,
        Sp1LidoAccountingReportContractWrapper,
    },
    prometheus_metrics::Metrics,
    scripts::{
        self,
        prelude::{
            BeaconStateReaderEnum, EthInfrastructure, Flags, LidoInfrastructure, LidoSettings, Sp1Infrastructure,
        },
    },
    sp1_client_wrapper::{SP1ClientWrapper, SP1ClientWrapperImpl},
    tracing as tracing_config,
};

use hex_literal::hex;
use sp1_lido_accounting_zk_shared::{
    eth_consensus_layer::{BeaconBlockHeader, BeaconState, Hash256, Slot, Validator},
    eth_spec,
    io::{
        eth_io::{BeaconChainSlot, HaveEpoch, HaveSlotWithBlock},
        program_io::WithdrawalVaultData,
    },
};
use sp1_sdk::{network::FulfillmentStrategy, ProverClient};
use std::{
    env,
    io::{BufRead, BufReader},
    path::PathBuf,
    sync::Arc,
};
use tempfile::TempDir;
use tree_hash::TreeHash;
use typenum::Unsigned;

use crate::test_utils::{self, adjustments::Adjuster, validator, DEPLOY_SLOT};
use lazy_static::lazy_static;

pub const RETRIES: usize = 3;
const SUPPRESS_LOGS: bool = false;
const FORWARD_ANVIL_LOGS: bool = !SUPPRESS_LOGS && false;

sol!(
    #[allow(missing_docs)]
    #[sol(rpc)]
    #[derive(Debug)]
    BeaconRootsMock,
    "../../test_contracts/out/BeaconRootsMock.sol/BeaconRootsMock.json",
);

lazy_static! {
    pub static ref METRICS: Arc<Metrics> = Arc::new(Metrics::new("irrelevant"));
}

lazy_static! {
    pub static ref SP1_CLIENT: Arc<SP1ClientWrapperImpl> = {
        tracing::warn!("Initializing SP1 Client");
        Arc::new(SP1ClientWrapperImpl::new(
            ProverClient::builder().network().build(),
            FulfillmentStrategy::Hosted,
            METRICS.services.sp1_client.clone(),
        ))
    };
}

fn create_validators_and_balances(
    count: usize,
    withdrawal_credentials: Hash256,
    status: validator::Status,
    balance: u64,
) -> (Vec<Validator>, Vec<u64>) {
    let validators = (0..count)
        .map(|_| validator::make(withdrawal_credentials, status.clone(), balance))
        .collect();
    let balances = vec![balance; count];
    (validators, balances)
}

pub struct IntegrationTestEnvironment {
    // When going out of scope, AnvilInstance will terminate the anvil instance it corresponds to,
    // so test env need to assume ownership of anvil instance even if it doesn't use it
    #[allow(dead_code)]
    pub anvil: AnvilInstance,
    pub script_runtime: scripts::prelude::ScriptRuntime,
    pub test_files: test_utils::files::TestFiles,
    temp_folders: Vec<TempDir>,
    file_writer: FileBeaconStateWriter,
    beacon_roots_mock: Option<BeaconRootsMock::BeaconRootsMockInstance<Arc<DefaultProvider>>>,
}

impl IntegrationTestEnvironment {
    pub async fn default() -> anyhow::Result<Self> {
        Self::new(test_utils::NETWORK.clone(), test_utils::DEPLOY_SLOT, None).await
    }

    pub async fn default_with_fork_slot(fork_bs_slot: BeaconChainSlot) -> anyhow::Result<Self> {
        Self::new(test_utils::NETWORK.clone(), test_utils::DEPLOY_SLOT, Some(fork_bs_slot)).await
    }

    fn parse_envs() -> anyhow::Result<(PathBuf, String, String, String, Address, Address)> {
        let file_store_location = PathBuf::from(env::var("BS_FILE_STORE")?);
        let rpc_endpoint = env::var("CONSENSUS_LAYER_RPC")?;
        let bs_endpoint = env::var("BEACON_STATE_RPC")?;
        let fork_url = env::var("INTEGRATION_TEST_FORK_URL")?;
        let verifier_address: Address = env::var("SP1_VERIFIER_ADDRESS")?.parse()?;
        let hash_consensus_address: Address = env::var("HASH_CONSENSUS_ADDRESS")?.parse()?;

        Ok((
            file_store_location,
            rpc_endpoint,
            bs_endpoint,
            fork_url,
            verifier_address,
            hash_consensus_address,
        ))
    }

    async fn start_anvil(
        fork_url: String,
        fork_block_number: u64,
        with_log_capture: bool,
    ) -> anyhow::Result<AnvilInstance> {
        tracing::info!(
            "Starting anvil: fork_block_number={}, fork_url={}",
            fork_block_number,
            fork_url
        );
        let anvil_builder = Anvil::new().fork(fork_url).fork_block_number(fork_block_number);

        let anvil = if with_log_capture {
            let mut anvil = anvil_builder.keep_stdout().try_spawn()?;
            let anvil_child_process = anvil.child_mut();

            if let Some(stdout) = anvil_child_process.stdout.take() {
                let reader_stdout = BufReader::new(stdout);
                std::thread::spawn(move || {
                    for line in reader_stdout.lines().map_while(Result::ok) {
                        println!("[anvil] {line}");
                    }
                });
            }

            if let Some(stderr) = anvil_child_process.stderr.take() {
                let reader_stderr = BufReader::new(stderr);
                std::thread::spawn(move || {
                    for line in reader_stderr.lines().map_while(Result::ok) {
                        println!("[anvil] {line}");
                    }
                });
            }
            anvil
        } else {
            anvil_builder.try_spawn()?
        };
        let port = anvil.port();
        tracing::debug!("Launched anvil at port {}", port);
        Ok(anvil)
    }

    pub async fn new(
        network: WrappedNetwork,
        deploy_slot: BeaconChainSlot,
        fork_bs_slot: Option<BeaconChainSlot>,
    ) -> anyhow::Result<Self> {
        if !SUPPRESS_LOGS {
            tracing_config::setup_logger(tracing_config::LoggingConfig::default_for_test());
        }

        let (file_store_location, rpc_endpoint, bs_endpoint, fork_url, verifier_address, hash_consensus_address) =
            Self::parse_envs()?;
        let temp_bs_folder = TempDir::new()?;
        let temp_bs_folder_path = temp_bs_folder.path();
        let test_files = test_utils::files::TestFiles::new_from_manifest_dir();

        let cached_reader = CachedReqwestBeaconStateReader::new(
            &rpc_endpoint,
            &bs_endpoint,
            &file_store_location,
            &[
                temp_bs_folder_path,                  // used for beacon state overrides
                test_files.beacon_states().as_path(), // for cached test data
            ],
            METRICS.services.beacon_state_client.clone(),
        )?;
        let beacon_state_reader = Arc::new(BeaconStateReaderEnum::RPCCached(cached_reader));
        let file_writer =
            FileBeaconStateWriter::new(temp_bs_folder_path, METRICS.services.beacon_state_client.clone())?;

        let bs_slot_fork = match fork_bs_slot {
            Some(bc_slot) => bc_slot,
            None => Self::finalized_slot(Arc::clone(&beacon_state_reader)).await?,
        };

        let fork_block_bs =
            Self::read_latest_bs_at_or_before(Arc::clone(&beacon_state_reader), bs_slot_fork, RETRIES).await?;
        let fork_el_block = fork_block_bs.latest_execution_payload_header().block_number + 2;

        let anvil = Self::start_anvil(fork_url, fork_el_block, FORWARD_ANVIL_LOGS).await?;

        tracing::info!("Initializing Eth client");
        let provider = Arc::new(ProviderFactory::create_provider(
            anvil.keys()[0].clone(),
            anvil.endpoint().parse()?,
        ));
        let eth_client = EthELClient::new(Arc::clone(&provider), METRICS.services.eth_client.clone());

        let deploy_bs: BeaconState = test_files
            .read_beacon_state(&StateId::Slot(deploy_slot))
            .await
            .map_err(test_utils::eyre_to_anyhow)?;

        // Sepolia values
        let withdrawal_vault_address = hex!("De7318Afa67eaD6d6bbC8224dfCe5ed6e4b86d76").into();
        let withdrawal_credentials = hex!("010000000000000000000000De7318Afa67eaD6d6bbC8224dfCe5ed6e4b86d76").into();

        let vkey = SP1_CLIENT.vk_bytes()?;

        let deploy_params = prepare_deploy_params(
            vkey,
            &deploy_bs,
            &network,
            verifier_address,
            withdrawal_vault_address,
            withdrawal_credentials,
            [1; 20].into(),
        );

        tracing::info!("Deploying contract with parameters {:?}", deploy_params);
        let report_contract = Sp1LidoAccountingReportContractWrapper::deploy(Arc::clone(&provider), &deploy_params)
            .await
            .map_err(test_utils::eyre_to_anyhow)?;

        let hash_consensus_contract = HashConsensusContractWrapper::new(
            Arc::clone(&provider),
            hash_consensus_address,
            METRICS.services.hash_consensus.clone(),
        );

        let lido_settings = LidoSettings {
            contract_address: report_contract.address().to_owned(),
            withdrawal_vault_address,
            withdrawal_credentials,
            hash_consensus_address,
        };

        tracing::info!("Deployed contract at {}", report_contract.address());

        let script_runtime = scripts::prelude::ScriptRuntime::new(
            EthInfrastructure {
                network,
                provider,
                eth_client,
                beacon_state_reader,
            },
            Sp1Infrastructure {
                sp1_client: Arc::clone(&SP1_CLIENT),
            },
            LidoInfrastructure {
                report_contract,
                hash_consensus_contract,
            },
            lido_settings,
            Arc::clone(&METRICS),
            Flags {
                dry_run: false,
                report_cycles: false,
            },
        );

        let instance = Self {
            anvil, // this needs to be here so that test executor assumes ownership of running anvil instance - otherwise it terminates right away
            script_runtime,
            test_files: test_utils::files::TestFiles::new_from_manifest_dir(),
            temp_folders: vec![temp_bs_folder],
            file_writer,
            beacon_roots_mock: None,
        };

        Ok(instance)
    }

    pub fn network_config(&self) -> NetworkConfig {
        self.script_runtime.network().get_config()
    }

    pub async fn finalized_slot(bs_reader: Arc<impl BeaconStateReader>) -> anyhow::Result<BeaconChainSlot> {
        let finalized_block_header = bs_reader.read_beacon_block_header(&StateId::Finalized).await?;
        Ok(finalized_block_header.bc_slot())
    }

    pub async fn get_finalized_slot(&self) -> anyhow::Result<BeaconChainSlot> {
        Self::finalized_slot(self.script_runtime.bs_reader()).await
    }

    pub async fn get_balance_proof(&self, state_id: &StateId) -> anyhow::Result<WithdrawalVaultData> {
        let address = self.script_runtime.lido_settings.withdrawal_vault_address;
        let bs: BeaconState = self.read_beacon_state(state_id).await?;
        let execution_layer_block_hash = bs.latest_execution_payload_header().block_hash;
        let withdrawal_vault_data = self
            .script_runtime
            .eth_infra
            .eth_client
            .get_withdrawal_vault_data(address, execution_layer_block_hash)
            .await?;
        Ok(withdrawal_vault_data)
    }

    pub async fn read_beacon_block_header(&self, state_id: &StateId) -> anyhow::Result<BeaconBlockHeader> {
        self.script_runtime.bs_reader().read_beacon_block_header(state_id).await
    }

    pub async fn read_beacon_state(&self, state_id: &StateId) -> anyhow::Result<BeaconState> {
        self.script_runtime.bs_reader().read_beacon_state(state_id).await
    }

    pub async fn stub_state(&self, beacon_state: &BeaconState, block_header: &BeaconBlockHeader) -> anyhow::Result<()> {
        let state_hash = beacon_state.tree_hash_root();
        assert_eq!(*beacon_state.slot(), block_header.slot);
        assert_eq!(state_hash, block_header.state_root);

        self.file_writer.write_beacon_state(beacon_state)?;
        self.file_writer.write_beacon_block_header(block_header)?;

        self.record_beacon_block_header(block_header).await?;
        Ok(())
    }

    async fn set_block_hash(&self, timestamp: U256, hash: Hash256) -> anyhow::Result<Hash256> {
        if let Some(beacon_roots_mock) = &self.beacon_roots_mock {
            let set_root_tx = beacon_roots_mock
                .setRoot(timestamp, hash)
                .send()
                .await?
                .get_receipt()
                .await?;

            tracing::warn!(
                "Stubbed hash, {hash:#?} for timestamp {timestamp} at {:#?}",
                set_root_tx.transaction_hash
            );

            let recorded = beacon_roots_mock.beacon_block_hashes(timestamp).call().await?;

            Ok(recorded)
        } else {
            panic!("BeaconRootsMock is not initialized, skipping setting block hash");
        }
    }

    pub async fn record_beacon_block_header(&self, block_header: &BeaconBlockHeader) -> anyhow::Result<()> {
        let block_hash = block_header.tree_hash_root();
        self.record_beacon_block_hash(block_header.slot, block_hash)
            .await
            .inspect(|_v| {
                tracing::info!(
                    "Stubbed state for slot {}, block_root: {:#?}, state_root: {:#?}",
                    block_header.slot,
                    block_hash,
                    block_header.state_root
                )
            })
    }

    pub async fn record_beacon_block_hash(&self, slot: Slot, beacon_block_hash: Hash256) -> anyhow::Result<()> {
        if let Some(beacon_roots_mock) = &self.beacon_roots_mock {
            let timestamp_for_block_exists_check =
                U256::from(self.network_config().genesis_block_timestamp + (slot * eth_spec::SecondsPerSlot::to_u64()));

            // +1 is EXTREMELY important - see comment above _findBeaconBlockHash in the Sp1LidoAccountingReportContract
            let timestamp_for_get_block_hash = U256::from(
                self.network_config().genesis_block_timestamp + ((slot + 1) * eth_spec::SecondsPerSlot::to_u64()),
            );

            tracing::debug!("Stubbing block hash for {slot}@{timestamp_for_get_block_hash} = {beacon_block_hash:#?}");

            let hash_at_block_timestamp = beacon_roots_mock
                .beacon_block_hashes(timestamp_for_block_exists_check)
                .call()
                .await?;

            let block_exists = hash_at_block_timestamp != [0; 32];
            if !block_exists {
                self.set_block_hash(timestamp_for_block_exists_check, test_utils::NONZERO_HASH.into())
                    .await?;
            }

            let recorded = self
                .set_block_hash(timestamp_for_get_block_hash, beacon_block_hash)
                .await?;

            tracing::info!("Recorded hash for slot {slot}, {recorded:#?}");
            Ok(())
        } else {
            Err(anyhow!("BeaconRootsMock is not initialized"))
        }
    }

    pub async fn mock_beacon_state_roots_contract(&mut self) -> anyhow::Result<()> {
        if self.beacon_roots_mock.is_some() {
            tracing::warn!("BeaconRootsMock is already initialized, skipping re-initialization");
            return Ok(());
        }
        tracing::info!("Replacing BEACON_STATE_ROOTS contract bytecode");
        let provider = Arc::clone(&self.script_runtime.eth_infra.provider);
        let old_bytecode = provider.get_code_at(BEACON_ROOTS_ADDRESS).latest().await?;
        assert_eq!(old_bytecode, BEACON_ROOTS_CODE);
        let _res: () = provider
            .raw_request(
                "anvil_setCode".into(),
                [
                    BEACON_ROOTS_ADDRESS.to_string(),
                    BeaconRootsMock::DEPLOYED_BYTECODE.to_string(),
                ],
            )
            .await?;

        let new_code = provider.get_code_at(BEACON_ROOTS_ADDRESS).latest().await?;
        assert_eq!(new_code, BeaconRootsMock::DEPLOYED_BYTECODE);
        tracing::debug!("New bytecode:\n{new_code:#}");
        tracing::info!("Replaced BEACON_STATE_ROOTS contract bytecode");
        let beacon_roots_mock_instance = BeaconRootsMock::new(BEACON_ROOTS_ADDRESS, provider);

        self.beacon_roots_mock = Some(beacon_roots_mock_instance);
        Ok(())
    }

    async fn read_latest_bs_at_or_before(
        bs_reader: Arc<impl BeaconStateReader>,
        slot: BeaconChainSlot,
        retries: usize,
    ) -> anyhow::Result<BeaconState> {
        let step = eth_spec::SlotsPerEpoch::to_u64();
        let mut attempt = 0;
        let mut current_slot = slot;
        loop {
            tracing::debug!("Fetching beacon state: attempt {attempt}, target slot {current_slot}");
            let try_bs = bs_reader.read_beacon_state(&StateId::Slot(current_slot)).await;

            if let Ok(beacon_state) = try_bs {
                break Ok(beacon_state);
            } else if attempt > retries {
                break try_bs;
            } else {
                attempt += 1;
                current_slot = BeaconChainSlot(current_slot.0 - step);
            }
        }
    }

    pub async fn make_adjustments(&mut self, target_slot: &BeaconChainSlot) -> anyhow::Result<AdjusterWrapper> {
        let original_bs = self.read_beacon_state(&StateId::Slot(DEPLOY_SLOT)).await?;
        let original_bh = self.read_beacon_block_header(&StateId::Slot(DEPLOY_SLOT)).await?;

        // Ensuring we have a diverse set of validators in different states to work with
        let wrapper = AdjusterWrapper::initialize(
            &original_bs,
            &original_bh,
            self.script_runtime.lido_settings.withdrawal_credentials,
            *target_slot,
        );

        if self.beacon_roots_mock.is_none() {
            tracing::info!("Initializing beacon state roots mock contract");
            self.mock_beacon_state_roots_contract().await?;
        }
        // Since we're mocking beacon state roots, we have to provide old hashes as well
        self.record_beacon_block_header(&original_bh).await?;

        Ok(wrapper)
    }

    pub async fn apply_standard_adjustments(&mut self, target_slot: &BeaconChainSlot) -> anyhow::Result<()> {
        let adjustments = self
            .make_adjustments(target_slot)
            .await?
            .add_lido_deposited(5)
            .add_lido_pending(2)
            .add_lido_exited(2)
            .add_other(3)
            .exit_lido(2);

        self.apply(adjustments).await?;
        Ok(())
    }

    pub async fn apply(&self, adjustments: AdjusterWrapper) -> anyhow::Result<()> {
        let (new_bs, new_bh) = adjustments.build();
        self.stub_state(&new_bs, &new_bh).await?;
        Ok(())
    }
}

pub struct AdjusterWrapper {
    adjuster: Adjuster,
    lido_credentials: Hash256,
    target_slot: BeaconChainSlot,
}

impl AdjusterWrapper {
    pub fn initialize(
        bs: &BeaconState,
        bh: &BeaconBlockHeader,
        lido_credentials: Hash256,
        target_slot: BeaconChainSlot,
    ) -> Self {
        let mut adjuster: Adjuster = Adjuster::start_with(bs, bh);
        adjuster.set_slot(&target_slot);
        AdjusterWrapper {
            adjuster,
            lido_credentials,
            target_slot,
        }
    }

    fn add_validators(mut self, validators: (Vec<Validator>, Vec<u64>)) -> Self {
        self.adjuster.add_validators(&validators.0, &validators.1);
        self
    }

    fn get_all_lido_indices(&self, validators: &[Validator]) -> Vec<usize> {
        validators
            .iter()
            .enumerate()
            .filter_map(|(index, validator)| {
                if validator.withdrawal_credentials == self.lido_credentials {
                    Some(index)
                } else {
                    None
                }
            })
            .collect()
    }

    fn beacon_state(&self) -> &BeaconState {
        &self.adjuster.beacon_state
    }

    pub fn add_lido_deposited(self, count: usize) -> Self {
        let validators = create_validators_and_balances(
            count,
            self.lido_credentials,
            validator::Status::Active(self.target_slot.epoch()),
            validator::DEP_BALANCE,
        );
        self.add_validators(validators)
    }

    pub fn add_lido_pending(self, count: usize) -> Self {
        let validators = create_validators_and_balances(
            count,
            self.lido_credentials,
            validator::Status::Pending(self.target_slot.epoch() + 2),
            0,
        );
        self.add_validators(validators)
    }

    pub fn add_lido_exited(self, count: usize) -> Self {
        let validators = create_validators_and_balances(
            count,
            self.lido_credentials,
            validator::Status::Exited {
                activated: self.target_slot.epoch() - 2,
                exited: self.target_slot.epoch() - 1,
            },
            validator::DEP_BALANCE,
        );
        self.add_validators(validators)
    }

    pub fn add_other(self, count: usize) -> Self {
        let validators = create_validators_and_balances(
            count,
            Hash256::random(),
            validator::Status::Active(self.target_slot.epoch()),
            validator::DEP_BALANCE,
        );
        self.add_validators(validators)
    }

    pub fn exit_lido(mut self, count: usize) -> Self {
        let bs = self.beacon_state();
        let validators = bs.validators().to_vec();
        let existing_validator_indices: Vec<usize> = self.get_all_lido_indices(&validators);

        let old_epoch = bs.epoch();
        let existing_non_exited: Vec<usize> = existing_validator_indices
            .into_iter()
            .filter(|index| {
                let validator = validators.get(*index).expect("Must exist");
                validator.exit_epoch >= old_epoch
            })
            .collect();

        assert!(
            existing_non_exited.len() >= count,
            "Need to have at least {count} existing validators, had {existing_non_exited:?}",
        );

        for to_exit in existing_non_exited.iter().take(count) {
            self.adjuster
                .change_validator(*to_exit, |val| val.exit_epoch = self.target_slot.epoch() - 1);
        }
        self
    }

    pub fn build(self) -> (BeaconState, BeaconBlockHeader) {
        self.adjuster.build()
    }
}
