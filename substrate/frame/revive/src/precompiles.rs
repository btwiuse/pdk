// This file is part of Substrate.

// Copyright (C) Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: Apache-2.0

// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
// http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

//! Exposes types that can be used to extend `pallet_revive` with additional functionality.
//!
//! In order to add a pre-compile:
//!
//! - Implement [`Precompile`] on a type. Most likely another pallet.
//! - Add the type to a tuple passed into [`Config::Precompiles`].
//! - Use the types inside the `run` module to test and benchmark your pre-compile.
//!
//! Use `alloy` through our re-export in this module to implement Eth ABI.

mod builtin;

mod tests;

pub use crate::{
	exec::{ExecError, PrecompileExt as Ext, PrecompileWithInfoExt as ExtWithInfo},
	gas::{GasMeter, Token},
	storage::meter::Diff,
	vm::RuntimeCosts,
	AddressMapper,
};
pub use alloy_core as alloy;
pub use sp_core::{H160, H256, U256};

use crate::{
	exec::ExecResult, precompiles::builtin::Builtin, primitives::ExecReturnValue, Config,
	Error as CrateError,
};
use alloc::vec::Vec;
use alloy::sol_types::{Panic, PanicKind, Revert, SolError, SolInterface};
use core::num::NonZero;
use pallet_revive_uapi::ReturnFlags;
use sp_runtime::DispatchError;

#[cfg(feature = "runtime-benchmarks")]
pub(crate) use builtin::{IBenchmarking, NoInfo as BenchmarkNoInfo, WithInfo as BenchmarkWithInfo};

const UNIMPLEMENTED: &str = "A precompile must either implement `call` or `call_with_info`";

/// A minimal EVM bytecode to be returned when a pre-compile is queried for its code.
pub(crate) const EVM_REVERT: [u8; 5] = sp_core::hex2array!("60006000fd");

/// The composition of all available pre-compiles.
///
/// This is how the rest of the pallet discovers and calls pre-compiles.
pub(crate) type All<T> = (Builtin<T>, <T as Config>::Precompiles);

/// Used by [`Precompile`] in order to declare at which addresses it will be called.
///
/// The 2 byte integer supplied here will be interpreted as big endian and copied to
/// `address[16,17]`. Address `address[18,19]` is reserved for builtin precompiles. All other
/// bytes are set to zero.
///
/// Big endian is chosen because it lines up with how you would invoke a pre-compile in Solidity.
/// For example writing `staticcall(..., 0x05, ...)` in Solidity sets the highest (`address[19]`)
/// byte to `5`.
pub enum AddressMatcher {
	/// The pre-compile will only be called for a single address.
	///
	/// This means the precompile will only be invoked for:
	/// ```ignore
	/// 00000000000000000000000000000000pppp0000
	/// ```
	///
	/// Where `p` is the `u16` defined here as big endian.
	Fixed(NonZero<u16>),
	/// The pre-compile will be called for multiple addresses.
	///
	/// This is useful when some information should be encoded into the address.
	///
	/// This means the precompile will be invoked for all `x`:
	/// ```ignore
	/// xxxxxxxx000000000000000000000000pppp0000
	/// ```
	///
	/// Where `p` is the `u16` defined here as big endian. Hence a maximum of 2 byte can be encoded
	/// into the address. Allowing more bytes could lead to the situation where legitimate
	/// accounts could exist at this address. Either by accident or on purpose.
	Prefix(NonZero<u16>),
}

/// Same as `AddressMatcher` but for builtin pre-compiles.
///
/// It works in the same way as `AddressMatcher` but allows setting the full 4 byte prefix.
/// Builtin pre-compiles must only use values `<= u16::MAX` to prevent collisions with
/// external pre-compiles.
pub(crate) enum BuiltinAddressMatcher {
	Fixed(NonZero<u32>),
	Prefix(NonZero<u32>),
}

/// A pre-compile can error in the same way that a real contract can.
#[derive(derive_more::From, Debug)]
pub enum Error {
	/// This is the same as a contract writing `revert("I reverted")`.
	///
	/// Those are the errors that are commonly caught by Solidity try-catch blocks. Encodes
	/// a string onto the output buffer.
	Revert(Revert),
	/// An error generated by Solidity itself.
	///
	/// Encodes an error code into the output buffer.
	Panic(PanicKind),
	/// Don't encode anything into the output buffer. Just trap.
	///
	/// Commonly used for out of gas or other resource errors.
	Error(ExecError),
}

