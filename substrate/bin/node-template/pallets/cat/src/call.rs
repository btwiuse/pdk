// This file is part of Substrate.

// Copyright (C) Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: Apache-2.0

// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
// 	http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

use frame_support::pallet_macros::*;

/// A [`pallet_section`] that defines the events for a pallet.
/// This can later be imported into the pallet using [`import_section`].
#[pallet_section]
mod call {
	// Dispatchable functions allows users to interact with the pallet and invoke state changes.
	// These functions materialize as "extrinsics", which are often compared to transactions.
	// Dispatchable functions must be annotated with a weight and must return a DispatchResult.

	#[pallet::call]
	impl<T: Config> Pallet<T> {
		#[pallet::weight(10_000)]
		pub fn create_cat(origin: OriginFor<T>) -> DispatchResult {
			let owner = ensure_signed(origin)?;
			let cat_id = Self::get_next_cat_id()?;
			let cat = Cat{dna:Self::random_cat_value(&owner), ..Cat::default() };

			let price = T::CatPrice::get();
			// T::Currency::reserve(&owner, price)?;
			T::Currency::transfer(
				&owner,
				&Self::get_account_id(),
				price,
				ExistenceRequirement::KeepAlive,
			)?;

			Cats::<T>::insert(cat_id, cat);
			CatOwner::<T>::insert(cat_id, &owner);

			Self::deposit_event(Event::CatCreated(owner, cat_id, cat));
			Ok(())
		}

		#[pallet::weight(10_000)]
		pub fn breed_cats(
			origin: OriginFor<T>,
			cat_id_1: T::CatId,
			cat_id_2: T::CatId,
		) -> DispatchResult {
			let owner = ensure_signed(origin)?;

			ensure!(cat_id_1 != cat_id_2, Error::<T>::SameCatId);

			let cat_1 = Self::cats(cat_id_1).ok_or(Error::<T>::InvalidCatId)?;
			let cat_2 = Self::cats(cat_id_2).ok_or(Error::<T>::InvalidCatId)?;

			let cat_id = Self::get_next_cat_id()?;

			let mut cat = Cat::default();
			let selector = Self::random_cat_value(&owner);
			for (i, (gene_1, gene_2)) in cat_1.dna.iter().zip(cat_2.dna.iter()).enumerate() {
				cat.dna[i] = (selector[i] & gene_1) | (!selector[i] & gene_2);
			}

			let price = T::CatPrice::get();
			// T::Currency::reserve(&owner, price)?;
			T::Currency::transfer(
				&owner,
				&Self::get_account_id(),
				price,
				ExistenceRequirement::KeepAlive,
			)?;

			Cats::<T>::insert(cat_id, cat);
			CatOwner::<T>::insert(cat_id, &owner);
			CatParents::<T>::insert(cat_id, (cat_id_1, cat_id_2));

			Self::deposit_event(Event::CatBred(owner, cat_id, cat));
			Ok(())
		}

		#[pallet::weight(10_000)]
		pub fn transfer_cat(
			origin: OriginFor<T>,
			to: T::AccountId,
			cat_id: T::CatId,
		) -> DispatchResult {
			let from = ensure_signed(origin)?;

			ensure!(CatOwner::<T>::contains_key(cat_id), Error::<T>::InvalidCatId);
			ensure!(Self::cat_owner(cat_id) == Some(from.clone()), Error::<T>::NotCatOwner);

			CatOwner::<T>::insert(cat_id, &to);

			Self::deposit_event(Event::CatTransferred(from, to, cat_id));
			Ok(())
		}

		#[pallet::weight(10_000)]
		pub fn list(origin: OriginFor<T>, cat_id: T::CatId) -> DispatchResult {
			let who = ensure_signed(origin)?;
			ensure!(Cats::<T>::contains_key(cat_id), Error::<T>::InvalidCatId);
			let owner = Self::cat_owner(cat_id).ok_or(Error::<T>::NotCatOwner)?;
			ensure!(owner == who, Error::<T>::NotCatOwner);
			ensure!(!CatListing::<T>::contains_key(cat_id), Error::<T>::AlreadyListed);

			CatListing::<T>::insert(cat_id, ());

			Self::deposit_event(Event::CatListed(who, cat_id));

			Ok(())
		}
		#[pallet::weight(10_000)]
		pub fn buy(origin: OriginFor<T>, cat_id: T::CatId) -> DispatchResult {
			let who = ensure_signed(origin)?;

			ensure!(Cats::<T>::contains_key(cat_id), Error::<T>::InvalidCatId);
			let owner = Self::cat_owner(cat_id).ok_or(Error::<T>::NotCatOwner)?;
			ensure!(owner != who, Error::<T>::AlreadyOwned);
			ensure!(CatListing::<T>::contains_key(cat_id), Error::<T>::NotListed);

			let price = T::CatPrice::get();
			T::Currency::transfer(&who, &owner, price, ExistenceRequirement::KeepAlive)?;

			// T::Currency::reserve(&who, &Self::get_account_id, price,
			// ExistenceRequirement::KeepAlive)?; T::Currency::unreserve(&owner, price);

			CatOwner::<T>::insert(cat_id, &who);
			CatListing::<T>::remove(cat_id);

			Self::deposit_event(Event::CatBought(who, cat_id));

			Ok(())
		}
	}

	use sp_io::hashing::blake2_128;

	impl<T: Config> Pallet<T> {
		fn get_next_cat_id() -> Result<T::CatId, DispatchError> {
			NextCatId::<T>::try_mutate(|next_id| -> Result<T::CatId, DispatchError> {
				let current_id = *next_id;
				*next_id = next_id.checked_add(&T::CatId::one()).ok_or(Error::<T>::InvalidCatId)?;
				Ok(current_id)
			})
		}

		fn random_cat_value(owner: &T::AccountId) -> [u8; 16] {
			let payload =
				(T::Randomness::random_seed(), owner, <frame_system::Pallet<T>>::extrinsic_index());
			payload.using_encoded(blake2_128)
		}

		fn get_account_id() -> T::AccountId {
			use sp_runtime::traits::AccountIdConversion;
			T::PalletId::get().into_account_truncating()
		}
	}
}
