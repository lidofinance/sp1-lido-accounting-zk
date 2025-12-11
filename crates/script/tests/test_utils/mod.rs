#![allow(dead_code)]
use anyhow::anyhow;
use hex_literal::hex;
use sp1_lido_accounting_scripts::consts::{Network, WrappedNetwork};
use sp1_lido_accounting_scripts::eth_client::Sp1LidoAccountingReportContract::Sp1LidoAccountingReportContractErrors;
use sp1_lido_accounting_scripts::{eth_client, sp1_client_wrapper};
use sp1_lido_accounting_zk_shared::eth_consensus_layer::{BeaconState, Validator};
use sp1_lido_accounting_zk_shared::eth_consensus_layer::{BeaconStateFields, BeaconStatePrecomputedHashes, Hash256};
use sp1_lido_accounting_zk_shared::io::eth_io::{BeaconChainSlot, ReferenceSlot};

pub mod env;
pub mod files;

pub static NETWORK: WrappedNetwork = WrappedNetwork::Anvil(Network::Hoodi);
pub const DEPLOY_SLOT: BeaconChainSlot = BeaconChainSlot(1871356);
pub const REPORT_COMPUTE_SLOT: BeaconChainSlot = BeaconChainSlot(1890000);

pub const ZERO_HASH: [u8; 32] = [0; 32];
pub const NONZERO_HASH: [u8; 32] = hex!("0101010101010101010101010101010101010101010101010101010101010101");

pub fn eyre_to_anyhow(err: eyre::Error) -> anyhow::Error {
    anyhow!("Eyre error: {:#?}", err)
}

// This function not OK to use it outside tests. Don't copy-paste.
// In short:
// * Only a few slots will be reference slots (one a day)
// * Not all reference slots will actually have block in them
#[cfg(test)]
pub fn mark_as_refslot(slot: BeaconChainSlot) -> ReferenceSlot {
    ReferenceSlot(slot.0)
}

pub fn set_validators(bs: &mut BeaconState, new_validators: Vec<Validator>) {
    match bs {
        BeaconState::Electra(inner_bs) => inner_bs.validators = new_validators.into(),
        BeaconState::Fulu(inner_bs) => inner_bs.validators = new_validators.into(),
    }
}

pub fn set_balances(bs: &mut BeaconState, new_balances: Vec<u64>) {
    match bs {
        BeaconState::Electra(inner_bs) => inner_bs.balances = new_balances.into(),
        BeaconState::Fulu(inner_bs) => inner_bs.balances = new_balances.into(),
    }
}

pub fn set_slot(bs: &mut BeaconState, new_slot: u64) {
    match bs {
        BeaconState::Electra(inner_bs) => inner_bs.slot = new_slot,
        BeaconState::Fulu(inner_bs) => inner_bs.slot = new_slot,
    }
}

pub mod adjustments {
    use super::set_slot;
    use sp1_lido_accounting_zk_shared::{
        eth_consensus_layer::{BeaconBlockHeader, BeaconState, Validator},
        io::eth_io::BeaconChainSlot,
    };
    use tree_hash::TreeHash;

    pub struct Adjuster {
        pub beacon_state: BeaconState,
        pub block_header: BeaconBlockHeader,
    }

    impl Adjuster {
        pub fn start_with(beacon_state: &BeaconState, block_header: &BeaconBlockHeader) -> Self {
            Self {
                beacon_state: beacon_state.clone(),
                block_header: block_header.clone(),
            }
        }

        pub fn set_slot(&mut self, slot: &BeaconChainSlot) -> &mut Self {
            set_slot(&mut self.beacon_state, slot.0);
            self.block_header.slot = slot.0;
            self
        }

        pub fn add_validator(&mut self, validator: Validator, balance: u64) -> &mut Self {
            self.beacon_state
                .validators_mut()
                .push(validator)
                .expect("Too many validators");
            self.beacon_state
                .balances_mut()
                .push(balance)
                .expect("Too many balances");
            self
        }

        pub fn add_validators(&mut self, validators: &[Validator], balances: &[u64]) -> &mut Self {
            assert_eq!(
                validators.len(),
                balances.len(),
                "Validators and balances length mismatch"
            );
            for (validator, balance) in validators.iter().zip(balances.iter()) {
                self.add_validator(validator.clone(), *balance);
            }
            self
        }

        pub fn set_validator(&mut self, index: usize, validator: Validator) -> &mut Self {
            self.beacon_state.validators_mut()[index] = validator;
            self
        }

        pub fn change_validator(&mut self, index: usize, modifier: impl FnOnce(&mut Validator)) -> &mut Self {
            modifier(&mut self.beacon_state.validators_mut()[index]);
            self
        }

