use rs_merkle::{algorithms::Sha256, proof_serializers, MerkleProof, MerkleTree};

use hex_literal::hex as h;
use ssz_types::VariableList;
use typenum::Unsigned;

use crate::{
    eth_consensus_layer::{BeaconBlockHeader, BeaconState, BeaconStatePrecomputedHashes, Hash256},
    hashing,
};

use itertools::Itertools;
use tree_hash::TreeHash;

type LeafIndex = usize;
type RsMerkleHash = <Sha256 as rs_merkle::Hasher>::Hash;

// TODO: better error
#[derive(Debug)]
pub enum Error {
    FieldDoesNotExist(String),
    VerificationError(String),
    DeserializationError(rs_merkle::Error),
}

const ZEROHASH: [u8; 32] = h!("0000000000000000000000000000000000000000000000000000000000000000");

pub trait MerkleTreeFieldLeaves {
    fn get_leaf_index(&self, field_name: &str) -> Result<LeafIndex, Error>;
    fn get_leafs_indices<const N: usize>(&self, field_names: [&str; N]) -> Result<[LeafIndex; N], Error> {
        let mut result: [LeafIndex; N] = [0; N];
        for (idx, name) in field_names.iter().enumerate() {
            result[idx] = self.get_leaf_index(name)?;
        }
        return Ok(result);
    }
    fn tree_field_leaves(&self) -> Vec<Hash256>;
}

fn is_power_of_two(n: usize) -> bool {
    n != 0 && (n & (n - 1)) == 0
}

pub trait FieldProof {
    fn get_field_multiproof(&self, indices: &[LeafIndex]) -> MerkleProof<Sha256>;
    fn verify(&self, proof: &MerkleProof<Sha256>, indices: &[LeafIndex]) -> Result<(), Error>;

    fn get_serialized_multiproof(&self, indices: &[LeafIndex]) -> Vec<u8> {
        let proof = self.get_field_multiproof(indices);
        proof.serialize::<proof_serializers::DirectHashesOrder>()
    }

    fn verify_serialized(&self, proof_bytes: &Vec<u8>, indices: &[LeafIndex]) -> Result<(), Error> {
        let maybe_proof = MerkleProof::deserialize::<proof_serializers::DirectHashesOrder>(proof_bytes.as_slice());
        match maybe_proof {
            Ok(proof) => self.verify(&proof, indices),
            Err(error) => Err(Error::DeserializationError(error)),
        }
    }
}

impl<T> FieldProof for T
where
    T: MerkleTreeFieldLeaves + TreeHash,
{
    fn get_field_multiproof(&self, indices: &[LeafIndex]) -> MerkleProof<Sha256> {
        let leaves_as_h256 = self.tree_field_leaves();
        let leaves_vec: Vec<RsMerkleHash> = leaves_as_h256
            .iter()
            .map(|val| val.as_fixed_bytes().to_owned())
            .collect();

        let merkle_tree = MerkleTree::<Sha256>::from_leaves(leaves_vec.as_slice());

        return merkle_tree.proof(indices);
    }

    fn verify(&self, proof: &MerkleProof<Sha256>, indices: &[LeafIndex]) -> Result<(), Error> {
        let leaves_as_h256 = self.tree_field_leaves();
        let total_leaves_count = leaves_as_h256.len();
        // Quirk: rs_merkle does not pad to next power of two, ending up with a different merkle root
        assert!(
            is_power_of_two(total_leaves_count),
            "Number of leaves must be a power of 2"
        );

        let leaves_vec: Vec<&RsMerkleHash> = leaves_as_h256.iter().map(|val| val.as_fixed_bytes()).collect();

        let leaves_to_prove: Vec<RsMerkleHash> = indices.iter().map(|idx| leaves_vec[*idx].to_owned()).collect();

        let verifies: bool = proof.verify(
            self.tree_hash_root().as_fixed_bytes().to_owned(),
            indices,
            leaves_to_prove.as_slice(),
            total_leaves_count,
        );
        if verifies {
            return Ok(());
        } else {
            return Err(Error::VerificationError("Verification failed".to_owned()));
        }
    }
}

