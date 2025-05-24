use alloy::node_bindings::{Anvil, AnvilInstance};
use sp1_lido_accounting_scripts::{
    beacon_state_reader::{BeaconStateReader, StateId},
    consts::{self, NetworkConfig, NetworkInfo, WrappedNetwork},
    deploy::prepare_deploy_params,
    eth_client::{EthELClient, HashConsensusContractWrapper, ProviderFactory, Sp1LidoAccountingReportContractWrapper},
    scripts::{self, prelude::BeaconStateReaderEnum},
    sp1_client_wrapper::{SP1ClientWrapper, SP1ClientWrapperImpl},
};

use sp1_lido_accounting_zk_shared::{
    eth_consensus_layer::{BeaconBlockHeader, BeaconState},
    eth_spec,
    io::{
        eth_io::{BeaconChainSlot, HaveSlotWithBlock},
        program_io::WithdrawalVaultData,
    },
};
use sp1_sdk::ProverClient;
use std::{env, sync::Arc};
use typenum::Unsigned;

use crate::test_utils;

pub const RETRIES: usize = 3;

pub struct IntegrationTestEnvironment {
    // When going out of scope, AnvilInstance will terminate the anvil instance it corresponds to,
    // so test env need to assume ownership of anvil instance even if it doesn't use it
    #[allow(dead_code)]
    pub anvil: AnvilInstance,
    pub script_runtime: scripts::prelude::ScriptRuntime,
    pub test_files: test_utils::files::TestFiles,
}

impl IntegrationTestEnvironment {
    pub async fn default() -> anyhow::Result<Self> {
        Self::new(test_utils::NETWORK.clone(), test_utils::DEPLOY_SLOT).await
    }

    pub async fn new(network: WrappedNetwork, deploy_slot: BeaconChainSlot) -> anyhow::Result<Self> {
        let beacon_state_reader = BeaconStateReaderEnum::new_from_env(&network)
            .map_err(|e| anyhow::anyhow!("Failed to create beacon state reader {e:?}"))?;

        let target_slot = Self::finalized_slot(&beacon_state_reader).await?;
        let finalized_bs = Self::read_latest_bs_at_or_before(&beacon_state_reader, target_slot, RETRIES).await?;
        let fork_url =
            env::var("INTEGRATION_TEST_FORK_URL").expect("INTEGRATION_TEST_FORK_URL env var must be specified");
        let fork_block_number = finalized_bs.latest_execution_payload_header.block_number + 2;
        tracing::debug!(
            "Starting anvil: fork_block_number={}, fork_url={}",
            fork_block_number,
            fork_url
        );
        let anvil = Anvil::new()
            .fork(fork_url)
            .fork_block_number(fork_block_number)
            .try_spawn()?;

        let sp1_client = SP1ClientWrapperImpl::new(ProverClient::from_env());

        let provider = Arc::new(ProviderFactory::create_provider(
            anvil.keys()[0].clone(),
            anvil.endpoint().parse()?,
        ));
        let eth_client = EthELClient::new(Arc::clone(&provider));

        let test_files = test_utils::files::TestFiles::new_from_manifest_dir();
        let deploy_bs: BeaconState = test_files
            .read_beacon_state(&StateId::Slot(deploy_slot))
            .await
            .map_err(test_utils::eyre_to_anyhow)?;
        let deploy_params = prepare_deploy_params(sp1_client.vk_bytes(), &deploy_bs, &network);

        tracing::info!("Deploying contract with parameters {:?}", deploy_params);
        let contract = Sp1LidoAccountingReportContractWrapper::deploy(Arc::clone(&provider), &deploy_params)
            .await
            .map_err(test_utils::eyre_to_anyhow)?;

        let hash_consensus_contract = HashConsensusContractWrapper::new(
            Arc::clone(&provider),
            network.get_config().lido_accounting_hash_consensus_contract.into(),
        );

        tracing::info!("Deployed contract at {}", contract.address());

        let script_runtime = scripts::prelude::ScriptRuntime::new(
            network,
            provider,
            sp1_client,
            beacon_state_reader,
            eth_client,
            contract,
            hash_consensus_contract,
        );

        let instance = Self {
            anvil, // this needs to be here so that test executor assumes ownership of running anvil instance - otherwise it terminates right away
            script_runtime,
            test_files: test_utils::files::TestFiles::new_from_manifest_dir(),
        };

        Ok(instance)
    }

    pub fn network_config(&self) -> NetworkConfig {
        self.script_runtime.network().get_config()
    }

    pub async fn finalized_slot(bs_reader: &impl BeaconStateReader) -> anyhow::Result<BeaconChainSlot> {
        let finalized_block_header = bs_reader.read_beacon_block_header(&StateId::Finalized).await?;
        Ok(finalized_block_header.bc_slot())
    }

    pub async fn get_finalized_slot(&self) -> anyhow::Result<BeaconChainSlot> {
        Self::finalized_slot(self.script_runtime.bs_reader()).await
    }

    pub async fn get_beacon_state(&self, state_id: &StateId) -> anyhow::Result<BeaconState> {
        let bs = self.script_runtime.bs_reader().read_beacon_state(state_id).await?;
        Ok(bs)
    }

    pub async fn get_balance_proof(&self, state_id: &StateId) -> anyhow::Result<WithdrawalVaultData> {
        let address = self.network_config().lido_withdrwawal_vault_address.into();
        let bs: BeaconState = self.get_beacon_state(state_id).await?;
        let execution_layer_block_hash = bs.latest_execution_payload_header.block_hash;
        let withdrawal_vault_data = self
            .script_runtime
            .eth_client
            .get_withdrawal_vault_data(address, execution_layer_block_hash)
            .await?;
        Ok(withdrawal_vault_data)
    }

    pub fn bs_reader(&self) -> &impl BeaconStateReader {
        self.script_runtime.bs_reader()
    }

    pub async fn read_beacon_block_header(&self, state_id: &StateId) -> anyhow::Result<BeaconBlockHeader> {
        self.script_runtime.bs_reader().read_beacon_block_header(state_id).await
    }

    pub async fn read_beacon_state(&self, state_id: &StateId) -> anyhow::Result<BeaconState> {
        self.script_runtime.bs_reader().read_beacon_state(state_id).await
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
        };
        result
    }
}