impl From<DispatchError> for Error {
	fn from(error: DispatchError) -> Self {
		Self::Error(error.into())
	}
}

impl<T: Config> From<CrateError<T>> for Error {
	fn from(error: CrateError<T>) -> Self {
		Self::Error(DispatchError::from(error).into())
	}
}

/// Type that can be implemented in other crates to extend the list of pre-compiles.
///
/// Only implement exactly one function. Either `call` or `call_with_info`.
///
/// # Warning
///
/// Pre-compiles are unmetered code. Hence they have to charge an appropriate amount of weight
/// themselves. Generally, their first line of code should be a call to `env.charge(weight)`.
pub trait Precompile {
	/// Your runtime.
	type T: Config;
	/// The Solidity ABI definition of this pre-compile.
	///
	/// Use the [`self::alloy::sol`] macro to define your interface using Solidity syntax.
	/// The input the caller passes to the pre-compile will be validated and parsed
	/// according to this interface.
	///
	/// Please note that the return value is not validated and it is the pre-compiles
	/// duty to return the abi encoded bytes conformant with the interface here.
	type Interface: SolInterface;
	/// Defines at which addresses this pre-compile exists.
	const MATCHER: AddressMatcher;
	/// Defines whether this pre-compile needs a contract info data structure in storage.
	///
	/// Enabling it unlocks more APIs for the pre-compile to use. Only pre-compiles with a
	/// fixed matcher can set this to true. This is enforced at compile time. Reason is that
	/// contract info is per address and not per pre-compile. Too many contract info structures
	/// and accounts would be created otherwise.
	///
	/// # When set to **true**
	///
	/// - An account will be created at the pre-compiles address when it is called for the first
	///   time. The ed is minted.
	/// - Contract info data structure will be created in storage on first call.
	/// - Only `call_with_info` should be implemented. `call` is never called.
	///
	/// # When set to **false**
	///
	/// - No account or any other state will be created for the address.
	/// - Only `call` should be implemented. `call_with_info` is never called.
	///
	/// # What to use
	///
	/// Should be set to false if the additional functionality is not needed. A pre-compile with
	/// contract info will incur both a storage read and write to its contract metadata when called.
	///
	/// The contract info enables additional functionality:
	/// - Storage deposits: Collect deposits from the origin rather than the caller. This makes it
	///   easier for contracts to interact with the pre-compile as deposits
	/// 	are paid by the transaction signer (just like gas). It also makes refunding easier.
	/// - Contract storage: You can use the contracts key value child trie storage instead of
	///   providing your own state.
	/// 	The contract storage automatically takes care of deposits.
	/// 	Providing your own storage and using pallet_revive to collect deposits is also possible,
	/// though.
	/// - Instantitation: Contract instantiation requires the instantiator to have an account. This
	/// 	is because its nonce is used to derive the new contracts account id and child trie id.
	///
	/// Have a look at [`ExtWithInfo`] to learn about the additional APIs that a contract info
	/// unlocks.
	const HAS_CONTRACT_INFO: bool;

	/// Entry point for your pre-compile when `HAS_CONTRACT_INFO = false`.
	#[allow(unused_variables)]
	fn call(
		address: &[u8; 20],
		input: &Self::Interface,
		env: &mut impl Ext<T = Self::T>,
	) -> Result<Vec<u8>, Error> {
		unimplemented!("{UNIMPLEMENTED}")
	}

	/// Entry point for your pre-compile when `HAS_CONTRACT_INFO = true`.
	#[allow(unused_variables)]
	fn call_with_info(
		address: &[u8; 20],
		input: &Self::Interface,
		env: &mut impl ExtWithInfo<T = Self::T>,
	) -> Result<Vec<u8>, Error> {
		unimplemented!("{UNIMPLEMENTED}")
	}
}

/// Same as `Precompile` but meant to be used by builtin pre-compiles.
///
/// This enabled builtin precompiles to exist at the highest bits. Those are not
/// available to external pre-compiles in order to avoid collisions.
///
/// Automatically implemented for all types that implement `Precompile`.
pub(crate) trait BuiltinPrecompile {
	type T: Config;
	type Interface: SolInterface;
	const MATCHER: BuiltinAddressMatcher;
	const HAS_CONTRACT_INFO: bool;
	const CODE: &[u8] = &EVM_REVERT;

