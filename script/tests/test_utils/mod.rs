use anyhow::anyhow;
use hex_literal::hex;
use lazy_static::lazy_static;
use sp1_lido_accounting_scripts::consts::{self, Network, NetworkInfo, WrappedNetwork};
use sp1_lido_accounting_scripts::sp1_client_wrapper::SP1ClientWrapperImpl;
use sp1_lido_accounting_zk_shared::eth_consensus_layer::{
    BeaconStateFields, BeaconStatePrecomputedHashes, Epoch, Hash256, Validator,
};
use sp1_lido_accounting_zk_shared::io::eth_io::{BeaconChainSlot, ReferenceSlot};
use sp1_sdk::ProverClient;

pub mod env;
pub mod files;

pub static NETWORK: WrappedNetwork = WrappedNetwork::Anvil(Network::Sepolia);
pub const DEPLOY_SLOT: BeaconChainSlot = BeaconChainSlot(7643456);

pub const REPORT_COMPUTE_SLOT: BeaconChainSlot = BeaconChainSlot(7696704);

// TODO: Enable local prover if/when it becomes feasible.
// In short, local proving with groth16 seems to not really work at the moment -
// get stuck at generating proof with ~100% CPU utilization for ~40 minutes.
// This makes local prover impractical - network takes ~5-10 minutes to finish
// #[cfg(not(feature = "test_network_prover"))]
// lazy_static! {
//     pub static ref SP1_CLIENT: SP1ClientWrapperImpl = SP1ClientWrapperImpl::new(ProverClient::local());
// }
// #[cfg(feature = "test_network_prover")]
lazy_static! {
    pub static ref SP1_CLIENT: SP1ClientWrapperImpl = SP1ClientWrapperImpl::new(ProverClient::from_env());
}

lazy_static! {
    pub static ref LIDO_CREDS: Hash256 = NETWORK.get_config().lido_withdrawal_credentials.into();
}

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

pub fn make_validator(current_epoch: Epoch, balance: u64) -> Validator {
    let activation_eligibility_epoch: u64 = current_epoch - 10;
    let activation_epoch: u64 = current_epoch - 5;
    let exit_epoch: u64 = u64::MAX;
    let withdrawable_epoch: u64 = current_epoch - 3;
    let bls_key: Vec<u8> =
        hex!("0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef").into();

    Validator {
        pubkey: bls_key.into(),
        withdrawal_credentials: Hash256::random(),
        effective_balance: balance,
        slashed: false,
        activation_eligibility_epoch,
        activation_epoch,
        exit_epoch,
        withdrawable_epoch,
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

    pub fn duplicate<Elem: Clone>(mut input: Vec<Elem>, index: usize) -> Vec<Elem> {
        let elem = input[index].clone();
        append(input, elem)
    }

    pub fn duplicate_random<Elem: Clone>(mut input: Vec<Elem>) -> Vec<Elem> {
        let duplicate_idx = rand::rng().random_range(0..input.len());
        duplicate(input, duplicate_idx)
    }

    pub fn modify<Elem: Clone>(mut input: Vec<Elem>, index: usize, modifier: impl Fn(Elem) -> Elem) -> Vec<Elem> {
        let new_val = modifier(input[index].clone());
        input[index] = new_val;
        input
    }

    pub fn modify_random<Elem: Clone>(mut input: Vec<Elem>, modifier: impl Fn(Elem) -> Elem) -> Vec<Elem> {
        let modify_idx = rand::rng().random_range(0..input.len());
        modify(input, modify_idx, modifier)
    }

    pub fn remove<Elem>(mut input: Vec<Elem>, index: usize) -> Vec<Elem> {
        assert!(!input.is_empty(), "Removing from empty vec leaves the input intact");
        input.remove(index);
        input
    }

    pub fn remove_random<Elem>(mut input: Vec<Elem>) -> Vec<Elem> {
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

    pub fn ensured_shuffle_keep_first<N: Clone + PartialEq>(input: &Vec<N>) -> Vec<N> {
        assert!(input.len() > 2); // no point shuffling a single element
        let mut new = input.clone();
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
        mut input: VariableList<Elem, Size>,
        index: usize,
    ) -> VariableList<Elem, Size> {
        let elem = input[index].clone();
        append(input, elem)
    }

    pub fn duplicate_random<Elem: Clone, Size: typenum::Unsigned>(
        mut input: VariableList<Elem, Size>,
    ) -> VariableList<Elem, Size> {
        let duplicate_idx = rand::thread_rng().gen_range(0..input.len());
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
        mut input: VariableList<Elem, Size>,
        modifier: fn(Elem) -> Elem,
    ) -> VariableList<Elem, Size> {
        let modify_idx = rand::thread_rng().gen_range(0..input.len());
        modify(input, modify_idx, modifier)
    }

    pub fn remove<Elem: Clone, Size: typenum::Unsigned>(
        mut input: VariableList<Elem, Size>,
        index: usize,
    ) -> VariableList<Elem, Size> {
        let as_vec = input.to_vec();
        vecs::remove(as_vec, index).into()
    }

    pub fn remove_random<Elem: Clone, Size: typenum::Unsigned>(
        mut input: VariableList<Elem, Size>,
    ) -> VariableList<Elem, Size> {
        let remove_idx = rand::thread_rng().gen_range(0..input.len());
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
        let mut as_vec = input.to_vec();
        vecs::ensured_shuffle_keep_first(&as_vec).into()
    }
}
