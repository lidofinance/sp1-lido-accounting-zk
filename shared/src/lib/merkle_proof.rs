use hex;
use itertools::Itertools;
use rs_merkle;
use std::any::type_name;

use ssz_types::VariableList;
use typenum::Unsigned;

use crate::eth_consensus_layer::Hash256;
use crate::hashing;

use tree_hash::TreeHash;

type MerkleHash = [u8; 32];
type LeafIndex = usize;

#[derive(Debug)]
pub enum Error {
    PreconditionError(String),
    ProofError(rs_merkle::Error),
    DeserializationError(rs_merkle::Error),
    HashesMistmatch(String, Hash256, Hash256),
}

const ZEROHASH: [u8; 32] = [0u8; 32];

pub struct MerkleProofWrapper {
    proof: rs_merkle::MerkleProof<rs_merkle::algorithms::Sha256>,
}

impl MerkleProofWrapper {
    pub fn from_hashes(hashes: Vec<Hash256>, indices: &[LeafIndex]) -> Self {
        // Quirk: rs_merkle does not pad to next power of two, ending up with a different merkle root
        let pad_to = hashes.len().next_power_of_two();
        assert!(
            pad_to >= hashes.len(),
            "Overflow happened finding the padding size: list len: {}, pad_to: {}",
            hashes.len(),
            pad_to
        );
        // Quirks:
        // * rs_merkle produces different values for different sequences of indices - the "correct one" happens when indices are sorted
        // * rs_merkle produces different values if indices are duplicated
        // Solution: make them unique and sorted
        let unique_sorted: Vec<LeafIndex> = indices.iter().unique().sorted().cloned().collect();

        let leaves_vec: Vec<MerkleHash> = hashes
            .iter()
            .map(|val| val.0)
            .pad_using(pad_to, |_i| ZEROHASH)
            .collect();

        let merkle_tree = rs_merkle::MerkleTree::<rs_merkle::algorithms::Sha256>::from_leaves(leaves_vec.as_slice());
        Self {
            proof: merkle_tree.proof(&unique_sorted),
        }
    }

    pub fn from_instance<T>(instance: &T, field_names: &[T::TFields]) -> Self
    where
        T: MerkleTreeFieldLeaves + TreeHash + StaticFieldProof<T>,
    {
        Self::from_hashes(instance.tree_field_leaves(), &T::get_leafs_indices(field_names))
    }

    fn from_variable_list<T, N>(list: &VariableList<T, N>, indices: &[usize]) -> Self
    where
        T: TreeHash,
        N: Unsigned,
    {
        let hashes = list.iter().map(|val| val.tree_hash_root()).collect();
        Self::from_hashes(hashes, indices)
    }

    pub fn proof_hashes_hex(&self) -> Vec<String> {
        self.proof.proof_hashes_hex()
    }

    pub fn build_root_from_proof(
        &self,
        total_leaves_count: usize,
        indices: &[LeafIndex],
        element_hashes: &[Hash256],
        expand_to_depth: Option<usize>,
        mix_in_size: Option<usize>,
    ) -> Result<Hash256, Error> {
        // Quirk: rs_merkle does not seem pad trees to the next power of two, resulting in hashes that don't match
        // ones computed by ssz
        let leaves_count = total_leaves_count.next_power_of_two();

        self._verify_build_root_from_proof(indices, element_hashes, leaves_count)?;

        self._build_root(leaves_count, indices, element_hashes, expand_to_depth, mix_in_size)
    }

    #[cfg(test)]
    pub fn build_root_from_proof_bypass_verify(
        &self,
        total_leaves_count: usize,
        indices: &[LeafIndex],
        element_hashes: &[Hash256],
        expand_to_depth: Option<usize>,
        mix_in_size: Option<usize>,
    ) -> Result<Hash256, Error> {
        // Quirk: rs_merkle does not seem pad trees to the next power of two, resulting in hashes that don't match
        // ones computed by ssz
        let leaves_count = total_leaves_count.next_power_of_two();
        self._build_root(leaves_count, indices, element_hashes, expand_to_depth, mix_in_size)
    }

