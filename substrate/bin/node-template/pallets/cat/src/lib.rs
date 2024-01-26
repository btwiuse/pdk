#![cfg_attr(not(feature = "std"), no_std)]

#[cfg(test)]
mod mock;

#[cfg(test)]
mod tests;

#[cfg(feature = "runtime-benchmarks")]
mod benchmarking;
pub mod weights;
pub use weights::*;

// Re-export pallet items so that they can be accessed from the crate namespace.
pub use pallet::*;

#[frame_support::pallet]
pub mod pallet {
    use super::*;
    use frame_support::pallet_prelude::*;
    use frame_system::pallet_prelude::*;

    use frame_support::traits::Randomness;
    use sp_io::hashing::blake2_128;
	use sp_runtime::traits::One;
	use sp_runtime::traits::CheckedAdd;

	#[pallet::config]
	pub trait Config: frame_system::Config {
		/// The overarching runtime event type.
		type RuntimeEvent: From<Event<Self>> + IsType<<Self as frame_system::Config>::RuntimeEvent>;
		type CatId: sp_runtime::traits::AtLeast32BitUnsigned + codec::EncodeLike + Clone + Copy + Decode + scale_info::prelude::fmt::Debug + Default + Eq + PartialEq + TypeInfo + MaxEncodedLen + One;
		type Randomness: Randomness<Self::Hash, BlockNumberFor<Self>>;
		type WeightInfo: WeightInfo;
	}

    #[derive(
        Encode, Decode, Clone, Copy, RuntimeDebug, PartialEq, Eq, Default, TypeInfo, MaxEncodedLen,
    )]
    pub struct Cat(pub [u8; 16]);

    #[pallet::pallet]
    pub struct Pallet<T>(_);

    #[pallet::storage]
    #[pallet::getter(fn next_cat_id)]
    pub type NextCatId<T: Config> = StorageValue<_, T::CatId, ValueQuery>;

    #[pallet::storage]
    #[pallet::getter(fn cats)]
    pub type Cats<T: Config> = StorageMap<_, Blake2_128Concat, T::CatId, Cat>;

    #[pallet::storage]
    #[pallet::getter(fn cat_owner)]
    pub type CatOwner<T: Config> = StorageMap<_, Blake2_128Concat, T::CatId, T::AccountId>;

    #[pallet::storage]
    #[pallet::getter(fn cat_parents)]
    pub type CatParents<T: Config> = StorageMap<_, Blake2_128Concat, T::CatId, (T::CatId, T::CatId)>;

    #[pallet::event]
    #[pallet::generate_deposit(pub(super) fn deposit_event)]
    pub enum Event<T: Config> {
        CatCreated(T::AccountId, T::CatId, Cat),
        CatBred(T::AccountId, T::CatId, Cat),
        CatTransferred(T::AccountId, T::AccountId, T::CatId),
    }

    #[pallet::error]
    pub enum Error<T> {
        InvalidCatId,
        SameCatId,
        NotCatOwner,
    }

    #[pallet::call]
    impl<T: Config> Pallet<T> {
        #[pallet::weight(10_000)]
        pub fn create_cat(origin: OriginFor<T>) -> DispatchResult {
            let owner = ensure_signed(origin)?;
            let cat_id = Self::get_next_cat_id()?;
            let cat = Cat(Self::random_cat_value(&owner));

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
            for (i, (gene_1, gene_2)) in cat_1.0.iter().zip(cat_2.0.iter()).enumerate() {
                cat.0[i] = (selector[i] & gene_1) | (!selector[i] & gene_2);
            }

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
    }

    impl<T: Config> Pallet<T> {
        fn get_next_cat_id() -> Result<T::CatId, DispatchError> {
            NextCatId::<T>::try_mutate(|next_id| -> Result<T::CatId, DispatchError> {
                let current_id = *next_id;
                *next_id = next_id.checked_add(&T::CatId::one()).ok_or(Error::<T>::InvalidCatId)?;
                Ok(current_id)
            })
        }

        fn random_cat_value(owner: &T::AccountId) -> [u8; 16] {
            let payload = (
                T::Randomness::random_seed(),
                owner,
                <frame_system::Pallet<T>>::extrinsic_index(),
            );
            payload.using_encoded(blake2_128)
        }
    }
}