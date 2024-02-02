#![cfg_attr(not(feature = "std"), no_std)]


#[cfg(test)]
mod mock;

#[cfg(test)]
mod tests;

#[cfg(feature = "runtime-benchmarks")]
mod benchmarking;

mod errors;
mod call;
mod events;
mod config;

pub mod weights;
pub use weights::*;

// Re-export pallet items so that they can be accessed from the crate namespace.
pub use pallet::*;
use frame_support::pallet_macros::*;

#[import_section(config::config)]
#[import_section(events::events)]
#[import_section(errors::errors)]
#[import_section(call::call)]
#[frame_support::pallet]
pub mod pallet {
    use super::*;
    use frame_support::pallet_prelude::*;
    use frame_system::pallet_prelude::*;

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
}