        pub fn set_balance(&mut self, index: usize, balance: u64) -> &mut Self {
            self.beacon_state.balances_mut()[index] = balance;
            self
        }

        pub fn build(mut self) -> (BeaconState, BeaconBlockHeader) {
            self.block_header.state_root = self.beacon_state.tree_hash_root();
            (self.beacon_state, self.block_header)
        }
    }
}

pub mod validator {
    use rand::Rng;
    use sp1_lido_accounting_zk_shared::{
        eth_consensus_layer::*,
        io::eth_io::{BeaconChainSlot, HaveEpoch},
    };

    #[derive(Clone, Debug, PartialEq)]
    pub enum Status {
        Pending(u64),
        Active(u64),
        Exited { activated: u64, exited: u64 },
    }

    impl Status {
        pub fn pending(slot: BeaconChainSlot) -> Self {
            Self::Pending(slot.epoch())
        }
        pub fn active(activation_slot: BeaconChainSlot) -> Self {
            Self::Active(activation_slot.epoch())
        }
        pub fn exited(activation_slot: BeaconChainSlot, exit_slot: BeaconChainSlot) -> Self {
            Self::Exited {
                activated: activation_slot.epoch(),
                exited: exit_slot.epoch(),
            }
        }
    }

    pub fn random_pubkey(prefix: Option<&[u8]>) -> BlsPublicKey {
        let mut pubkey = [0u8; 48];
        let mut rng = rand::rng();

        // Fill with random bytes
        rng.fill(&mut pubkey);

        // Overwrite with prefix if provided
        if let Some(p) = prefix {
            let len = p.len().min(48);
            pubkey[..len].copy_from_slice(&p[..len]);
        }
        pubkey.to_vec().into()
    }

    pub const DEP_BALANCE: u64 = 32_000_000_000; // 32 ETH in Gwei

    pub fn make(withdrawal_credentials: WithdrawalCredentials, status: Status, balance: u64) -> Validator {
        let (activation_eligibility_epoch, activation_epoch, exit_epoch) = match status {
            Status::Pending(epoch) => (epoch, u64::MAX, u64::MAX),
            Status::Active(activated) => (activated - 2, activated - 1, u64::MAX),
            Status::Exited { activated, exited } => (activated - 2, activated - 1, exited),
        };

        Validator {
            pubkey: random_pubkey(None),
            withdrawal_credentials,
            effective_balance: balance,
            slashed: false,
            activation_eligibility_epoch,
            activation_epoch,
            exit_epoch,
            withdrawable_epoch: activation_epoch,
        }
    }
}

pub fn set_bs_field(bs: &mut BeaconStatePrecomputedHashes, field: &BeaconStateFields, value: Hash256) {
    match field {
        BeaconStateFields::genesis_time => bs.genesis_time = value,
        BeaconStateFields::genesis_validators_root => bs.genesis_validators_root = value,
        BeaconStateFields::slot => bs.slot = value,
        BeaconStateFields::fork => bs.fork = value,
        BeaconStateFields::latest_block_header => bs.latest_block_header = value,
        BeaconStateFields::block_roots => bs.block_roots = value,
        BeaconStateFields::state_roots => bs.state_roots = value,
        BeaconStateFields::historical_roots => bs.historical_roots = value,
        BeaconStateFields::eth1_data => bs.eth1_data = value,
        BeaconStateFields::eth1_data_votes => bs.eth1_data_votes = value,
        BeaconStateFields::eth1_deposit_index => bs.eth1_deposit_index = value,
        BeaconStateFields::validators => bs.validators = value,
        BeaconStateFields::balances => bs.balances = value,
        BeaconStateFields::randao_mixes => bs.randao_mixes = value,
        BeaconStateFields::slashings => bs.slashings = value,
        BeaconStateFields::previous_epoch_participation => bs.previous_epoch_participation = value,
        BeaconStateFields::current_epoch_participation => bs.current_epoch_participation = value,
        BeaconStateFields::justification_bits => bs.justification_bits = value,
        BeaconStateFields::previous_justified_checkpoint => bs.previous_justified_checkpoint = value,
        BeaconStateFields::current_justified_checkpoint => bs.current_justified_checkpoint = value,
        BeaconStateFields::finalized_checkpoint => bs.finalized_checkpoint = value,
        BeaconStateFields::inactivity_scores => bs.inactivity_scores = value,
        BeaconStateFields::current_sync_committee => bs.current_sync_committee = value,
        BeaconStateFields::next_sync_committee => bs.next_sync_committee = value,
        BeaconStateFields::latest_execution_payload_header => bs.latest_execution_payload_header = value,
        BeaconStateFields::next_withdrawal_index => bs.next_withdrawal_index = value,
        BeaconStateFields::next_withdrawal_validator_index => bs.next_withdrawal_validator_index = value,
        BeaconStateFields::historical_summaries => bs.historical_summaries = value,
        BeaconStateFields::deposit_requests_start_index => bs.deposit_requests_start_index = value,
        BeaconStateFields::deposit_balance_to_consume => bs.deposit_balance_to_consume = value,
        BeaconStateFields::exit_balance_to_consume => bs.exit_balance_to_consume = value,
        BeaconStateFields::earliest_exit_epoch => bs.earliest_exit_epoch = value,
        BeaconStateFields::consolidation_balance_to_consume => bs.consolidation_balance_to_consume = value,
        BeaconStateFields::earliest_consolidation_epoch => bs.earliest_consolidation_epoch = value,
        BeaconStateFields::pending_deposits => bs.pending_deposits = value,
        BeaconStateFields::pending_partial_withdrawals => bs.pending_partial_withdrawals = value,
        BeaconStateFields::pending_consolidations => bs.pending_consolidations = value,
        BeaconStateFields::proposer_lookahead => bs.proposer_lookahead = value,
    }
}

