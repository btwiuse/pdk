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
mod config {
	use frame_support::traits::Randomness;
	use sp_runtime::traits::One;
	use sp_runtime::traits::CheckedAdd;
	/// Configure the pallet by specifying the parameters and types on which it depends.
	#[pallet::config]
	pub trait Config: frame_system::Config {
		/// The overarching runtime event type.
		type RuntimeEvent: From<Event<Self>> + IsType<<Self as frame_system::Config>::RuntimeEvent>;
		type CatId: sp_runtime::traits::AtLeast32BitUnsigned + codec::EncodeLike + Clone + Copy + Decode + scale_info::prelude::fmt::Debug + Default + Eq + PartialEq + TypeInfo + MaxEncodedLen + One;
		type Randomness: Randomness<Self::Hash, BlockNumberFor<Self>>;
		type WeightInfo: WeightInfo;
	}
}
