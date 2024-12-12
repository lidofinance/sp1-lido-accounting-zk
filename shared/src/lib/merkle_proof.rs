use hex;
use rs_merkle::{algorithms::Sha256, proof_serializers, MerkleProof, MerkleTree};
use std::any::type_name;

use ssz_types::VariableList;
use typenum::Unsigned;

use crate::hashing;
use ethereum_types::H256 as Hash256;

use itertools::Itertools;
use tree_hash::TreeHash;

type LeafIndex = usize;
pub type RsMerkleHash = <Sha256 as rs_merkle::Hasher>::Hash;

#[derive(Debug)]
pub enum Error {
    ProofError(rs_merkle::Error),
    DeserializationError(rs_merkle::Error),
    HashesMistmatch(String, Hash256, Hash256),
}

const ZEROHASH: [u8; 32] = [0u8; 32];
const ZEROHASH_H256: Hash256 = Hash256::zero();

pub trait MerkleTreeFieldLeaves {
    const FIELD_COUNT: usize;
    type TFields;

    fn get_tree_leaf_count() -> usize {
        Self::FIELD_COUNT.next_power_of_two()
    }

    fn get_leaf_index(field_name: &Self::TFields) -> LeafIndex;
    fn get_leafs_indices<const N: usize>(field_names: [Self::TFields; N]) -> [LeafIndex; N] {
        let mut result: [LeafIndex; N] = [0; N];
        for (idx, name) in field_names.iter().enumerate() {
            result[idx] = Self::get_leaf_index(name);
        }
        result
    }

    // This requires const generic that blocks/breaks FieldProof blanket implementation
    // fn get_fields(&self) -> [Hash256; FIELD_COUNT];
    // so we do this instead
    fn get_fields(&self) -> Vec<Hash256>;

    fn tree_field_leaves(&self) -> Vec<Hash256> {
        let field_leaves = self.get_fields();
        let tree_leaf_count = Self::get_tree_leaf_count();
        let padding = tree_leaf_count - Self::FIELD_COUNT;
        let mut result = field_leaves.to_vec();
        // Quirk: padding to the nearest power of 2 - rs_merkle doesn't seem to do it
        result.extend(std::iter::repeat(ZEROHASH_H256).take(padding));

        // Self-check
        assert!(result.len() == tree_leaf_count);
        result
    }
}

pub mod serde {
    use super::{proof_serializers, Error, MerkleProof, Sha256};

    pub fn deserialize_proof(proof_bytes: &[u8]) -> Result<MerkleProof<Sha256>, Error> {
        MerkleProof::deserialize::<proof_serializers::DirectHashesOrder>(proof_bytes)
            .map_err(Error::DeserializationError)
    }

    pub fn serialize_proof(proof: MerkleProof<Sha256>) -> Vec<u8> {
        proof.serialize::<proof_serializers::DirectHashesOrder>()
    }
}

pub fn build_root_from_proof(
    proof: &MerkleProof<Sha256>,
    total_leaves_count: usize,
    indices: &[LeafIndex],
    leaves_to_prove: &[RsMerkleHash],
    expand_to_depth: Option<usize>,
    mix_in_size: Option<usize>,
) -> Result<Hash256, Error> {
    assert!(
        total_leaves_count >= leaves_to_prove.len(),
        "Total number of elements {} must be >= the number of leafs to prove {}",
        total_leaves_count,
        leaves_to_prove.len()
    );
    assert!(
        indices.len() == leaves_to_prove.len(),
        "Number of leafs {} != number of indices {}",
        indices.len(),
        leaves_to_prove.len()
    );

    let mut root = proof
        .root(indices, leaves_to_prove, total_leaves_count)
        .map_err(Error::ProofError)?
        .into();

    log::debug!("Main data hash {}", hex::encode(root));
    if let Some(target_depth) = expand_to_depth {
        let main_data_depth: usize = total_leaves_count.trailing_zeros() as usize;
        root = hashing::pad_to_depth(&root, main_data_depth, target_depth);
    }
    if let Some(size) = mix_in_size {
        log::debug!("Mixing in size {} to {}", size, hex::encode(root));
        root = tree_hash::mix_in_length(&root, size);
    }

    Ok(root)
}

pub fn verify_hashes(expected: &Hash256, actual: &Hash256) -> Result<(), Error> {
    if actual == expected {
        return Ok(());
    }

    let err_msg = format!(
        "Root constructed from proof ({}) != actual ({})",
        hex::encode(expected),
        hex::encode(actual)
    );
    Err(Error::HashesMistmatch(err_msg, *actual, *expected))
}

pub trait StaticFieldProof {
    fn verify(
        proof: &MerkleProof<Sha256>,
        indices: &[LeafIndex],
        leaves: &[RsMerkleHash],
        expected_hash: &Hash256,
    ) -> Result<(), Error>;
}

