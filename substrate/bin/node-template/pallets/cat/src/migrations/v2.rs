use codec::{Decode, Encode, MaxEncodedLen};
use frame_support::{
	migration::storage_key_iter, storage::StoragePrefixedMap,
	traits::GetStorageVersion, weights::Weight, Blake2_128Concat,
};
use scale_info::TypeInfo;

use crate::{Config, Cats, Pallet};

#[derive(Encode, Decode, Clone, Copy, Debug, PartialEq, Eq, TypeInfo, MaxEncodedLen)]
pub struct OldCat {
	dna: [u8; 16],
	name: [u8; 4],
}

pub fn migrate<T: Config>() -> Weight {
	let on_chain_version = Pallet::<T>::on_chain_storage_version();
	let current_version = Pallet::<T>::current_storage_version();

	if on_chain_version != 1 {
		return Weight::zero()
	}

	if current_version != 2 {
		return Weight::zero()
	}

	let module = Cats::<T>::pallet_prefix();
	let item = Cats::<T>::storage_prefix();

	for (index, old_cat) in
		storage_key_iter::<T::CatId, OldCat, Blake2_128Concat>(module, item).drain()
	{
		let mut name = [0u8; 8];
		name[0..4].copy_from_slice(&old_cat.name[..4]);
		let new_cat = crate::Cat { dna: old_cat.dna, name};
		Cats::<T>::insert(index, &new_cat);
	}

	Weight::zero()
}