    fn _verify_build_root_from_proof(
        &self,
        indices: &[LeafIndex],
        element_hashes: &[Hash256],
        leaves_count: usize,
    ) -> Result<(), Error> {
        if leaves_count < element_hashes.len() {
            return Err(Error::PreconditionError(format!(
                "Total number of elements {} must be >= the number of leafs to prove {}",
                leaves_count,
                element_hashes.len()
            )));
        }
        if indices.len() != element_hashes.len() {
            return Err(Error::PreconditionError(format!(
                "Number of leafs {} != number of indices {}",
                indices.len(),
                element_hashes.len()
            )));
        }
        if !indices.iter().all_unique() {
            return Err(Error::PreconditionError("Indices must be unique".to_owned()));
        }
        Ok(())
    }

    pub fn _build_root(
        &self,
        leaves_count: usize,
        indices: &[LeafIndex],
        element_hashes: &[Hash256],
        expand_to_depth: Option<usize>,
        mix_in_size: Option<usize>,
    ) -> Result<Hash256, Error> {
        let mut leaf_hashes: Vec<MerkleHash> = Vec::with_capacity(element_hashes.len());
        for element_hash in element_hashes {
            leaf_hashes.push(element_hash.0);
        }

        let mut root = self
            .proof
            .root(indices, &leaf_hashes, leaves_count)
            .map_err(Error::ProofError)?
            .into();

        tracing::debug!("Main data hash {}", hex::encode(root));
        if let Some(target_depth) = expand_to_depth {
            let main_data_depth: usize = leaves_count.trailing_zeros() as usize;
            root = hashing::pad_to_depth(&root, main_data_depth, target_depth);
        }
        if let Some(size) = mix_in_size {
            tracing::debug!("Mixing in size {} to {}", size, hex::encode(root));
            root = tree_hash::mix_in_length(&root, size);
        }

        Ok(root)
    }
}

pub trait MerkleTreeFieldLeaves {
    const FIELD_COUNT: usize;
    type TFields;

    fn get_tree_leaf_count() -> usize {
        Self::FIELD_COUNT.next_power_of_two()
    }

    fn get_leaf_index(field_name: &Self::TFields) -> LeafIndex;

    fn get_leafs_indices(field_names: &[Self::TFields]) -> Vec<LeafIndex> {
        field_names.iter().map(|v| Self::get_leaf_index(v)).collect()
    }

    fn get_leafs_indices_const<const N: usize>(field_names: &[Self::TFields; N]) -> [LeafIndex; N] {
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
        self.get_fields()
    }
}

pub mod serde {
    use super::{Error, MerkleProofWrapper};
    use rs_merkle::{proof_serializers, MerkleProof};

    pub fn deserialize_proof(proof_bytes: &[u8]) -> Result<MerkleProofWrapper, Error> {
        MerkleProof::deserialize::<proof_serializers::DirectHashesOrder>(proof_bytes)
            .map_err(Error::DeserializationError)
            .map(|proof| MerkleProofWrapper { proof })
    }

    pub fn serialize_proof(proof: MerkleProofWrapper) -> Vec<u8> {
        proof.proof.serialize::<proof_serializers::DirectHashesOrder>()
    }
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

pub trait StaticFieldProof<T: MerkleTreeFieldLeaves> {
    fn verify(
        proof: &MerkleProofWrapper,
        indices: &[T::TFields],
        leaves: &[Hash256],
        expected_hash: &Hash256,
    ) -> Result<(), Error>;
}

impl<T> StaticFieldProof<T> for T
where
    T: MerkleTreeFieldLeaves,
{
    fn verify(
        proof: &MerkleProofWrapper,
        indices: &[T::TFields],
        leaves: &[Hash256],
        expected_hash: &Hash256,
    ) -> Result<(), Error> {
        let field_indices: Vec<usize> = indices.iter().map(|v| T::get_leaf_index(v)).collect();
        let root_from_proof =
            proof.build_root_from_proof(T::get_tree_leaf_count(), &field_indices, leaves, None, None)?;

        verify_hashes(expected_hash, &root_from_proof)
    }
}

pub trait FieldProof {
    type LeafIndex;
    fn get_members_multiproof(&self, indices: &[Self::LeafIndex]) -> MerkleProofWrapper;

    fn get_serialized_multiproof(&self, indices: &[Self::LeafIndex]) -> Vec<u8> {
        serde::serialize_proof(self.get_members_multiproof(indices))
    }