impl<T> StaticFieldProof for T
where
    T: MerkleTreeFieldLeaves,
{
    fn verify(
        proof: &MerkleProof<Sha256>,
        indices: &[LeafIndex],
        leaves: &[RsMerkleHash],
        expected_hash: &Hash256,
    ) -> Result<(), Error> {
        // Quirk: rs_merkle does not seem pad trees to the next power of two, resulting in hashes that don't match
        // ones computed by ssz
        assert!(
            T::get_tree_leaf_count().is_power_of_two(),
            "{}::TREE_FIELDS_LENGTH should be a power of two, got {}",
            type_name::<T>(),
            T::get_tree_leaf_count()
        );
        let root_from_proof = build_root_from_proof(proof, T::get_tree_leaf_count(), indices, leaves, None, None)?;

        verify_hashes(expected_hash, &root_from_proof)
    }
}

pub trait FieldProof {
    fn get_members_multiproof(&self, indices: &[LeafIndex]) -> MerkleProof<Sha256>;
    fn verify_instance(
        &self,
        proof: &MerkleProof<Sha256>,
        indices: &[LeafIndex],
        leafs: &[RsMerkleHash],
    ) -> Result<(), Error>;

    fn get_serialized_multiproof(&self, indices: &[LeafIndex]) -> Vec<u8> {
        serde::serialize_proof(self.get_members_multiproof(indices))
    }

    fn verify_serialized(
        &self,
        proof_bytes: &Vec<u8>,
        indices: &[LeafIndex],
        leafs: &[RsMerkleHash],
    ) -> Result<(), Error> {
        let proof = serde::deserialize_proof(proof_bytes.as_slice())?;

        self.verify_instance(&proof, indices, leafs)
    }
}

impl<T> FieldProof for T
where
    T: MerkleTreeFieldLeaves + TreeHash + StaticFieldProof,
{
    fn get_members_multiproof(&self, indices: &[LeafIndex]) -> MerkleProof<Sha256> {
        let leaves_as_h256 = self.tree_field_leaves();
        let leaves_vec: Vec<RsMerkleHash> = leaves_as_h256
            .iter()
            .map(|val| val.as_fixed_bytes().to_owned())
            .collect();

        let merkle_tree = MerkleTree::<Sha256>::from_leaves(leaves_vec.as_slice());

        merkle_tree.proof(indices)
    }

    fn verify_instance(
        &self,
        proof: &MerkleProof<Sha256>,
        indices: &[LeafIndex],
        leaves: &[RsMerkleHash],
    ) -> Result<(), Error> {
        Self::verify(proof, indices, leaves, &self.tree_hash_root())
    }
}

impl<T, N> FieldProof for VariableList<T, N>
where
    T: TreeHash,
    N: Unsigned,
{
    fn get_members_multiproof(&self, indices: &[LeafIndex]) -> MerkleProof<Sha256> {
        assert!(
            hashing::packing_factor::<T>() == 1,
            "Multiproof is not yet supported for type {} that involve packing",
            type_name::<T>()
        );

        // Quirk: rs_merkle produces different values for different sequences of indices - the
        // "correct one" happens when indices are sorted
        let mut sorted = indices.to_vec();
        sorted.sort();

        // Quirk: rs_merkle does not pad to next power of two, ending up with a different merkle root
        let pad_to = self.len().next_power_of_two();
        assert!(pad_to >= self.len(), "Overflow finding the padding size");
        let leaves: Vec<RsMerkleHash> = self
            .iter()
            .map(|val| val.tree_hash_root().to_fixed_bytes())
            .pad_using(pad_to, |_i| ZEROHASH)
            .collect();
        assert!(leaves.len().is_power_of_two(), "Number of leaves must be a power of 2");

        let merkle_tree = MerkleTree::<Sha256>::from_leaves(leaves.as_slice());
        return merkle_tree.proof(sorted.as_slice());
    }

    fn verify_instance(
        &self,
        proof: &MerkleProof<Sha256>,
        indices: &[LeafIndex],
        leaves: &[RsMerkleHash],
    ) -> Result<(), Error> {
        assert!(
            hashing::packing_factor::<T>() == 1,
            "multiproof is not yet supported for types that involve packing",
        );

        // Quirk: rs_merkle does not pad to next power of two, ending up with a different merkle root
        let total_leaves_count = self.len().next_power_of_two();
        let target_depth = hashing::target_tree_depth::<T, N>();

        let with_height = build_root_from_proof(
            proof,
            total_leaves_count,
            indices,
            leaves,
            Some(target_depth),
            Some(self.len()),
        )?;

        verify_hashes(&self.tree_hash_root(), &with_height)
    }
}

#[cfg(test)]
mod test {
    use sp1_lido_accounting_zk_shared_merkle_tree_leaves_derive::MerkleTreeFieldLeaves;
    use ssz_types::VariableList;
    use tree_hash::TreeHash;
    use tree_hash_derive::TreeHash;
    use typenum::Unsigned;

    use crate::{eth_consensus_layer::Hash256, hashing};

    use super::{build_root_from_proof, verify_hashes, FieldProof, MerkleTreeFieldLeaves, RsMerkleHash};

    #[derive(Debug, Clone, PartialEq, TreeHash, MerkleTreeFieldLeaves)]
    pub struct GuineaPig {
        pub uint1: u64,
        pub uint2: u64,
        pub hash: Hash256,
    }

