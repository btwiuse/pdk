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

//! # Split Example Pallet
//!
//! **This pallet serves as an example and is not meant to be used in production.**
//!
//! A FRAME pallet demonstrating the ability to split sections across multiple files.
//!
//! Note that this is purely experimental at this point.

#![cfg_attr(not(feature = "std"), no_std)]

// Re-export pallet items so that they can be accessed from the crate namespace.
pub use pallet::*;

#[cfg(test)]
mod mock;

#[cfg(test)]
mod tests;

#[cfg(feature = "runtime-benchmarks")]
mod benchmarking;
mod events;
mod errors;
mod call;
mod config;

pub mod weights;
pub use weights::*;

use frame_support::pallet_macros::*;

/// Imports a [`pallet_section`] defined at [`events::events`].
/// This brings the events defined in that section into the pallet's namespace.
#[import_section(events::events)]
#[import_section(errors::errors)]
#[import_section(call::call)]
#[import_section(config::config)]
#[frame_support::pallet]
pub mod pallet {
	use super::*;
	use frame_support::pallet_prelude::*;
	use frame_system::pallet_prelude::*;

	#[pallet::pallet]
	pub struct Pallet<T>(_);

	// The pallet's runtime storage items.
	#[pallet::storage]
	pub type Something<T> = StorageValue<_, u32>;
}
