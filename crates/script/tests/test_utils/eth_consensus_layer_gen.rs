use std::time::{SystemTime, UNIX_EPOCH};

use alloy_primitives::U256;
use sp1_lido_accounting_zk_shared::eth_consensus_layer::BlsPublicKey;
use sp1_lido_accounting_zk_shared::eth_consensus_layer::{
    BeaconBlockHeader, BeaconState, Checkpoint, Eth1Data, ExecutionPayloadHeader, Fork, Hash256, JustificationBits,
    SyncCommittee, Validator,
};
use sp1_lido_accounting_zk_shared::eth_spec;

use rand::{self, Rng};
use typenum::Unsigned;

mod constants {
    pub mod genesis {
        use lazy_static::lazy_static;
        pub const TIME: u64 = 123456;
        lazy_static! {
            pub static ref VALIDATORS_ROOT: [u8; 32] = 100200300400u128.to_le_bytes().repeat(2).try_into().unwrap();
        }
        lazy_static! {
            pub static ref BLOCK_ROOT: [u8; 32] = {
                let mut arr = [0u8; 32];
                let s = b"== Root Block Hash ==";
                arr[..s.len()].copy_from_slice(s);
                arr
            };
        }
    }

    pub mod fork {
        pub const PREVIOUS_VERSION: [u8; 4] = 1u32.to_le_bytes();
        pub const CURRENT_VERSION: [u8; 4] = 2u32.to_le_bytes();
    }
}

pub struct Generator<T: rand::Rng> {
    pub rng: T,
}

impl<T: rand::Rng> Generator<T> {
    pub fn new(rng: T) -> Self {
        Self { rng }
    }
    pub fn hash_root(&mut self) -> Hash256 {
        let val: [u8; 32] = self.rng.random();
        val.into()
    }
    pub fn bls_signature(&mut self) -> BlsPublicKey {
        let val: [u8; 48] = self.rng.random();
        val.to_vec().into()
    }
    pub fn zero_int_vector(&self, length: usize) -> Vec<u64> {
        vec![0; length]
    }
    pub fn empty_hash_vector(&self, length: usize) -> Vec<Hash256> {
        vec![[0u8; 32].into(); length]
    }
    pub fn empty_bls_sig_vector(&self, length: usize) -> Vec<BlsPublicKey> {
        vec![[0u8; 48].to_vec().into(); length]
    }
}

fn make_validator(
    current_epoch: u64,
    withdrawal_credentials: [u8; 32],
    deposited: bool,
    active: bool,
    exited: bool,
    pubkey: [u8; 48],
) -> Validator {
    let activation_eligibility_epoch = if deposited { current_epoch - 1 } else { u64::MAX };
    let activation_epoch = if active { current_epoch } else { u64::MAX };
    let exit_epoch = if exited { current_epoch } else { u64::MAX };

    Validator {
        pubkey: pubkey.to_vec().into(),
        withdrawal_credentials: withdrawal_credentials.into(),
        effective_balance: 32 * 1_000_000_000,
        slashed: false,
        activation_eligibility_epoch,
        activation_epoch,
        exit_epoch,
        withdrawable_epoch: activation_epoch,
    }
}