	fn call(
		_address: &[u8; 20],
		_input: &Self::Interface,
		_env: &mut impl Ext<T = Self::T>,
	) -> Result<Vec<u8>, Error> {
		unimplemented!("{UNIMPLEMENTED}")
	}

	fn call_with_info(
		_address: &[u8; 20],
		_input: &Self::Interface,
		_env: &mut impl ExtWithInfo<T = Self::T>,
	) -> Result<Vec<u8>, Error> {
		unimplemented!("{UNIMPLEMENTED}")
	}
}

/// A low level pre-compile that does not use Solidity ABI.
///
/// It is used to implement the original Ethereum pre-compiles which do not
/// use Solidity ABI but just encode inputs and outputs packed in memory.
///
/// Automatically implemented for all types that implement `BuiltinPrecompile`.
/// By extension also automatically implemented for all types implementing `Precompile`.
pub(crate) trait PrimitivePrecompile {
	type T: Config;
	const MATCHER: BuiltinAddressMatcher;
	const HAS_CONTRACT_INFO: bool;
	const CODE: &[u8] = &[];

	fn call(
		_address: &[u8; 20],
		_input: Vec<u8>,
		_env: &mut impl Ext<T = Self::T>,
	) -> Result<Vec<u8>, Error> {
		unimplemented!("{UNIMPLEMENTED}")
	}

	fn call_with_info(
		_address: &[u8; 20],
		_input: Vec<u8>,
		_env: &mut impl ExtWithInfo<T = Self::T>,
	) -> Result<Vec<u8>, Error> {
		unimplemented!("{UNIMPLEMENTED}")
	}
}

/// A pre-compile ready to be called.
pub(crate) struct Instance<E> {
	has_contract_info: bool,
	address: [u8; 20],
	/// This is the function inside `PrimitivePrecompile` at `address`.
	function: fn(&[u8; 20], Vec<u8>, &mut E) -> Result<Vec<u8>, Error>,
}

impl<E> Instance<E> {
	pub fn has_contract_info(&self) -> bool {
		self.has_contract_info
	}

	pub fn call(&self, input: Vec<u8>, env: &mut E) -> ExecResult {
		let result = (self.function)(&self.address, input, env);
		match result {
			Ok(data) => Ok(ExecReturnValue { flags: ReturnFlags::empty(), data }),
			Err(Error::Revert(msg)) =>
				Ok(ExecReturnValue { flags: ReturnFlags::REVERT, data: msg.abi_encode() }),
			Err(Error::Panic(kind)) => Ok(ExecReturnValue {
				flags: ReturnFlags::REVERT,
				data: Panic::from(kind).abi_encode(),
			}),
			Err(Error::Error(err)) => Err(err.into()),
		}
	}
}

/// A composition of pre-compiles.
///
/// Automatically implemented for tuples of types that implement any of the
/// pre-compile traits.
pub(crate) trait Precompiles<T: Config> {
	/// Used to generate compile time error when multiple pre-compiles use the same matcher.
	const CHECK_COLLISION: ();
	/// Does any of the pre-compiles use the range reserved for external pre-compiles.
	///
	/// This is just used to generate a compile time error if `Builtin` is using the external
	/// range by accident.
	const USES_EXTERNAL_RANGE: bool;

	/// Returns the code of the pre-compile.
	///
	/// Just used when queried by `EXTCODESIZE` or the RPC. It is just
	/// a bogus code that is never executed. Returns None if no pre-compile
	/// exists at the specified address.
	fn code(address: &[u8; 20]) -> Option<&'static [u8]>;

	/// Get a reference to a specific pre-compile.
	///
	/// Returns `None` if no pre-compile exists at `address`.
	fn get<E: ExtWithInfo<T = T>>(address: &[u8; 20]) -> Option<Instance<E>>;
}

impl<P: Precompile> BuiltinPrecompile for P {
	type T = <Self as Precompile>::T;
	type Interface = <Self as Precompile>::Interface;
	const MATCHER: BuiltinAddressMatcher = P::MATCHER.into_builtin();
	const HAS_CONTRACT_INFO: bool = P::HAS_CONTRACT_INFO;

	fn call(
		address: &[u8; 20],
		input: &Self::Interface,
		env: &mut impl Ext<T = Self::T>,
	) -> Result<Vec<u8>, Error> {
		Self::call(address, input, env)
	}

	fn call_with_info(
		address: &[u8; 20],
		input: &Self::Interface,
		env: &mut impl ExtWithInfo<T = Self::T>,
	) -> Result<Vec<u8>, Error> {
		Self::call_with_info(address, input, env)
	}
}