    fn verify_instance(
        &self,
        proof: &MerkleProofWrapper,
        indices: &[Self::LeafIndex],
        element_hashes: &[Hash256],
    ) -> Result<(), Error>;

    fn verify_serialized(
        &self,
        proof_bytes: &Vec<u8>,
        indices: &[Self::LeafIndex],
        element_hashes: &[Hash256],
    ) -> Result<(), Error> {
        let proof = serde::deserialize_proof(proof_bytes.as_slice())?;

        self.verify_instance(&proof, indices, element_hashes)
    }
}

impl<T> FieldProof for T
where
    T: MerkleTreeFieldLeaves + TreeHash + StaticFieldProof<T>,
{
    type LeafIndex = T::TFields;
    fn get_members_multiproof(&self, indices: &[Self::LeafIndex]) -> MerkleProofWrapper {
        MerkleProofWrapper::from_instance(self, indices)
    }

    fn verify_instance(
        &self,
        proof: &MerkleProofWrapper,
        indices: &[Self::LeafIndex],
        element_hashes: &[Hash256],
    ) -> Result<(), Error> {
        Self::verify(proof, indices, element_hashes, &self.tree_hash_root())
    }
}

impl<T, N> FieldProof for VariableList<T, N>
where
    T: TreeHash,
    N: Unsigned,
{
    type LeafIndex = usize;

    fn get_members_multiproof(&self, indices: &[LeafIndex]) -> MerkleProofWrapper {
        assert!(
            hashing::packing_factor::<T>() == 1,
            "Multiproof is not yet supported for type {} that involve packing",
            type_name::<T>()
        );

        MerkleProofWrapper::from_variable_list(self, indices)
    }

    fn verify_instance(
        &self,
        proof: &MerkleProofWrapper,
        indices: &[Self::LeafIndex],
        element_hashes: &[Hash256],
    ) -> Result<(), Error> {
        assert!(
            hashing::packing_factor::<T>() == 1,
            "multiproof is not yet supported for types that involve packing",
        );

        let with_height = proof.build_root_from_proof(
            self.len(),
            indices,
            element_hashes,
            Some(hashing::target_tree_depth::<T, N>()),
            Some(self.len()),
        )?;

        verify_hashes(&self.tree_hash_root(), &with_height)
    }
}

#[cfg(test)]
mod test {
    use alloy_primitives::U256;
    use sp1_lido_accounting_zk_shared_merkle_tree_leaves_derive::MerkleTreeFieldLeaves;
    use ssz_types::VariableList;
    use tree_hash::TreeHash;
    use tree_hash_derive::TreeHash;
    use typenum::Unsigned;

    use crate::{eth_consensus_layer::Hash256, hashing};

    use super::{verify_hashes, FieldProof, LeafIndex, MerkleTreeFieldLeaves};

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

    fn struct_round_trip(guinea_pig: GuineaPig, fields: &[<GuineaPig as MerkleTreeFieldLeaves>::TFields]) {
        let proof = guinea_pig.get_members_multiproof(fields);
        let all_leaves = guinea_pig.tree_field_leaves();
        let target_indices = GuineaPig::get_leafs_indices(fields);
        let target_leaves: Vec<Hash256> = target_indices.iter().map(|idx| all_leaves[*idx]).collect();
        guinea_pig
            .verify_instance(&proof, fields, &target_leaves)
            .expect("Verification failed")
    }

    #[test]
    fn test_struct_round_trip() {
        struct_round_trip(GuineaPig::new(1, 2, Hash256::ZERO), &GuineaPigFields::all());
        struct_round_trip(
            GuineaPig::new(1, 2, Hash256::ZERO),
            &[GuineaPigFields::hash, GuineaPigFields::uint1],
        );
        struct_round_trip(
            GuineaPig::new(10, 20, Hash256::random()),
            &[GuineaPigFields::uint2, GuineaPigFields::uint1],
        );
    }