// TODO: derive
impl MerkleTreeFieldLeaves for BeaconState {
    fn get_leaf_index(&self, field_name: &str) -> Result<LeafIndex, Error> {
        let precomp: BeaconStatePrecomputedHashes = self.into();
        precomp.get_leaf_index(field_name)
    }

    fn tree_field_leaves(&self) -> Vec<Hash256> {
        let precomp: BeaconStatePrecomputedHashes = self.into();
        precomp.tree_field_leaves()
    }
}

// TODO: derive
impl MerkleTreeFieldLeaves for BeaconStatePrecomputedHashes {
    fn get_leaf_index(&self, field_name: &str) -> Result<LeafIndex, Error> {
        let start_index = 0;
        match field_name {
            "genesis_time" => Ok(start_index + 0),
            "genesis_validators_root" => Ok(start_index + 1),
            "slot" => Ok(start_index + 2),
            "fork" => Ok(start_index + 3),
            "latest_block_header" => Ok(start_index + 4),
            "block_roots" => Ok(start_index + 5),
            "state_roots" => Ok(start_index + 6),
            "historical_roots" => Ok(start_index + 7),
            "eth1_data" => Ok(start_index + 8),
            "eth1_data_votes" => Ok(start_index + 9),
            "eth1_deposit_index" => Ok(start_index + 10),
            "validators" => Ok(start_index + 11),
            "balances" => Ok(start_index + 12),
            "randao_mixes" => Ok(start_index + 13),
            "slashings" => Ok(start_index + 14),
            "previous_epoch_participation" => Ok(start_index + 15),
            "current_epoch_participation" => Ok(start_index + 16),
            "justification_bits" => Ok(start_index + 17),
            "previous_justified_checkpoint" => Ok(start_index + 18),
            "current_justified_checkpoint" => Ok(start_index + 19),
            "finalized_checkpoint" => Ok(start_index + 20),
            "inactivity_scores" => Ok(start_index + 21),
            "current_sync_committee" => Ok(start_index + 22),
            "next_sync_committee" => Ok(start_index + 23),
            "latest_execution_payload_header" => Ok(start_index + 24),
            "next_withdrawal_index" => Ok(start_index + 25),
            "next_withdrawal_validator_index" => Ok(start_index + 26),
            "historical_summaries" => Ok(start_index + 27),
            _ => Err(Error::FieldDoesNotExist(format!("Field {} does not exist", field_name))),
        }
    }

    fn tree_field_leaves(&self) -> Vec<Hash256> {
        let result = vec![
            self.genesis_time,
            self.genesis_validators_root,
            self.slot,
            self.fork,
            self.latest_block_header,
            self.block_roots,
            self.state_roots,
            self.historical_roots,
            self.eth1_data,
            self.eth1_data_votes,
            self.eth1_deposit_index,
            self.validators,
            self.balances,
            self.randao_mixes,
            self.slashings,
            self.previous_epoch_participation,
            self.current_epoch_participation,
            self.justification_bits,
            self.previous_justified_checkpoint,
            self.current_justified_checkpoint,
            self.finalized_checkpoint,
            self.inactivity_scores,
            self.current_sync_committee,
            self.next_sync_committee,
            self.latest_execution_payload_header,
            self.next_withdrawal_index,
            self.next_withdrawal_validator_index,
            self.historical_summaries,
            // Quirk: padding to the nearest power of 2 - rs_merkle doesn't seem to do it
            ZEROHASH.into(),
            ZEROHASH.into(),
            ZEROHASH.into(),
            ZEROHASH.into(),
        ];
        // This is just a self-check - if BeaconState grows beyond 32 fields, it should become 64
        assert!(result.len() == 32);
        result
    }
}

