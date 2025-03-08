use alloy::node_bindings::{Anvil, AnvilInstance};
use sp1_lido_accounting_scripts::{
    beacon_state_reader::{BeaconStateReader, BeaconStateReaderEnum, StateId},
    consts::{NetworkConfig, NetworkInfo, WrappedNetwork},
    eth_client::{Contract, EthELClient, ProviderFactory, Sp1LidoAccountingReportContractWrapper},
    scripts::{self},
    sp1_client_wrapper::{SP1ClientWrapper, SP1ClientWrapperImpl},
};

use sp1_lido_accounting_zk_lib::{
    eth_consensus_layer::{BeaconBlockHeader, BeaconState},
    eth_spec,
    io::{
        eth_io::{BeaconChainSlot, HaveSlotWithBlock},
        program_io::WithdrawalVaultData,
    },
};
use std::{env, sync::Arc};
use typenum::Unsigned;

use crate::test_utils;

pub const RETRIES: usize = 3;

pub struct IntegrationTestEnvironment {
    // When going out of scope, AnvilInstance will terminate the anvil instance it corresponds to,
    // so test env need to assume ownership of anvil instance even if it doesn't use it
    #[allow(dead_code)]
    pub anvil: AnvilInstance,
    pub network: WrappedNetwork,
    pub bs_reader: Arc<BeaconStateReaderEnum>,
    pub eth_el_client: EthELClient,
    pub contract: Contract,
    pub sp1_client: &'static SP1ClientWrapperImpl,
    pub test_files: test_utils::files::TestFiles,
}

impl IntegrationTestEnvironment {
    pub async fn default() -> anyhow::Result<Self> {
        Self::new(test_utils::NETWORK.clone(), test_utils::DEPLOY_SLOT).await
    }

    pub async fn new(network: WrappedNetwork, deploy_slot: BeaconChainSlot) -> anyhow::Result<Self> {
        let bs_reader = BeaconStateReaderEnum::new_from_env(&network);

        let target_slot = Self::finalized_slot(&bs_reader).await?;
        let finalized_bs = Self::read_latest_bs_at_or_before(&bs_reader, target_slot, RETRIES).await?;
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

        let sp1_client = &test_utils::SP1_CLIENT;

        let provider = ProviderFactory::create_provider(anvil.keys()[0].clone(), anvil.endpoint().parse()?);
        let prov = Arc::new(provider);
        let eth_client = EthELClient::new(Arc::clone(&prov));

        let test_files = test_utils::files::TestFiles::new_from_manifest_dir();
        let deploy_bs: BeaconState = test_files
            .read_beacon_state(&StateId::Slot(deploy_slot))
            .await
            .map_err(test_utils::eyre_to_anyhow)?;
        let deploy_params = scripts::deploy::prepare_deploy_params(sp1_client.vk_bytes(), &deploy_bs, &network);

        log::info!("Deploying contract with parameters {:?}", deploy_params);
        let contract = Sp1LidoAccountingReportContractWrapper::deploy(Arc::clone(&prov), &deploy_params)
            .await
            .map_err(test_utils::eyre_to_anyhow)?;

        log::info!("Deployed contract at {}", contract.address());

        let instance = Self {
            anvil, // this needs to be here so that test executor assumes ownership of running anvil instance - otherwise it terminates right away
            network,
            bs_reader: Arc::new(bs_reader),
            eth_el_client: eth_client,
            contract,
            sp1_client: &test_utils::SP1_CLIENT,
            test_files: test_utils::files::TestFiles::new_from_manifest_dir(),
        };

        Ok(instance)
    }

    pub fn network_config(&self) -> NetworkConfig {
        self.network.get_config()
    }

    pub async fn finalized_slot(bs_reader: &BeaconStateReaderEnum) -> anyhow::Result<BeaconChainSlot> {
        let finalized_block_header = bs_reader.read_beacon_block_header(&StateId::Finalized).await?;
        Ok(finalized_block_header.bc_slot())
    }

    pub async fn get_finalized_slot(&self) -> anyhow::Result<BeaconChainSlot> {
        Self::finalized_slot(&self.bs_reader).await
    }

    pub async fn get_beacon_state(&self, state_id: &StateId) -> anyhow::Result<BeaconState> {
        let bs = self.bs_reader.read_beacon_state(state_id).await?;
        Ok(bs)
    }

    pub async fn get_balance_proof(&self, state_id: &StateId) -> anyhow::Result<WithdrawalVaultData> {
        let address = self.network_config().lido_withdrwawal_vault_address.into();
        let bs: BeaconState = self.get_beacon_state(state_id).await?;
        let execution_layer_block_hash = bs.latest_execution_payload_header.block_hash;
        let withdrawal_vault_data = self
            .eth_el_client
            .get_withdrawal_vault_data(address, execution_layer_block_hash)
            .await?;
        Ok(withdrawal_vault_data)
    }

    pub fn clone_reader(&self) -> Arc<BeaconStateReaderEnum> {
        Arc::clone(&self.bs_reader)
    }

    pub async fn read_beacon_block_header(&self, state_id: &StateId) -> anyhow::Result<BeaconBlockHeader> {
        self.bs_reader.read_beacon_block_header(state_id).await
    }

    pub async fn read_beacon_state(&self, state_id: &StateId) -> anyhow::Result<BeaconState> {
        self.bs_reader.read_beacon_state(state_id).await
    }

    pub async fn read_latest_bs_at_or_before(
        bs_reader: &impl BeaconStateReader,
        slot: BeaconChainSlot,
        retries: usize,
    ) -> anyhow::Result<BeaconState> {
        let step = eth_spec::SlotsPerEpoch::to_u64();
        let mut attempt = 0;
        let mut current_slot = slot;
        let result = loop {
            log::debug!("Fetching beacon state: attempt {attempt}, target slot {current_slot}");
            let try_bs = bs_reader.read_beacon_state(&StateId::Slot(current_slot)).await;

            if let Ok(beacon_state) = try_bs {
                break Ok(beacon_state);
            } else if attempt > retries {
                break try_bs;
            } else {
                attempt += 1;
                current_slot = BeaconChainSlot(current_slot.0 - step);
            }
        };
        result
    }
}
