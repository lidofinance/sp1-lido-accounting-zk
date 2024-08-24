use ethereum_hashing::{hash32_concat, ZERO_HASHES};
use ssz_types::VariableList;
use tree_hash::{Hash256, PackedEncoding, TreeHash, TreeHashType};
use typenum::Unsigned;

pub trait HashHelper<T: TreeHash> {
    fn hash_list<N: Unsigned>(list: &VariableList<T, N>) -> Hash256;
}

pub fn pad_to_depth(hash: &Hash256, current_depth: usize, target_depth: usize) -> Hash256 {
    let mut curhash: [u8; 32] = hash.to_fixed_bytes();
    for depth in current_depth..target_depth {
        curhash = hash32_concat(&curhash, ZERO_HASHES[depth].as_slice());
    }
    return curhash.into();
}

pub fn packing_factor<T: TreeHash>() -> usize {
    match T::tree_hash_type() {
        TreeHashType::Basic => T::tree_hash_packing_factor(),
        TreeHashType::Container | TreeHashType::List | TreeHashType::Vector => 1,
    }
}

pub fn item_encoding<T: TreeHash>(item: &T) -> PackedEncoding {
    match T::tree_hash_type() {
        TreeHashType::Basic => item.tree_hash_packed_encoding(),
        TreeHashType::Container | TreeHashType::List | TreeHashType::Vector => item.tree_hash_root().as_bytes().into(),
    }
}

pub fn tree_depth<N: Unsigned>() -> usize {
    // trailing zeroes is essentially a log2
    N::to_u64().next_power_of_two().trailing_zeros() as usize
}

pub fn target_tree_depth<T: TreeHash, N: Unsigned>() -> usize {
    let packing_factor = packing_factor::<T>();
    let packing_factor_log2 = packing_factor.trailing_zeros() as usize;
    tree_depth::<N>() - packing_factor_log2
}

#[cfg(not(target_arch = "riscv32"))]
mod default {
    use super::{Hash256, HashHelper, TreeHash, VariableList};
    use std::marker::PhantomData;
    pub struct HashHelperImpl<T: super::TreeHash> {
        _phatom: PhantomData<T>,
    }

    impl<T: TreeHash> HashHelper<T> for HashHelperImpl<T> {
        fn hash_list<N>(list: &VariableList<T, N>) -> Hash256
        where
            T: TreeHash,
            N: typenum::Unsigned,
        {
            list.tree_hash_root()
        }
    }
}

#[cfg(target_arch = "riscv32")]
mod riscv {
    use super::{item_encoding, pad_to_depth, target_tree_depth, Hash256, HashHelper, TreeHash, VariableList};
    use std::marker::PhantomData;
    use tree_hash::MerkleHasher;
    use typenum::Unsigned;

    pub struct HashHelperImpl<T: super::TreeHash> {
        _phatom: PhantomData<T>,
    }

    impl<T: super::TreeHash> HashHelperImpl<T> {
        const MAX_DEPTH: usize = 29;
    }

    impl<T: TreeHash> HashHelper<T> for HashHelperImpl<T> {
        fn hash_list<N: Unsigned>(list: &VariableList<T, N>) -> Hash256 {
            assert!((list.len() as u64) < (u32::MAX as u64));

            let main_tree_depth: usize = Self::MAX_DEPTH;
            let main_tree_elems: usize = (2_usize).pow(main_tree_depth as u32);

            let mut hasher = MerkleHasher::with_leaves(main_tree_elems);

            for item in list {
                hasher
                    .write(&item_encoding(item))
                    .expect("Failed to write item into hasher");
            }

            let actual_elements_root = hasher.finish().expect("Failed to finalize the hasher");
            let expanded_tree_root = pad_to_depth(&actual_elements_root, main_tree_depth, target_tree_depth::<T, N>());

            tree_hash::mix_in_length(&expanded_tree_root, list.len())
        }
    }
}

#[cfg(not(target_arch = "riscv32"))]
pub use default::HashHelperImpl;
#[cfg(target_arch = "riscv32")]
pub use riscv::HashHelperImpl;