pub mod vecs {
    use rand::{seq::SliceRandom, Rng};

    fn vectors_equal<N: PartialEq>(left: &[N], right: &[N]) -> bool {
        if left.len() != right.len() {
            return false;
        }
        left.iter().zip(right.iter()).all(|(l, r)| l == r)
    }

    pub fn append<Elem>(mut input: Vec<Elem>, element: Elem) -> Vec<Elem> {
        input.push(element);
        input
    }

    pub fn duplicate<Elem: Clone>(input: Vec<Elem>, index: usize) -> Vec<Elem> {
        let elem = input[index].clone();
        append(input, elem)
    }

    pub fn duplicate_random<Elem: Clone>(input: Vec<Elem>) -> Vec<Elem> {
        let duplicate_idx = rand::rng().random_range(0..input.len());
        duplicate(input, duplicate_idx)
    }

    pub fn modify<Elem: Clone>(mut input: Vec<Elem>, index: usize, modifier: impl Fn(Elem) -> Elem) -> Vec<Elem> {
        let new_val = modifier(input[index].clone());
        input[index] = new_val;
        input
    }

    pub fn modify_random<Elem: Clone>(input: Vec<Elem>, modifier: impl Fn(Elem) -> Elem) -> Vec<Elem> {
        let modify_idx = rand::rng().random_range(0..input.len());
        modify(input, modify_idx, modifier)
    }

    pub fn remove<Elem>(mut input: Vec<Elem>, index: usize) -> Vec<Elem> {
        assert!(!input.is_empty(), "Removing from empty vec leaves the input intact");
        input.remove(index);
        input
    }

    pub fn remove_random<Elem>(input: Vec<Elem>) -> Vec<Elem> {
        let remove_idx = rand::rng().random_range(0..input.len());
        remove(input, remove_idx)
    }

    pub fn ensured_shuffle<N: Clone + PartialEq>(input: &[N]) -> Vec<N> {
        assert!(input.len() > 1); // no point shuffling a single element
        let mut new = input.to_vec();
        let mut rng = rand::rng();
        while vectors_equal(&new, input) {
            new.shuffle(&mut rng);
        }
        new
    }

    pub fn ensured_shuffle_keep_first<N: Clone + PartialEq>(input: &[N]) -> Vec<N> {
        assert!(input.len() > 2); // no point shuffling a single element
        let mut new = input.to_vec();
        new.splice(1.., ensured_shuffle(&new[1..]));
        new
    }
}

pub mod varlists {
    use rand::Rng;
    use sp1_lido_accounting_zk_shared::eth_consensus_layer::VariableList;

    use super::vecs;

    pub fn append<Elem: Clone, Size: typenum::Unsigned>(
        mut input: VariableList<Elem, Size>,
        element: Elem,
    ) -> VariableList<Elem, Size> {
        input.push(element).expect("Error: must not fail");
        input
    }

    pub fn duplicate<Elem: Clone, Size: typenum::Unsigned>(
        input: VariableList<Elem, Size>,
        index: usize,
    ) -> VariableList<Elem, Size> {
        let elem = input[index].clone();
        append(input, elem)
    }

    pub fn duplicate_random<Elem: Clone, Size: typenum::Unsigned>(
        input: VariableList<Elem, Size>,
    ) -> VariableList<Elem, Size> {
        let duplicate_idx = rand::rng().random_range(0..input.len());
        duplicate(input, duplicate_idx)
    }

