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

//! Autogenerated weights for `cumulus_pallet_xcmp_queue`
//!
//! THIS FILE WAS AUTO-GENERATED USING THE SUBSTRATE BENCHMARK CLI VERSION 32.0.0
//! DATE: 2025-05-05, STEPS: `50`, REPEAT: `20`, LOW RANGE: `[]`, HIGH RANGE: `[]`
//! WORST CASE MAP SIZE: `1000000`
//! HOSTNAME: `16d5a52ef0dc`, CPU: `Intel(R) Xeon(R) CPU @ 2.60GHz`
//! WASM-EXECUTION: `Compiled`, CHAIN: `None`, DB CACHE: 1024

// Executed Command:
// frame-omni-bencher
// v1
// benchmark
// pallet
// --extrinsic=*
// --runtime=target/production/wbuild/asset-hub-rococo-runtime/asset_hub_rococo_runtime.wasm
// --pallet=cumulus_pallet_xcmp_queue
// --header=/__w/polkadot-sdk/polkadot-sdk/cumulus/file_header.txt
// --output=./cumulus/parachains/runtimes/assets/asset-hub-rococo/src/weights
// --wasm-execution=compiled
// --steps=50
// --repeat=20
// --heap-pages=4096
// --no-storage-info
// --no-min-squares
// --no-median-slopes

#![cfg_attr(rustfmt, rustfmt_skip)]
#![allow(unused_parens)]
#![allow(unused_imports)]
#![allow(missing_docs)]

use frame_support::{traits::Get, weights::Weight};
use core::marker::PhantomData;