// TODO: derive
impl MerkleTreeFieldLeaves for BeaconBlockHeader {
    fn get_leaf_index(&self, field_name: &str) -> Result<LeafIndex, Error> {
        let start_index = 0;
        match field_name {
            "slot" => Ok(start_index + 0),
            "proposer_index" => Ok(start_index + 1),
            "parent_root" => Ok(start_index + 2),
            "state_root" => Ok(start_index + 3),
            "body_root" => Ok(start_index + 4),
            _ => Err(Error::FieldDoesNotExist(format!("Field {} does not exist", field_name))),
        }
    }

    fn tree_field_leaves(&self) -> Vec<Hash256> {
        let result: Vec<Hash256> = vec![
            self.slot.tree_hash_root(),
            self.proposer_index.tree_hash_root(),
            self.parent_root,
            self.state_root,
            self.body_root,
            // Quirk: padding to the nearest power of 2 - rs_merkle doesn't seem to do it
            ZEROHASH.into(),
            ZEROHASH.into(),
            ZEROHASH.into(),
        ];
        // This is just a self-check - if BeaconState grows beyond 32 fields, it should become 64
        assert!(result.len() == 8);
        result
    }
}

fn pad_leaves_to_power_of_2<T: TreeHash, N: Unsigned>(value: &VariableList<T, N>) -> Vec<RsMerkleHash> {
    let pad_to = value.len().next_power_of_two();
    assert!(pad_to > 0, "Overflow finding the padding size");
    let leaves: Vec<RsMerkleHash> = value
        .iter()
        .map(|val| val.tree_hash_root().as_fixed_bytes().to_owned())
        .pad_using(pad_to, |_i| ZEROHASH)
        .collect();
    assert!(is_power_of_two(leaves.len()), "Number of leaves must be a power of 2");

    return leaves;
}

impl<T, N> FieldProof for VariableList<T, N>
where
    T: TreeHash,
    N: Unsigned,
{
    fn get_field_multiproof(&self, indices: &[LeafIndex]) -> MerkleProof<Sha256> {
        // Quirk: rs_merkle does not pad to next power of two, ending up with a different merkle root
        let leaves: Vec<RsMerkleHash> = pad_leaves_to_power_of_2(self);

        let merkle_tree = MerkleTree::<Sha256>::from_leaves(leaves.as_slice());
        return merkle_tree.proof(indices);
    }

    fn verify(&self, proof: &MerkleProof<Sha256>, indices: &[LeafIndex]) -> Result<(), Error> {
        // Quirk: rs_merkle does not pad to next power of two, ending up with a different merkle root
        let leaves: Vec<RsMerkleHash> = pad_leaves_to_power_of_2(self);
        let leaves_to_prove: Vec<RsMerkleHash> = indices.iter().map(|idx| leaves[*idx].to_owned()).collect();

        // for multiples of 2, trailing zeroes is essentially log2()
        let main_data_height: usize = leaves.len().trailing_zeros() as usize;
        let try_actual_hash = proof.root(indices, leaves_to_prove.as_slice(), leaves.len());

        if let Ok(data_root) = try_actual_hash {
            let target_depth = hashing::target_tree_depth::<T, N>();
            log::debug!(
                "Actual data {}, padded data {}, Main data height {}, tree target depth {}",
                self.len(),
                leaves.len(),
                main_data_height,
                target_depth
            );
            log::debug!("Main data hash {}", hex::encode(data_root));
            let expanded = hashing::pad_to_depth(&data_root.into(), main_data_height, target_depth);
            let with_height = tree_hash::mix_in_length(&expanded, self.len());
            let expected = self.tree_hash_root();
            if with_height == expected {
                return Ok(());
            } else {
                return Err(Error::VerificationError(format!(
                    "Root constructed from proof ({}) != actual ({})",
                    hex::encode(with_height),
                    hex::encode(expected)
                )));
            }
        } else {
            return Err(Error::VerificationError(
                "Failed to construct root from proof".to_owned(),
            ));
        }
    }
}