    pub fn modify<Elem: Clone, Size: typenum::Unsigned>(
        mut input: VariableList<Elem, Size>,
        index: usize,
        modifier: fn(Elem) -> Elem,
    ) -> VariableList<Elem, Size> {
        input[index] = modifier(input[index].clone());
        input
    }

    pub fn modify_random<Elem: Clone, Size: typenum::Unsigned>(
        input: VariableList<Elem, Size>,
        modifier: fn(Elem) -> Elem,
    ) -> VariableList<Elem, Size> {
        let modify_idx = rand::rng().random_range(0..input.len());
        modify(input, modify_idx, modifier)
    }

    pub fn remove<Elem: Clone, Size: typenum::Unsigned>(
        input: VariableList<Elem, Size>,
        index: usize,
    ) -> VariableList<Elem, Size> {
        let as_vec = input.to_vec();
        vecs::remove(as_vec, index).into()
    }

    pub fn remove_random<Elem: Clone, Size: typenum::Unsigned>(
        input: VariableList<Elem, Size>,
    ) -> VariableList<Elem, Size> {
        let remove_idx = rand::rng().random_range(0..input.len());
        remove(input, remove_idx)
    }

    pub fn ensured_shuffle<Elem: Clone + PartialEq, Size: typenum::Unsigned>(
        input: VariableList<Elem, Size>,
    ) -> VariableList<Elem, Size> {
        let as_vec = input.to_vec();
        vecs::ensured_shuffle(&as_vec).into()
    }

    pub fn ensured_shuffle_keep_first<Elem: Clone + PartialEq, Size: typenum::Unsigned>(
        input: VariableList<Elem, Size>,
    ) -> VariableList<Elem, Size> {
        let as_vec = input.to_vec();
        vecs::ensured_shuffle_keep_first(&as_vec).into()
    }
}

#[derive(Debug, thiserror::Error)]
pub enum TestError {
    #[error("Proof rejected for known reason {0:?}")]
    ContractRejected(Sp1LidoAccountingReportContractErrors),
    #[error("Proof rejected for other reasons {0:?}")]
    OtherRejection(eth_client::ContractError),
    #[error("Failed to build proof {0:?}")]
    ProofFailed(sp1_client_wrapper::Error),
    #[error("Other eyre error {0:?}")]
    OtherEyre(#[from] eyre::Error),
    #[error("Other anyhow error {0:?}")]
    OtherAnyhow(#[from] anyhow::Error),
}

impl From<eth_client::ContractError> for TestError {
    fn from(value: eth_client::ContractError) -> Self {
        match value {
            eth_client::ContractError::Rejection(e) => TestError::ContractRejected(e),
            other => TestError::OtherRejection(other),
        }
    }
}

pub struct TestAssertions;
impl TestAssertions {
    pub fn assert_accepted<T>(result: Result<T, TestError>) -> anyhow::Result<()> {
        match result {
            Ok(_) => {
                tracing::info!("As expected, contract accepted");
                Ok(())
            }
            Err(TestError::ContractRejected(err)) => Err(anyhow!("Contract rejected {:#?}", err)),
            Err(other_error) => Err(anyhow!("Other error {:#?}", other_error)),
        }
    }

    pub fn assert_rejected<T>(result: Result<T, TestError>) -> anyhow::Result<()> {
        match result {
            Err(TestError::ContractRejected(err)) => {
                tracing::info!("As expected, contract rejected {:#?}", err);
                Ok(())
            }
            Err(other_error) => Err(anyhow!("Other error {:#?}", other_error)),
            Ok(_txhash) => Err(anyhow!("Report accepted")),
        }
    }

    pub fn assert_rejected_with<T, F>(result: Result<T, TestError>, check: F) -> anyhow::Result<()>
    where
        F: FnOnce(&Sp1LidoAccountingReportContractErrors) -> bool,
    {
        match result {
            Err(TestError::ContractRejected(ref err)) if check(err) => {
                tracing::info!("As expected, contract rejected {:#?}", err);
                Ok(())
            }
            Err(TestError::ContractRejected(err)) => Err(anyhow!("Unexpected rejection type: {:#?}", err)),
            Err(other_error) => Err(anyhow!("Other error {:#?}", other_error)),
            Ok(_) => Err(anyhow!("Expected rejection, but got acceptance")),
        }
    }

    pub fn assert_failed_proof<T>(result: Result<T, TestError>) -> anyhow::Result<()> {
        match result {
            Err(TestError::ProofFailed(e)) => {
                tracing::info!("Failed to create proof - as expected: {:?}", e);
                Ok(())
            }
            Err(other_error) => Err(anyhow!("Other error {:#?}", other_error)),
            Ok(_) => Err(anyhow!("Report accepted")),
        }
    }
}