    impl GuineaPig {
        fn new(uint1: u64, uint2: u64, hash: Hash256) -> Self {
            GuineaPig { uint1, uint2, hash }
        }
    }

    #[test]
    fn struct_round_trip() {
        let guinea_pig = GuineaPig::new(1, 2, Hash256::zero());

        let indices = GuineaPig::get_leafs_indices([GuineaPigFields::uint1, GuineaPigFields::hash]);

        let proof = guinea_pig.get_members_multiproof(&indices);
        let leafs = [
            guinea_pig.uint1.tree_hash_root().to_fixed_bytes(),
            guinea_pig.hash.tree_hash_root().to_fixed_bytes(),
        ];
        guinea_pig
            .verify_instance(&proof, &indices, leafs.as_slice())
            .expect("Verification failed")
    }

    fn test_list<N: Unsigned>(input: &[GuineaPig], target_indices: &[usize]) {
        let list: VariableList<GuineaPig, N> = input.to_vec().into();
        let target_hashes: Vec<RsMerkleHash> = target_indices
            .iter()
            .map(|index| input[*index].tree_hash_root().to_fixed_bytes())
            .collect();

        let proof = list.get_members_multiproof(target_indices);
        list.verify_instance(&proof, target_indices, target_hashes.as_slice())
            .expect("Verification failed")
    }

    #[test]
    fn variable_list_round_trip() {
        let guinea_pigs = vec![
            GuineaPig::new(1, 10, Hash256::zero()),
            GuineaPig::new(2, 20, Hash256::random()),
            GuineaPig::new(3, 30, Hash256::random()),
            GuineaPig::new(4, 40, Hash256::random()),
            GuineaPig::new(5, 50, Hash256::random()),
        ];

        test_list::<typenum::U4>(&guinea_pigs, &[0, 2]);
        test_list::<typenum::U4>(&guinea_pigs, &[2, 0]);
        test_list::<typenum::U9>(&guinea_pigs, &[0, 1]);
        test_list::<typenum::U31>(&guinea_pigs, &[0, 1, 2]);
        test_list::<typenum::U31>(&guinea_pigs, &[0, 2, 1]);
        test_list::<typenum::U32>(&guinea_pigs, &[2]);
        test_list::<typenum::U255>(&guinea_pigs, &[1]);
        test_list::<typenum::U999>(&guinea_pigs, &[0, 1, 2, 3]);
        test_list::<typenum::U999>(&guinea_pigs, &[3, 2, 1, 0]);
        test_list::<typenum::U999>(&guinea_pigs, &[3, 1, 2, 0]);
    }

    fn test_list_against_hash<N: Unsigned>(input: &[GuineaPig], target_indices: &[usize]) {
        let list: VariableList<GuineaPig, N> = input.to_vec().into();

        let expected_root = list.tree_hash_root();
        let total_leaves_count = input.len().next_power_of_two();
        let target_depth = hashing::target_tree_depth::<GuineaPig, N>();

        let target_hashes: Vec<RsMerkleHash> = target_indices
            .iter()
            .map(|index| input[*index].tree_hash_root().to_fixed_bytes())
            .collect();

        let proof = list.get_members_multiproof(target_indices);
        let actiual_hash = build_root_from_proof(
            &proof,
            total_leaves_count,
            target_indices,
            target_hashes.as_slice(),
            Some(target_depth),
            Some(input.len()),
        )
        .expect("Failed to build hash");

        verify_hashes(&actiual_hash, &expected_root).expect("Verification failed");
    }

    #[test]
    fn variable_list_verify_against_hash() {
        let guinea_pigs = vec![
            GuineaPig::new(1, 10, Hash256::zero()),
            GuineaPig::new(2, 20, Hash256::random()),
            GuineaPig::new(3, 30, Hash256::random()),
            GuineaPig::new(4, 40, Hash256::random()),
            GuineaPig::new(5, 50, Hash256::random()),
            GuineaPig::new(6, 60, Hash256::random()),
        ];

        test_list_against_hash::<typenum::U8>(&guinea_pigs, &[0, 2]);
        test_list_against_hash::<typenum::U8>(&guinea_pigs, &[2, 0]);
        test_list_against_hash::<typenum::U9>(&guinea_pigs, &[0, 1]);
        test_list_against_hash::<typenum::U31>(&guinea_pigs, &[0, 1, 2]);
        test_list_against_hash::<typenum::U31>(&guinea_pigs, &[0, 2, 1]);
        test_list_against_hash::<typenum::U32>(&guinea_pigs, &[2]);
        test_list_against_hash::<typenum::U255>(&guinea_pigs, &[1]);
        test_list_against_hash::<typenum::U999>(&guinea_pigs, &[0, 1, 2, 3]);
        test_list_against_hash::<typenum::U999>(&guinea_pigs, &[3, 2, 1, 0]);
        test_list_against_hash::<typenum::U999>(&guinea_pigs, &[3, 1, 2, 0]);
    }
}