    #[test]
    fn test_struct_duplicate_indices_fails() {
        // Handling duplicates
        let guinea_pig = GuineaPig::new(1, 2, Hash256::random());
        let fields = [GuineaPigFields::hash, GuineaPigFields::hash];
        let proof = guinea_pig.get_members_multiproof(&fields);
        let all_leaves = guinea_pig.tree_field_leaves();
        let target_indices = GuineaPig::get_leafs_indices(&fields);
        let target_leaves: Vec<Hash256> = target_indices.iter().map(|idx| all_leaves[*idx]).collect();
        let verification = guinea_pig.verify_instance(&proof, &fields, &target_leaves);
        assert!(verification.is_err());
    }

    fn test_list<N: Unsigned>(input: &[GuineaPig], target_indices: &[usize]) {
        let list: VariableList<GuineaPig, N> = input.to_vec().into();
        let target_hashes: Vec<Hash256> = target_indices
            .iter()
            .map(|index| input[*index].tree_hash_root())
            .collect();

        let proof = list.get_members_multiproof(target_indices);
        list.verify_instance(&proof, target_indices, target_hashes.as_slice())
            .expect("Verification failed")
    }

    #[test]
    fn variable_list_round_trip() {
        let guinea_pigs = vec![
            GuineaPig::new(1, 10, Hash256::ZERO),
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

        let target_hashes: Vec<Hash256> = target_indices
            .iter()
            .map(|index| input[*index].tree_hash_root())
            .collect();

        let proof = list.get_members_multiproof(target_indices);
        let actiual_hash = proof
            .build_root_from_proof(
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
            GuineaPig::new(1, 10, Hash256::ZERO),
            GuineaPig::new(2, 20, Hash256::random()),
            GuineaPig::new(3, 30, Hash256::random()),
            GuineaPig::new(4, 40, Hash256::random()),
            GuineaPig::new(5, 50, Hash256::random()),
            GuineaPig::new(6, 60, Hash256::random()),
        ];

        test_list_against_hash::<typenum::U8>(&guinea_pigs, &[0, 2]);
        // test_list_against_hash::<typenum::U8>(&guinea_pigs, &[0, 0, 2]);
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

    #[test]
    fn variable_list_duplicate_indices_fails() {
        let guinea_pigs = vec![
            GuineaPig::new(1, 10, Hash256::ZERO),
            GuineaPig::new(2, 20, Hash256::random()),
            GuineaPig::new(3, 30, Hash256::random()),
            GuineaPig::new(4, 40, Hash256::random()),
            GuineaPig::new(5, 50, Hash256::random()),
        ];
        let target_indices = [0, 0, 2];
        let list: VariableList<GuineaPig, typenum::U32> = guinea_pigs.to_vec().into();
        let target_hashes: Vec<Hash256> = target_indices
            .iter()
            .map(|index| guinea_pigs[*index].tree_hash_root())
            .collect();

        let proof = list.get_members_multiproof(&target_indices);
        let verification = list.verify_instance(&proof, &target_indices, target_hashes.as_slice());
        assert!(verification.is_err())
    }

    fn wrapped_proof_compute_ssz_list_hash<Item: TreeHash, N: typenum::Unsigned>(
        list: &VariableList<Item, N>,
        verify_indices: &[usize],
        element_hashes: &[Hash256],
        proof: super::MerkleProofWrapper,
    ) -> Result<Hash256, super::Error> {
        let list_len = list.len();
        let target_depth = hashing::target_tree_depth::<Item, N>();
        proof.build_root_from_proof(
            list_len,
            verify_indices,
            element_hashes,
            Some(target_depth),
            Some(list_len),
        )
    }

    #[test]
    fn test_duplicate_handling() {
        let raw_vec: Vec<U256> = [1u64, 2, 3, 4, 5, 6, 7, 8].iter().map(|v| U256::from(*v)).collect();
        let list: VariableList<U256, typenum::U8> = raw_vec.into();
        let expected_root = list.tree_hash_root();

        let verify_indices: Vec<LeafIndex> = vec![2, 2, 3];
        let proof_indices: Vec<LeafIndex> = vec![2, 3];
        let hashes: Vec<Hash256> = vec![
            U256::from(3).tree_hash_root(),
            U256::from(4).tree_hash_root(),
            U256::from(12).tree_hash_root(), // non-existent leaf
        ];

        let proof = list.get_members_multiproof(&proof_indices);

        let list_len = list.len();
        let target_depth = hashing::target_tree_depth::<U256, typenum::U8>();

        let raw_proof_root = proof
            .build_root_from_proof_bypass_verify(list_len, &verify_indices, &hashes, Some(target_depth), Some(list_len))
            .expect("Should not fail");
        // Exposes rs_merkle allowing duplicates with
        // See https://github.com/color-typea/sp1-lido-accounting-zk/issues/5
        let raw_proof_result = verify_hashes(&raw_proof_root, &expected_root);
        assert!(raw_proof_result.is_ok());

        let wrapped_proof_root = wrapped_proof_compute_ssz_list_hash(&list, &verify_indices, &hashes, proof);

        // Demonstrates wrapper fixes the problem
        assert!(wrapped_proof_root.is_err())
    }

    mod proptests {
        use std::collections::HashSet;

        use alloy_primitives::U256;
        use itertools::Itertools;
        use proptest as prop;
        use proptest::prelude::*;
        use proptest_arbitrary_interop::arb;
        use ssz_types::VariableList;
        use tree_hash::TreeHash;

        use crate::{eth_consensus_layer::Hash256, merkle_proof::FieldProof};

        const MAX_INDICES: usize = 32;
        const MAX_LIST_SIZE: usize = 32;
        const MAX_MANIPULATIONS: usize = 32;

        #[derive(Debug, Clone)]
        struct TestData {
            list: Vec<U256>,
            prove_indices: Vec<usize>,
        }

        prop_compose! {
            fn test_data_strategy(unique: bool)(
                list in prop::collection::vec(arb::<U256>(), 1..=MAX_LIST_SIZE),
                prove_indices_index in prop::collection::vec(any::<prop::sample::Index>(), 1..=MAX_INDICES),
            ) -> TestData {
                let prove_index_builder = prove_indices_index.iter().map(|idx| idx.index(list.len()));
                let prove_indices: Vec<usize> = if unique {
                    prove_index_builder.unique().collect()
                } else {
                    prove_index_builder.collect()
                };
                TestData {
                    list,
                    prove_indices
                }
            }
        }

        #[derive(Debug, Clone)]
        struct Append {
            item: U256,
            index_in_list: usize,
        }
        #[derive(Debug, Clone)]
        struct EditHash {
            item: U256,
            index_in_hashes: usize,
        }
        #[derive(Debug, Clone)]
        struct Insert {
            item: U256,
            index_in_list: usize,
            index_in_hashes: usize,
        }

        #[derive(Debug, Clone)]
        enum Manipulation {
            Append(Append),
            EditHash(EditHash),
            Insert(Insert),
        }

        #[derive(Debug, Clone)]
        enum MultiManipulation {
            Append { items: Vec<Append> },
            EditHash { items: Vec<EditHash> },
            Insert { items: Vec<Insert> },
        }

        prop_compose! {
            fn append_strategy(test_data: TestData)(
                item in arb::<U256>(),
                append_index in any::<prop::sample::Index>()
            ) -> Append {
                Append{
                    item,
                    index_in_list: append_index.index(test_data.list.len()),
                }
            }
        }

        prop_compose! {
            fn edit_hash_strategy(test_data: TestData)(
                item in arb::<U256>(),
                edit_index in any::<prop::sample::Index>()
            ) -> EditHash {
                EditHash{
                    item,
                    index_in_hashes: edit_index.index(test_data.prove_indices.len()),
                }
            }
        }

        prop_compose! {
            fn insert_strategy(test_data: TestData)(
                item in arb::<U256>(),
                list_index in any::<prop::sample::Index>(),
                insert_index in any::<prop::sample::Index>()
            ) -> Insert {
                Insert{
                    item,
                    index_in_list: list_index.index(test_data.list.len()),
                    index_in_hashes: insert_index.index(test_data.prove_indices.len()),
                }
            }
        }

        fn single_manipulations_strategy(test_data: TestData) -> impl Strategy<Value = Manipulation> {
            prop_oneof![
                append_strategy(test_data.clone()).prop_map(Manipulation::Append),
                edit_hash_strategy(test_data.clone()).prop_map(Manipulation::EditHash),
                insert_strategy(test_data.clone()).prop_map(Manipulation::Insert)
            ]
        }

        fn multi_manipulations_strategy(test_data: TestData) -> impl Strategy<Value = MultiManipulation> {
            prop_oneof![
                prop::collection::vec(append_strategy(test_data.clone()), 1..=MAX_MANIPULATIONS)
                    .prop_map(|edits| MultiManipulation::Append { items: edits }),
                prop::collection::vec(edit_hash_strategy(test_data.clone()), 1..=MAX_MANIPULATIONS)
                    .prop_map(|edits| MultiManipulation::EditHash { items: edits }),
                prop::collection::vec(insert_strategy(test_data.clone()), 1..=MAX_MANIPULATIONS)
                    .prop_map(|edits| MultiManipulation::Insert { items: edits })
            ]
        }

        fn data_and_single_manipulation_strategy(
            unique_prove_indices: bool,
        ) -> impl Strategy<Value = (TestData, Manipulation)> {
            test_data_strategy(unique_prove_indices).prop_flat_map(|test_data| {
                let test_data_strategy = Just(test_data.clone());
                let manipulation_strategy = single_manipulations_strategy(test_data);
                (test_data_strategy, manipulation_strategy)
            })
        }

        fn data_and_multi_manipulation_strategy(
            unique_prove_indices: bool,
        ) -> impl Strategy<Value = (TestData, MultiManipulation)> {
            test_data_strategy(unique_prove_indices).prop_flat_map(|test_data| {
                let test_data_strategy = Just(test_data.clone());
                let manipulation_strategy = multi_manipulations_strategy(test_data);
                (test_data_strategy, manipulation_strategy)
            })
        }

        proptest! {
            #![proptest_config(ProptestConfig {
                cases: 10000,
                .. ProptestConfig::default()
            })]
            #[test]
            fn test_wrapper_list_proof_manipulations(
                (test_data, manipulation) in data_and_single_manipulation_strategy(false)
            ) {
                let list: VariableList<U256, typenum::U32> = test_data.list.into();
                let proof = list.get_members_multiproof(&test_data.prove_indices);

                let mut hashes: Vec<Hash256> = test_data.prove_indices.iter().map(|idx| list[*idx].tree_hash_root()).collect();
                let mut verify_indices = test_data.prove_indices.clone();

                match manipulation {
                    Manipulation::Append(item) => {
                        prop_assume!(list[item.index_in_list] != item.item);
                        hashes.push(item.item.tree_hash_root());
                        verify_indices.push(item.index_in_list);
                    },
                    Manipulation::EditHash(item) => {
                        prop_assume!(list[test_data.prove_indices[item.index_in_hashes]] != item.item);
                        hashes[item.index_in_hashes] = item.item.tree_hash_root();
                    },
                    Manipulation::Insert(item) => {
                        prop_assume!(list[item.index_in_list] != item.item);
                        hashes.insert(item.index_in_hashes, item.item.tree_hash_root());
                        verify_indices.push(item.index_in_list);
                    },
                }
                assert_eq!(hashes.len(), verify_indices.len());

                let wrapped_proof_root = super::wrapped_proof_compute_ssz_list_hash(&list, &verify_indices, &hashes, proof);

                if let Ok(hash) = wrapped_proof_root {
                    assert!(hash != list.tree_hash_root());
                } else {
                    // Failing to produce hash is also fine
                }
            }
        }