pub fn make_beacon_block_state(
    slot: u64,
    epoch: u64,
    parent_root: [u8; 32],
    validators: Vec<Validator>,
    balances: Vec<u64>,
    finalized_epoch: Option<u64>,
    previous_epoch: Option<u64>,
) -> BeaconState {
    let finalized_epoch = finalized_epoch.unwrap_or(epoch);
    let previous_epoch = previous_epoch.unwrap_or(epoch);
    let current_timestamp = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_secs();
    let validators_root: [u8; 32] = *constants::genesis::VALIDATORS_ROOT;

    let mut generator = Generator::new(rand::rng());
    let deposit_count: u64 = generator.rng.random_range(0..100_000);

    BeaconState {
        genesis_time: constants::genesis::TIME,
        genesis_validators_root: validators_root.into(),
        slot,
        fork: Fork {
            previous_version: constants::fork::PREVIOUS_VERSION.to_vec().into(),
            current_version: constants::fork::CURRENT_VERSION.to_vec().into(),
            epoch,
        },
        latest_block_header: BeaconBlockHeader {
            slot,
            proposer_index: generator.rng.random_range(0..100_000),
            parent_root: parent_root.into(),
            state_root: generator.hash_root(),
            body_root: generator.hash_root(),
        },
        block_roots: generator
            .empty_hash_vector(eth_spec::SlotsPerHistoricalRoot::to_usize())
            .into(),
        state_roots: generator
            .empty_hash_vector(eth_spec::SlotsPerHistoricalRoot::to_usize())
            .into(),
        historical_roots: vec![].into(),
        eth1_data: Eth1Data {
            deposit_root: generator.hash_root(),
            deposit_count,
            block_hash: generator.hash_root(),
        },
        eth1_data_votes: vec![].into(),
        eth1_deposit_index: deposit_count,
        validators: validators.into(),
        balances: balances.into(),
        randao_mixes: generator
            .empty_hash_vector(eth_spec::EpochsPerHistoricalVector::to_usize())
            .into(),
        slashings: generator
            .zero_int_vector(eth_spec::EpochsPerSlashingsVector::to_usize())
            .into(),
        previous_epoch_participation: vec![].into(),
        current_epoch_participation: vec![].into(),
        justification_bits: JustificationBits::from_bytes([0u8; 128].into()).unwrap(),
        previous_justified_checkpoint: Checkpoint {
            epoch: previous_epoch,
            root: generator.hash_root(),
        },
        current_justified_checkpoint: Checkpoint {
            epoch,
            root: generator.hash_root(),
        },
        finalized_checkpoint: Checkpoint {
            epoch: finalized_epoch,
            root: generator.hash_root(),
        },
        inactivity_scores: vec![].into(),
        current_sync_committee: SyncCommittee {
            pubkeys: generator
                .empty_bls_sig_vector(eth_spec::SyncCommitteeSize::to_usize())
                .into(),
            aggregate_pubkey: generator.bls_signature(),
        },
        next_sync_committee: SyncCommittee {
            pubkeys: generator
                .empty_bls_sig_vector(eth_spec::SyncCommitteeSize::to_usize())
                .into(),
            aggregate_pubkey: generator.bls_signature(),
        },
        latest_execution_payload_header: ExecutionPayloadHeader {
            parent_hash: parent_root.into(),
            fee_recipient: [0u8; 20].into(),
            state_root: generator.hash_root(),
            receipts_root: generator.hash_root(),
            logs_bloom: [0u8; 256].to_vec().into(),
            prev_randao: generator.hash_root(),
            block_number: slot + 10_000_000,
            gas_limit: 1_000_000_000_000,
            gas_used: 1_234_567_890,
            timestamp: current_timestamp - 3600,
            extra_data: vec![].into(),
            base_fee_per_gas: U256::from(1000),
            block_hash: generator.hash_root(),
            transactions_root: generator.hash_root(),
            withdrawals_root: generator.hash_root(),
            blob_gas_used: 1234,
            excess_blob_gas: 6789,
        },
        next_withdrawal_index: 0,
        next_withdrawal_validator_index: 1,
        historical_summaries: vec![].into(),

        deposit_requests_start_index: 0,
        deposit_balance_to_consume: 0,
        exit_balance_to_consume: 0,
        earliest_exit_epoch: epoch + 100,
        consolidation_balance_to_consume: 0,
        earliest_consolidation_epoch: epoch + 200,
        pending_deposits: vec![].into(),
        pending_partial_withdrawals: vec![].into(),
        pending_consolidations: vec![].into(),
    }
}

/// Helper for generating validators with a fixed withdrawal_credentials
pub fn generate_validators_with_credentials(
    count: usize,
    epoch: u64,
    withdrawal_credentials_gen: impl FnMut() -> [u8; 32],
    deposited: bool,
    active: bool,
    exited: bool,
    pubkey_prefix: Option<u8>,
) -> Vec<Validator> {
    let mut gen = withdrawal_credentials_gen;
    (0..count)
        .map(|i| {
            let mut pubkey = [0u8; 48];
            if let Some(prefix) = pubkey_prefix {
                pubkey[0] = prefix;
            }
            pubkey[1..9].copy_from_slice(&(i as u64).to_le_bytes());
            let validator_creds = gen();
            make_validator(epoch, validator_creds, deposited, active, exited, pubkey)
        })
        .collect()
}
struct ValidatorGenSpec {
    lido_widthrawal_credentials: [u8; 32],
    non_lido_validators: usize,
    pending_deposit: usize,
    deposited: usize,
    exited: usize,
}

/// Generate validators and balances, similar to the Python logic
pub fn generate_validators_and_balances(
    epoch: u64,
    spec: ValidatorGenSpec,
    shuffle: bool,
) -> (Vec<Validator>, Vec<u64>) {
    let mut validators = Vec::new();
    let mut rng = rand::rng();
    let gen_lido_creds = || spec.lido_widthrawal_credentials;
    let gen_random_creds = || rng.random();

    // Lido deposited
    validators.extend(generate_validators_with_credentials(
        spec.deposited,
        epoch,
        gen_lido_creds,
        true,
        true,
        false,
        Some(0xA1),
    ));
    // Lido exited
    validators.extend(generate_validators_with_credentials(
        spec.exited,
        epoch,
        gen_lido_creds,
        true,
        true,
        true,
        Some(0xA2),
    ));
    // Lido pending deposit
    validators.extend(generate_validators_with_credentials(
        spec.pending_deposit,
        epoch,
        gen_lido_creds,
        false,
        false,
        false,
        Some(0xA3),
    ));
    // Non-Lido
    validators.extend(generate_validators_with_credentials(
        spec.non_lido_validators,
        epoch,
        gen_random_creds,
        true,
        true,
        false,
        Some(0xB0),
    ));

    let mut balances: Vec<u64> = validators
        .iter()
        .map(|v| {
            if v.exit_epoch == u64::MAX && v.activation_epoch != u64::MAX {
                32 * 1_000_000_000
            } else {
                0
            }
        })
        .collect();

    if shuffle {
        use rand::seq::SliceRandom;
        let mut rng = rand::rng();
        let mut zipped: Vec<_> = validators.into_iter().zip(balances.into_iter()).collect();
        zipped.shuffle(&mut rng);
        let (v, b): (Vec<_>, Vec<_>) = zipped.into_iter().unzip();
        (v, b)
    } else {
        (validators, balances)
    }
}