impl<P: BuiltinPrecompile> PrimitivePrecompile for P {
	type T = <Self as BuiltinPrecompile>::T;
	const MATCHER: BuiltinAddressMatcher = P::MATCHER;
	const HAS_CONTRACT_INFO: bool = P::HAS_CONTRACT_INFO;
	const CODE: &[u8] = P::CODE;

	fn call(
		address: &[u8; 20],
		input: Vec<u8>,
		env: &mut impl Ext<T = Self::T>,
	) -> Result<Vec<u8>, Error> {
		let call = <Self as BuiltinPrecompile>::Interface::abi_decode_validate(&input)
			.map_err(|_| Error::Panic(PanicKind::ResourceError))?;
		<Self as BuiltinPrecompile>::call(address, &call, env)
	}

	fn call_with_info(
		address: &[u8; 20],
		input: Vec<u8>,
		env: &mut impl ExtWithInfo<T = Self::T>,
	) -> Result<Vec<u8>, Error> {
		let call = <Self as BuiltinPrecompile>::Interface::abi_decode_validate(&input)
			.map_err(|_| Error::Panic(PanicKind::ResourceError))?;
		<Self as BuiltinPrecompile>::call_with_info(address, &call, env)
	}
}

#[impl_trait_for_tuples::impl_for_tuples(20)]
#[tuple_types_custom_trait_bound(PrimitivePrecompile<T=T>)]
impl<T: Config> Precompiles<T> for Tuple {
	const CHECK_COLLISION: () = {
		let matchers = [for_tuples!( #( Tuple::MATCHER ),* )];
		if BuiltinAddressMatcher::has_duplicates(&matchers) {
			panic!("Precompiles with duplicate matcher detected")
		}
		for_tuples!(
			#(
				let is_fixed = Tuple::MATCHER.is_fixed();
				let has_info = Tuple::HAS_CONTRACT_INFO;
				assert!(is_fixed || !has_info, "Only fixed precompiles can have a contract info.");
			)*
		);
	};
	const USES_EXTERNAL_RANGE: bool = {
		let mut uses_external = false;
		for_tuples!(
			#(
				if Tuple::MATCHER.suffix() > u16::MAX as u32 {
					uses_external = true;
				}
			)*
		);
		uses_external
	};

	fn code(address: &[u8; 20]) -> Option<&'static [u8]> {
		for_tuples!(
			#(
				if Tuple::MATCHER.matches(address) {
					return Some(Tuple::CODE)
				}
			)*
		);
		None
	}

	fn get<E: ExtWithInfo<T = T>>(address: &[u8; 20]) -> Option<Instance<E>> {
		let _ = <Self as Precompiles<T>>::CHECK_COLLISION;
		let mut instance: Option<Instance<E>> = None;
		for_tuples!(
			#(
				if Tuple::MATCHER.matches(address) {
					if Tuple::HAS_CONTRACT_INFO {
						instance = Some(Instance {
							address: *address,
							has_contract_info: true,
							function: Tuple::call_with_info,
						})
					} else {
						instance = Some(Instance {
							address: *address,
							has_contract_info: false,
							function: Tuple::call,
						})
					}
				}
			)*
		);
		instance
	}
}

impl<T: Config> Precompiles<T> for (Builtin<T>, <T as Config>::Precompiles) {
	const CHECK_COLLISION: () = {
		assert!(
			!<Builtin<T>>::USES_EXTERNAL_RANGE,
			"Builtin precompiles must not use addresses reserved for external precompiles"
		);
	};
	const USES_EXTERNAL_RANGE: bool = { <T as Config>::Precompiles::USES_EXTERNAL_RANGE };

	fn code(address: &[u8; 20]) -> Option<&'static [u8]> {
		<Builtin<T>>::code(address).or_else(|| <T as Config>::Precompiles::code(address))
	}

	fn get<E: ExtWithInfo<T = T>>(address: &[u8; 20]) -> Option<Instance<E>> {
		let _ = <Self as Precompiles<T>>::CHECK_COLLISION;
		<Builtin<T>>::get(address).or_else(|| <T as Config>::Precompiles::get(address))
	}
}

impl AddressMatcher {
	pub const fn base_address(&self) -> [u8; 20] {
		self.into_builtin().base_address()
	}

	pub const fn highest_address(&self) -> [u8; 20] {
		self.into_builtin().highest_address()
	}