        proptest! {
            #![proptest_config(ProptestConfig {
                // Multiple manipulations have higher chance of producing invalid proof, so need more iterations to run
                cases: 10000,
                .. ProptestConfig::default()
            })]
            #[test]
            fn test_wrapper_list_proof_multi_manipulations(
                (test_data, manipulation) in data_and_multi_manipulation_strategy(false)
            ) {
                let list: VariableList<U256, typenum::U32> = test_data.list.into();
                let proof = list.get_members_multiproof(&test_data.prove_indices);

                let mut hashes: Vec<Hash256> = test_data.prove_indices.iter().map(|idx| list[*idx].tree_hash_root()).collect();
                let mut verify_indices = test_data.prove_indices.clone();

                match manipulation {
                    MultiManipulation::Append { items } => {
                        for item in items {
                            prop_assume!(list[item.index_in_list] != item.item);
                            hashes.push(item.item.tree_hash_root());
                            verify_indices.push(item.index_in_list);
                        }
                    },
                    MultiManipulation::EditHash { items } => {
                        for item in items {
                            prop_assume!(list[test_data.prove_indices[item.index_in_hashes]] != item.item);
                            hashes[item.index_in_hashes] = item.item.tree_hash_root();
                        }
                    },
                    MultiManipulation::Insert { items } => {
                        for item in items {
                            prop_assume!(list[item.index_in_list] != item.item);
                            hashes.insert(item.index_in_hashes, item.item.tree_hash_root());
                            verify_indices.push(item.index_in_list);
                        }
                    }
                }
                assert_eq!(hashes.len(), verify_indices.len());

                let wrapped_proof_root = super::wrapped_proof_compute_ssz_list_hash(&list, &verify_indices, &hashes, proof);

                if let Ok(hash) = wrapped_proof_root {
                    assert!(hash != list.tree_hash_root());
                } else {
                    // Failing to produce hash is also fine
                }
            }
        }

        proptest! {
            #[test]
            fn test_wrapper_list_proof_unrelated_verification_indices(
                test_data in test_data_strategy(true),
                verify_indices_index in prop::collection::vec(any::<prop::sample::Index>(), 1..=MAX_INDICES),
            ) {
                let list: VariableList<U256, typenum::U32> = test_data.list.into();
                let proof = list.get_members_multiproof(&test_data.prove_indices);

                let verify_indices: Vec<usize> = verify_indices_index.iter().map(|idx| idx.index(list.len())).collect();

                let special_case = list.len().is_power_of_two() && verify_indices.iter().cloned().collect::<HashSet<_>>() == (0..list.len()).collect();
                prop_assume!(!special_case); // special case - see test_wrapper_list_proof_full_list_verification_power_of_2
                let hashes: Vec<Hash256> = verify_indices.iter().map(|idx| list[*idx].tree_hash_root()).collect();

                let verify_elems = verify_indices.iter().map(|idx| list[*idx]).collect::<HashSet<_>>();
                let prove_elems = test_data.prove_indices.iter().map(|idx| list[*idx]).collect::<HashSet<_>>();

                prop_assume!(verify_elems != prove_elems);

                assert_eq!(hashes.len(), verify_indices.len());

                let wrapped_proof_root = super::wrapped_proof_compute_ssz_list_hash(&list, &verify_indices, &hashes, proof);

                if let Ok(hash) = wrapped_proof_root {
                    assert!(hash != list.tree_hash_root());
                } else {
                    // Failing to produce hash is also fine
                }
            }
        }

        proptest! {
            #[test]
            fn test_wrapper_list_proof_full_list_verification_power_of_2(
                raw_values in (0u32..=10).prop_flat_map(|num| prop::collection::vec(arb::<U256>(), 2usize.pow(num))),
                prove_indices_index in prop::collection::vec(any::<prop::sample::Index>(), 1..=MAX_INDICES)
            ) {
                // QUIRK (?): potentially a rs-merkle quick, but verifying elements not included into the original proof
                // still works, if _all_ actual elements are passed and list length is a power of two
                // This is a bit unexpected, but does not produce a security/correctness risk - only possible to prove
                // actually existing leafs. However, this is a bit unexpected, so codifying this in the
                // test; if the underlying behavior changes to fail producing the output, this test can be safely removed
                let list: VariableList<U256, typenum::U32> = raw_values.into();
                let prove_indices: Vec<usize> = prove_indices_index.iter().map(|idx| idx.index(list.len())).collect();
                let proof = list.get_members_multiproof(&prove_indices);

                prop_assume!(list.len().is_power_of_two());

                let verify_indices: Vec<usize> = (0..list.len()).collect();
                let hashes: Vec<Hash256> = verify_indices.iter().map(|idx| list[*idx].tree_hash_root()).collect();

                let verify_elems = verify_indices.iter().map(|idx| list[*idx]).collect::<HashSet<_>>();
                let prove_elems = prove_indices.iter().map(|idx| list[*idx]).collect::<HashSet<_>>();

                prop_assume!(verify_elems != prove_elems);
                assert_eq!(hashes.len(), verify_indices.len());

                let wrapped_proof_root = super::wrapped_proof_compute_ssz_list_hash(&list, &verify_indices, &hashes, proof);
                let hash = wrapped_proof_root.expect("This should not fail");
                assert!(hash == list.tree_hash_root());
            }
        }
    }
}