/// Weight functions for `cumulus_pallet_xcmp_queue`.
pub struct WeightInfo<T>(PhantomData<T>);
impl<T: frame_system::Config> cumulus_pallet_xcmp_queue::WeightInfo for WeightInfo<T> {
	/// Storage: `XcmpQueue::QueueConfig` (r:1 w:1)
	/// Proof: `XcmpQueue::QueueConfig` (`max_values`: Some(1), `max_size`: Some(12), added: 507, mode: `MaxEncodedLen`)
	fn set_config_with_u32() -> Weight {
		// Proof Size summary in bytes:
		//  Measured:  `109`
		//  Estimated: `1497`
		// Minimum execution time: 4_908_000 picoseconds.
		Weight::from_parts(5_133_000, 0)
			.saturating_add(Weight::from_parts(0, 1497))
			.saturating_add(T::DbWeight::get().reads(1))
			.saturating_add(T::DbWeight::get().writes(1))
	}
	/// Storage: `XcmpQueue::QueueConfig` (r:1 w:0)
	/// Proof: `XcmpQueue::QueueConfig` (`max_values`: Some(1), `max_size`: Some(12), added: 507, mode: `MaxEncodedLen`)
	/// Storage: `MessageQueue::BookStateFor` (r:1 w:1)
	/// Proof: `MessageQueue::BookStateFor` (`max_values`: None, `max_size`: Some(52), added: 2527, mode: `MaxEncodedLen`)
	/// Storage: `MessageQueue::ServiceHead` (r:1 w:1)
	/// Proof: `MessageQueue::ServiceHead` (`max_values`: Some(1), `max_size`: Some(5), added: 500, mode: `MaxEncodedLen`)
	/// Storage: `XcmpQueue::InboundXcmpSuspended` (r:1 w:0)
	/// Proof: `XcmpQueue::InboundXcmpSuspended` (`max_values`: Some(1), `max_size`: Some(4002), added: 4497, mode: `MaxEncodedLen`)
	/// Storage: `MessageQueue::Pages` (r:0 w:1)
	/// Proof: `MessageQueue::Pages` (`max_values`: None, `max_size`: Some(105521), added: 107996, mode: `MaxEncodedLen`)
	/// The range of component `n` is `[0, 105467]`.
	fn enqueue_n_bytes_xcmp_message(n: u32, ) -> Weight {
		// Proof Size summary in bytes:
		//  Measured:  `151`
		//  Estimated: `5487`
		// Minimum execution time: 13_653_000 picoseconds.
		Weight::from_parts(9_457_298, 0)
			.saturating_add(Weight::from_parts(0, 5487))
			// Standard Error: 6
			.saturating_add(Weight::from_parts(1_016, 0).saturating_mul(n.into()))
			.saturating_add(T::DbWeight::get().reads(4))
			.saturating_add(T::DbWeight::get().writes(3))
	}
	/// Storage: `XcmpQueue::QueueConfig` (r:1 w:0)
	/// Proof: `XcmpQueue::QueueConfig` (`max_values`: Some(1), `max_size`: Some(12), added: 507, mode: `MaxEncodedLen`)
	/// Storage: `MessageQueue::BookStateFor` (r:1 w:1)
	/// Proof: `MessageQueue::BookStateFor` (`max_values`: None, `max_size`: Some(52), added: 2527, mode: `MaxEncodedLen`)
	/// Storage: `MessageQueue::ServiceHead` (r:1 w:1)
	/// Proof: `MessageQueue::ServiceHead` (`max_values`: Some(1), `max_size`: Some(5), added: 500, mode: `MaxEncodedLen`)
	/// Storage: `XcmpQueue::InboundXcmpSuspended` (r:1 w:0)
	/// Proof: `XcmpQueue::InboundXcmpSuspended` (`max_values`: Some(1), `max_size`: Some(4002), added: 4497, mode: `MaxEncodedLen`)
	/// Storage: `MessageQueue::Pages` (r:0 w:1)
	/// Proof: `MessageQueue::Pages` (`max_values`: None, `max_size`: Some(105521), added: 107996, mode: `MaxEncodedLen`)
	/// The range of component `n` is `[0, 1000]`.
	fn enqueue_n_empty_xcmp_messages(n: u32, ) -> Weight {
		// Proof Size summary in bytes:
		//  Measured:  `151`
		//  Estimated: `5487`
		// Minimum execution time: 11_593_000 picoseconds.
		Weight::from_parts(15_263_900, 0)
			.saturating_add(Weight::from_parts(0, 5487))
			// Standard Error: 239
			.saturating_add(Weight::from_parts(136_065, 0).saturating_mul(n.into()))
			.saturating_add(T::DbWeight::get().reads(4))
			.saturating_add(T::DbWeight::get().writes(3))
	}
	/// Storage: `XcmpQueue::QueueConfig` (r:1 w:0)
	/// Proof: `XcmpQueue::QueueConfig` (`max_values`: Some(1), `max_size`: Some(12), added: 507, mode: `MaxEncodedLen`)
	/// Storage: `MessageQueue::BookStateFor` (r:1 w:1)
	/// Proof: `MessageQueue::BookStateFor` (`max_values`: None, `max_size`: Some(52), added: 2527, mode: `MaxEncodedLen`)
	/// Storage: `MessageQueue::Pages` (r:1 w:1)
	/// Proof: `MessageQueue::Pages` (`max_values`: None, `max_size`: Some(105521), added: 107996, mode: `MaxEncodedLen`)
	/// Storage: `XcmpQueue::InboundXcmpSuspended` (r:1 w:0)
	/// Proof: `XcmpQueue::InboundXcmpSuspended` (`max_values`: Some(1), `max_size`: Some(4002), added: 4497, mode: `MaxEncodedLen`)
	/// The range of component `n` is `[0, 105457]`.
	fn enqueue_empty_xcmp_message_at(n: u32, ) -> Weight {
		// Proof Size summary in bytes:
		//  Measured:  `334 + n * (1 ±0)`
		//  Estimated: `108986`
		// Minimum execution time: 20_217_000 picoseconds.
		Weight::from_parts(20_647_000, 0)
			.saturating_add(Weight::from_parts(0, 108986))
			// Standard Error: 12
			.saturating_add(Weight::from_parts(2_576, 0).saturating_mul(n.into()))
			.saturating_add(T::DbWeight::get().reads(4))
			.saturating_add(T::DbWeight::get().writes(2))
	}
	/// Storage: `XcmpQueue::QueueConfig` (r:1 w:0)
	/// Proof: `XcmpQueue::QueueConfig` (`max_values`: Some(1), `max_size`: Some(12), added: 507, mode: `MaxEncodedLen`)
	/// Storage: `MessageQueue::BookStateFor` (r:1 w:1)
	/// Proof: `MessageQueue::BookStateFor` (`max_values`: None, `max_size`: Some(52), added: 2527, mode: `MaxEncodedLen`)
	/// Storage: `MessageQueue::ServiceHead` (r:1 w:1)
	/// Proof: `MessageQueue::ServiceHead` (`max_values`: Some(1), `max_size`: Some(5), added: 500, mode: `MaxEncodedLen`)
	/// Storage: `XcmpQueue::InboundXcmpSuspended` (r:1 w:0)
	/// Proof: `XcmpQueue::InboundXcmpSuspended` (`max_values`: Some(1), `max_size`: Some(4002), added: 4497, mode: `MaxEncodedLen`)
	/// Storage: `MessageQueue::Pages` (r:0 w:100)
	/// Proof: `MessageQueue::Pages` (`max_values`: None, `max_size`: Some(105521), added: 107996, mode: `MaxEncodedLen`)
	/// The range of component `n` is `[0, 100]`.
	fn enqueue_n_full_pages(n: u32, ) -> Weight {
		// Proof Size summary in bytes:
		//  Measured:  `186`
		//  Estimated: `5487`
		// Minimum execution time: 13_262_000 picoseconds.
		Weight::from_parts(13_670_000, 0)
			.saturating_add(Weight::from_parts(0, 5487))
			// Standard Error: 81_154
			.saturating_add(Weight::from_parts(105_979_285, 0).saturating_mul(n.into()))
			.saturating_add(T::DbWeight::get().reads(4))
			.saturating_add(T::DbWeight::get().writes(2))
			.saturating_add(T::DbWeight::get().writes((1_u64).saturating_mul(n.into())))
	}
	/// Storage: `XcmpQueue::QueueConfig` (r:1 w:0)
	/// Proof: `XcmpQueue::QueueConfig` (`max_values`: Some(1), `max_size`: Some(12), added: 507, mode: `MaxEncodedLen`)
	/// Storage: `MessageQueue::BookStateFor` (r:1 w:1)
	/// Proof: `MessageQueue::BookStateFor` (`max_values`: None, `max_size`: Some(52), added: 2527, mode: `MaxEncodedLen`)
	/// Storage: `MessageQueue::Pages` (r:1 w:1)
	/// Proof: `MessageQueue::Pages` (`max_values`: None, `max_size`: Some(105521), added: 107996, mode: `MaxEncodedLen`)
	/// Storage: `XcmpQueue::InboundXcmpSuspended` (r:1 w:0)
	/// Proof: `XcmpQueue::InboundXcmpSuspended` (`max_values`: Some(1), `max_size`: Some(4002), added: 4497, mode: `MaxEncodedLen`)
	fn enqueue_1000_small_xcmp_messages() -> Weight {
		// Proof Size summary in bytes:
		//  Measured:  `53067`
		//  Estimated: `108986`
		// Minimum execution time: 289_304_000 picoseconds.
		Weight::from_parts(299_215_000, 0)
			.saturating_add(Weight::from_parts(0, 108986))
			.saturating_add(T::DbWeight::get().reads(4))
			.saturating_add(T::DbWeight::get().writes(2))
	}
	/// Storage: `XcmpQueue::OutboundXcmpStatus` (r:1 w:1)
	/// Proof: `XcmpQueue::OutboundXcmpStatus` (`max_values`: Some(1), `max_size`: Some(1282), added: 1777, mode: `MaxEncodedLen`)
	fn suspend_channel() -> Weight {
		// Proof Size summary in bytes:
		//  Measured:  `109`
		//  Estimated: `2767`
		// Minimum execution time: 3_121_000 picoseconds.
		Weight::from_parts(3_254_000, 0)
			.saturating_add(Weight::from_parts(0, 2767))
			.saturating_add(T::DbWeight::get().reads(1))
			.saturating_add(T::DbWeight::get().writes(1))
	}
	/// Storage: `XcmpQueue::OutboundXcmpStatus` (r:1 w:1)
	/// Proof: `XcmpQueue::OutboundXcmpStatus` (`max_values`: Some(1), `max_size`: Some(1282), added: 1777, mode: `MaxEncodedLen`)
	fn resume_channel() -> Weight {
		// Proof Size summary in bytes:
		//  Measured:  `144`
		//  Estimated: `2767`
		// Minimum execution time: 4_409_000 picoseconds.
		Weight::from_parts(4_555_000, 0)
			.saturating_add(Weight::from_parts(0, 2767))
			.saturating_add(T::DbWeight::get().reads(1))
			.saturating_add(T::DbWeight::get().writes(1))
	}
	fn take_first_concatenated_xcm() -> Weight {
		// Proof Size summary in bytes:
		//  Measured:  `0`
		//  Estimated: `0`
		// Minimum execution time: 5_368_000 picoseconds.
		Weight::from_parts(5_614_000, 0)
			.saturating_add(Weight::from_parts(0, 0))
	}
	/// Storage: UNKNOWN KEY `0x7b3237373ffdfeb1cab4222e3b520d6b345d8e88afa015075c945637c07e8f20` (r:1 w:1)
	/// Proof: UNKNOWN KEY `0x7b3237373ffdfeb1cab4222e3b520d6b345d8e88afa015075c945637c07e8f20` (r:1 w:1)
	/// Storage: UNKNOWN KEY `0x7b3237373ffdfeb1cab4222e3b520d6bedc49980ba3aa32b0a189290fd036649` (r:1 w:1)
	/// Proof: UNKNOWN KEY `0x7b3237373ffdfeb1cab4222e3b520d6bedc49980ba3aa32b0a189290fd036649` (r:1 w:1)
	/// Storage: `MessageQueue::BookStateFor` (r:1 w:1)
	/// Proof: `MessageQueue::BookStateFor` (`max_values`: None, `max_size`: Some(52), added: 2527, mode: `MaxEncodedLen`)
	/// Storage: `MessageQueue::ServiceHead` (r:1 w:1)
	/// Proof: `MessageQueue::ServiceHead` (`max_values`: Some(1), `max_size`: Some(5), added: 500, mode: `MaxEncodedLen`)
	/// Storage: `XcmpQueue::QueueConfig` (r:1 w:0)
	/// Proof: `XcmpQueue::QueueConfig` (`max_values`: Some(1), `max_size`: Some(12), added: 507, mode: `MaxEncodedLen`)
	/// Storage: `XcmpQueue::InboundXcmpSuspended` (r:1 w:0)
	/// Proof: `XcmpQueue::InboundXcmpSuspended` (`max_values`: Some(1), `max_size`: Some(4002), added: 4497, mode: `MaxEncodedLen`)
	/// Storage: `MessageQueue::Pages` (r:0 w:1)
	/// Proof: `MessageQueue::Pages` (`max_values`: None, `max_size`: Some(105521), added: 107996, mode: `MaxEncodedLen`)
	fn on_idle_good_msg() -> Weight {
		// Proof Size summary in bytes:
		//  Measured:  `105716`
		//  Estimated: `109181`
		// Minimum execution time: 231_957_000 picoseconds.
		Weight::from_parts(242_676_000, 0)
			.saturating_add(Weight::from_parts(0, 109181))
			.saturating_add(T::DbWeight::get().reads(6))
			.saturating_add(T::DbWeight::get().writes(5))
	}
	/// Storage: UNKNOWN KEY `0x7b3237373ffdfeb1cab4222e3b520d6b345d8e88afa015075c945637c07e8f20` (r:1 w:1)
	/// Proof: UNKNOWN KEY `0x7b3237373ffdfeb1cab4222e3b520d6b345d8e88afa015075c945637c07e8f20` (r:1 w:1)
	/// Storage: UNKNOWN KEY `0x7b3237373ffdfeb1cab4222e3b520d6bedc49980ba3aa32b0a189290fd036649` (r:1 w:1)
	/// Proof: UNKNOWN KEY `0x7b3237373ffdfeb1cab4222e3b520d6bedc49980ba3aa32b0a189290fd036649` (r:1 w:1)
	/// Storage: `MessageQueue::BookStateFor` (r:1 w:1)
	/// Proof: `MessageQueue::BookStateFor` (`max_values`: None, `max_size`: Some(52), added: 2527, mode: `MaxEncodedLen`)
	/// Storage: `MessageQueue::ServiceHead` (r:1 w:1)
	/// Proof: `MessageQueue::ServiceHead` (`max_values`: Some(1), `max_size`: Some(5), added: 500, mode: `MaxEncodedLen`)
	/// Storage: `XcmpQueue::QueueConfig` (r:1 w:0)
	/// Proof: `XcmpQueue::QueueConfig` (`max_values`: Some(1), `max_size`: Some(12), added: 507, mode: `MaxEncodedLen`)
	/// Storage: `XcmpQueue::InboundXcmpSuspended` (r:1 w:0)
	/// Proof: `XcmpQueue::InboundXcmpSuspended` (`max_values`: Some(1), `max_size`: Some(4002), added: 4497, mode: `MaxEncodedLen`)
	/// Storage: `MessageQueue::Pages` (r:0 w:1)
	/// Proof: `MessageQueue::Pages` (`max_values`: None, `max_size`: Some(105521), added: 107996, mode: `MaxEncodedLen`)
	fn on_idle_large_msg() -> Weight {
		// Proof Size summary in bytes:
		//  Measured:  `65785`
		//  Estimated: `69250`
		// Minimum execution time: 134_722_000 picoseconds.
		Weight::from_parts(138_495_000, 0)
			.saturating_add(Weight::from_parts(0, 69250))
			.saturating_add(T::DbWeight::get().reads(6))
			.saturating_add(T::DbWeight::get().writes(5))
	}
}