	pub const fn matches(&self, address: &[u8; 20]) -> bool {
		self.into_builtin().matches(address)
	}

	const fn into_builtin(&self) -> BuiltinAddressMatcher {
		const fn left_shift(val: NonZero<u16>) -> NonZero<u32> {
			let shifted = (val.get() as u32) << 16;
			NonZero::new(shifted).expect(
				"Value was non zero before.
				The shift is small enough to not truncate any existing bits.
				Hence the value is still non zero; qed",
			)
		}

		match self {
			Self::Fixed(i) => BuiltinAddressMatcher::Fixed(left_shift(*i)),
			Self::Prefix(i) => BuiltinAddressMatcher::Prefix(left_shift(*i)),
		}
	}
}

impl BuiltinAddressMatcher {
	pub const fn base_address(&self) -> [u8; 20] {
		let suffix = self.suffix().to_be_bytes();
		let mut address = [0u8; 20];
		let mut i = 16;
		while i < address.len() {
			address[i] = suffix[i - 16];
			i = i + 1;
		}
		address
	}

	pub const fn highest_address(&self) -> [u8; 20] {
		let mut address = self.base_address();
		match self {
			Self::Fixed(_) => (),
			Self::Prefix(_) => {
				address[0] = 0xFF;
				address[1] = 0xFF;
				address[2] = 0xFF;
				address[3] = 0xFF;
			},
		}
		address
	}

	pub const fn matches(&self, address: &[u8; 20]) -> bool {
		let base_address = self.base_address();
		let mut i = match self {
			Self::Fixed(_) => 0,
			Self::Prefix(_) => 4,
		};
		while i < base_address.len() {
			if address[i] != base_address[i] {
				return false
			}
			i = i + 1;
		}
		true
	}

	const fn suffix(&self) -> u32 {
		match self {
			Self::Fixed(i) => i.get(),
			Self::Prefix(i) => i.get(),
		}
	}

	const fn has_duplicates(nums: &[Self]) -> bool {
		let len = nums.len();
		let mut i = 0;
		while i < len {
			let mut j = i + 1;
			while j < len {
				if nums[i].suffix() == nums[j].suffix() {
					return true;
				}
				j += 1;
			}
			i += 1;
		}
		false
	}

	const fn is_fixed(&self) -> bool {
		matches!(self, Self::Fixed(_))
	}
}

/// Types to run a pre-compile during testing or benchmarking.
///
/// Use the types exported from this module in order to test or benchmark
/// your pre-compile. Module only exists when compiles for benchmarking
/// or tests.
#[cfg(any(test, feature = "runtime-benchmarks"))]
pub mod run {
	pub use crate::{
		call_builder::{CallSetup, Contract, VmBinaryModule},
		BalanceOf, MomentOf,
	};
	pub use sp_core::{H256, U256};

	use super::*;

	/// Convenience function to run pre-compiles for testing or benchmarking purposes.
	///
	/// Use [`CallSetup`] to create an appropriate environment to pass as the `ext` parameter.
	/// Panics in case the `MATCHER` of `P` does not match the passed `address`.
	pub fn precompile<P, E>(
		ext: &mut E,
		address: &[u8; 20],
		input: &P::Interface,
	) -> Result<Vec<u8>, Error>
	where
		P: Precompile<T = E::T>,
		E: ExtWithInfo,
		BalanceOf<E::T>: Into<U256> + TryFrom<U256>,
		MomentOf<E::T>: Into<U256>,
		<<E as Ext>::T as frame_system::Config>::Hash: frame_support::traits::IsType<H256>,
	{
		assert!(P::MATCHER.into_builtin().matches(address));
		if P::HAS_CONTRACT_INFO {
			P::call_with_info(address, input, ext)
		} else {
			P::call(address, input, ext)
		}
	}

	/// Convenience function to run builtin pre-compiles from benchmarks.
	#[cfg(feature = "runtime-benchmarks")]
	pub(crate) fn builtin<E>(ext: &mut E, address: &[u8; 20], input: Vec<u8>) -> ExecResult
	where
		E: ExtWithInfo,
		BalanceOf<E::T>: Into<U256> + TryFrom<U256>,
		MomentOf<E::T>: Into<U256>,
		<<E as Ext>::T as frame_system::Config>::Hash: frame_support::traits::IsType<H256>,
	{
		let precompile = <Builtin<E::T>>::get(address)
			.ok_or(DispatchError::from("No pre-compile at address"))?;
		precompile.call(input, ext)
	}
}
