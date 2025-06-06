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

//! This module contains functions to meter the storage deposit.

use crate::{
	storage::ContractInfo, AccountIdOf, BalanceOf, Config, Error, HoldReason, Inspect, Origin,
	StorageDeposit as Deposit, System, LOG_TARGET,
};
use alloc::vec::Vec;
use core::{fmt::Debug, marker::PhantomData};
use frame_support::{
	traits::{
		fungible::{Mutate, MutateHold},
		tokens::{Fortitude, Fortitude::Polite, Precision, Preservation, Restriction},
		Get,
	},
	DefaultNoBound, RuntimeDebugNoBound,
};
use sp_runtime::{
	traits::{Saturating, Zero},
	DispatchError, FixedPointNumber, FixedU128,
};

/// Deposit that uses the native fungible's balance type.
pub type DepositOf<T> = Deposit<BalanceOf<T>>;

/// A production root storage meter that actually charges from its origin.
pub type Meter<T> = RawMeter<T, ReservingExt, Root>;

/// A production nested storage meter that actually charges from its origin.
pub type NestedMeter<T> = RawMeter<T, ReservingExt, Nested>;

/// A production storage meter that actually charges from its origin.
///
/// This can be used where we want to be generic over the state (Root vs. Nested).
pub type GenericMeter<T, S> = RawMeter<T, ReservingExt, S>;

/// A trait that allows to decouple the metering from the charging of balance.
///
/// This mostly exists for testing so that the charging can be mocked.
pub trait Ext<T: Config> {
	/// This is called to inform the implementer that some balance should be charged due to
	/// some interaction of the `origin` with a `contract`.
	///
	/// The balance transfer can either flow from `origin` to `contract` or the other way
	/// around depending on whether `amount` constitutes a `Charge` or a `Refund`.
	/// It will fail in case the `origin` has not enough balance to cover all storage deposits.
	fn charge(
		origin: &T::AccountId,
		contract: &T::AccountId,
		amount: &DepositOf<T>,
		state: &ContractState<T>,
	) -> Result<(), DispatchError>;
}

/// This [`Ext`] is used for actual on-chain execution when balance needs to be charged.
///
/// It uses [`frame_support::traits::fungible::Mutate`] in order to do accomplish the reserves.
pub enum ReservingExt {}

/// Used to implement a type state pattern for the meter.
///
/// It is sealed and cannot be implemented outside of this module.
pub trait State: private::Sealed {}

/// State parameter that constitutes a meter that is in its root state.
#[derive(Default, Debug)]
pub struct Root;

/// State parameter that constitutes a meter that is in its nested state.
/// Its value indicates whether the nested meter has its own limit.
#[derive(Default, Debug)]
pub struct Nested;

impl State for Root {}
impl State for Nested {}

/// A type that allows the metering of consumed or freed storage of a single contract call stack.
#[derive(DefaultNoBound, RuntimeDebugNoBound)]
pub struct RawMeter<T: Config, E, S: State + Default + Debug> {
	/// The limit of how much balance this meter is allowed to consume.
	limit: BalanceOf<T>,
	/// The amount of balance that was used in this meter and all of its already absorbed children.
	total_deposit: DepositOf<T>,
	/// The amount of storage changes that were recorded in this meter alone.
	own_contribution: Contribution<T>,
	/// List of charges that should be applied at the end of a contract stack execution.
	///
	/// We only have one charge per contract hence the size of this vector is
	/// limited by the maximum call depth.
	charges: Vec<Charge<T>>,
	/// Type parameter only used in impls.
	_phantom: PhantomData<(E, S)>,
}

/// This type is used to describe a storage change when charging from the meter.
#[derive(Default, RuntimeDebugNoBound)]
pub struct Diff {
	/// How many bytes were added to storage.
	pub bytes_added: u32,
	/// How many bytes were removed from storage.
	pub bytes_removed: u32,
	/// How many storage items were added to storage.
	pub items_added: u32,
	/// How many storage items were removed from storage.
	pub items_removed: u32,
}

impl Diff {
	/// Calculate how much of a charge or refund results from applying the diff and store it
	/// in the passed `info` if any.
	///
	/// # Note
	///
	/// In case `None` is passed for `info` only charges are calculated. This is because refunds
	/// are calculated pro rata of the existing storage within a contract and hence need extract
	/// this information from the passed `info`.
	pub fn update_contract<T: Config>(&self, info: Option<&mut ContractInfo<T>>) -> DepositOf<T> {
		let per_byte = T::DepositPerByte::get();
		let per_item = T::DepositPerItem::get();
		let bytes_added = self.bytes_added.saturating_sub(self.bytes_removed);
		let items_added = self.items_added.saturating_sub(self.items_removed);
		let mut bytes_deposit = Deposit::Charge(per_byte.saturating_mul((bytes_added).into()));
		let mut items_deposit = Deposit::Charge(per_item.saturating_mul((items_added).into()));

		// Without any contract info we can only calculate diffs which add storage
		let info = if let Some(info) = info {
			info
		} else {
			debug_assert_eq!(self.bytes_removed, 0);
			debug_assert_eq!(self.items_removed, 0);
			return bytes_deposit.saturating_add(&items_deposit)
		};

		// Refunds are calculated pro rata based on the accumulated storage within the contract
		let bytes_removed = self.bytes_removed.saturating_sub(self.bytes_added);
		let items_removed = self.items_removed.saturating_sub(self.items_added);
		let ratio = FixedU128::checked_from_rational(bytes_removed, info.storage_bytes)
			.unwrap_or_default()
			.min(FixedU128::from_u32(1));
		bytes_deposit = bytes_deposit
			.saturating_add(&Deposit::Refund(ratio.saturating_mul_int(info.storage_byte_deposit)));
		let ratio = FixedU128::checked_from_rational(items_removed, info.storage_items)
			.unwrap_or_default()
			.min(FixedU128::from_u32(1));
		items_deposit = items_deposit
			.saturating_add(&Deposit::Refund(ratio.saturating_mul_int(info.storage_item_deposit)));

		// We need to update the contract info structure with the new deposits
		info.storage_bytes =
			info.storage_bytes.saturating_add(bytes_added).saturating_sub(bytes_removed);
		info.storage_items =
			info.storage_items.saturating_add(items_added).saturating_sub(items_removed);
		match &bytes_deposit {
			Deposit::Charge(amount) =>
				info.storage_byte_deposit = info.storage_byte_deposit.saturating_add(*amount),
			Deposit::Refund(amount) =>
				info.storage_byte_deposit = info.storage_byte_deposit.saturating_sub(*amount),
		}
		match &items_deposit {
			Deposit::Charge(amount) =>
				info.storage_item_deposit = info.storage_item_deposit.saturating_add(*amount),
			Deposit::Refund(amount) =>
				info.storage_item_deposit = info.storage_item_deposit.saturating_sub(*amount),
		}

		bytes_deposit.saturating_add(&items_deposit)
	}
}

impl Diff {
	fn saturating_add(&self, rhs: &Self) -> Self {
		Self {
			bytes_added: self.bytes_added.saturating_add(rhs.bytes_added),
			bytes_removed: self.bytes_removed.saturating_add(rhs.bytes_removed),
			items_added: self.items_added.saturating_add(rhs.items_added),
			items_removed: self.items_removed.saturating_add(rhs.items_removed),
		}
	}
}

/// The state of a contract.
///
/// In case of termination the beneficiary is indicated.
#[derive(RuntimeDebugNoBound, Clone, PartialEq, Eq)]
pub enum ContractState<T: Config> {
	Alive,
	Terminated { beneficiary: AccountIdOf<T> },
}

/// Records information to charge or refund a plain account.
///
/// All the charges are deferred to the end of a whole call stack. Reason is that by doing
/// this we can do all the refunds before doing any charge. This way a plain account can use
/// more deposit than it has balance as along as it is covered by a refund. This
/// essentially makes the order of storage changes irrelevant with regard to the deposit system.
/// The only exception is when a special (tougher) deposit limit is specified for a cross-contract
/// call. In that case the limit is enforced once the call is returned, rolling it back if
/// exhausted.
#[derive(RuntimeDebugNoBound, Clone)]
struct Charge<T: Config> {
	contract: T::AccountId,
	amount: DepositOf<T>,
	state: ContractState<T>,
}

/// Records the storage changes of a storage meter.
#[derive(RuntimeDebugNoBound)]
enum Contribution<T: Config> {
	/// The contract the meter belongs to is alive and accumulates changes using a [`Diff`].
	Alive(Diff),
	/// The meter was checked against its limit using [`RawMeter::enforce_limit`] at the end of
	/// its execution. In this process the [`Diff`] was converted into a [`Deposit`].
	Checked(DepositOf<T>),
	/// The contract was terminated. In this process the [`Diff`] was converted into a [`Deposit`]
	/// in order to calculate the refund. Upon termination the `reducible_balance` in the
	/// contract's account is transferred to the [`beneficiary`].
	Terminated { deposit: DepositOf<T>, beneficiary: AccountIdOf<T> },
}

impl<T: Config> Contribution<T> {
	/// See [`Diff::update_contract`].
	fn update_contract(&self, info: Option<&mut ContractInfo<T>>) -> DepositOf<T> {
		match self {
			Self::Alive(diff) => diff.update_contract::<T>(info),
			Self::Terminated { deposit, beneficiary: _ } | Self::Checked(deposit) =>
				deposit.clone(),
		}
	}
}

impl<T: Config> Default for Contribution<T> {
	fn default() -> Self {
		Self::Alive(Default::default())
	}
}

/// Functions that apply to all states.
impl<T, E, S> RawMeter<T, E, S>
where
	T: Config,
	E: Ext<T>,
	S: State + Default + Debug,
{
	/// Create a new child that has its `limit`.
	///
	/// This is called whenever a new subcall is initiated in order to track the storage
	/// usage for this sub call separately. This is necessary because we want to exchange balance
	/// with the current contract we are interacting with.
	pub fn nested(&self, limit: BalanceOf<T>) -> RawMeter<T, E, Nested> {
		debug_assert!(matches!(self.contract_state(), ContractState::Alive));

		RawMeter { limit: self.available().min(limit), ..Default::default() }
	}

	/// Absorb a child that was spawned to handle a sub call.
	///
	/// This should be called whenever a sub call comes to its end and it is **not** reverted.
	/// This does the actual balance transfer from/to `origin` and `contract` based on the
	/// overall storage consumption of the call. It also updates the supplied contract info.
	///
	/// In case a contract reverted the child meter should just be dropped in order to revert
	/// any changes it recorded.
	///
	/// # Parameters
	///
	/// - `absorbed`: The child storage meter that should be absorbed.
	/// - `origin`: The origin that spawned the original root meter.
	/// - `contract`: The contract's account that this sub call belongs to.
	/// - `info`: The info of the contract in question. `None` if the contract was terminated.
	pub fn absorb(
		&mut self,
		absorbed: RawMeter<T, E, Nested>,
		contract: &T::AccountId,
		info: Option<&mut ContractInfo<T>>,
	) {
		let own_deposit = absorbed.own_contribution.update_contract(info);
		self.total_deposit = self
			.total_deposit
			.saturating_add(&absorbed.total_deposit)
			.saturating_add(&own_deposit);
		self.charges.extend_from_slice(&absorbed.charges);
		if !own_deposit.is_zero() {
			self.charges.push(Charge {
				contract: contract.clone(),
				amount: own_deposit,
				state: absorbed.contract_state(),
			});
		}
	}

	/// Record a charge that has taken place externally.
	///
	/// This will not perform a charge. It just records it to reflect it in the
	/// total amount of storage required for a transaction.
	pub fn record_charge(&mut self, amount: &DepositOf<T>) {
		self.total_deposit = self.total_deposit.saturating_add(&amount);
	}

	/// The amount of balance that is still available from the original `limit`.
	fn available(&self) -> BalanceOf<T> {
		self.total_deposit.available(&self.limit)
	}

	/// Returns the state of the currently executed contract.
	fn contract_state(&self) -> ContractState<T> {
		match &self.own_contribution {
			Contribution::Terminated { deposit: _, beneficiary } =>
				ContractState::Terminated { beneficiary: beneficiary.clone() },
			_ => ContractState::Alive,
		}
	}
}

/// Functions that only apply to the root state.
impl<T, E> RawMeter<T, E, Root>
where
	T: Config,
	E: Ext<T>,
{
	/// Create new storage limiting storage deposits to the passed `limit`.
	///
	/// If the limit larger then what the origin can afford we will just fail
	/// when collecting the deposits in `try_into_deposit`.
	pub fn new(limit: BalanceOf<T>) -> Self {
		Self { limit, ..Default::default() }
	}

	/// Create new storage meter without checking the limit.
	pub fn new_unchecked(limit: BalanceOf<T>) -> Self {
		return Self { limit, ..Default::default() }
	}

	/// The total amount of deposit that should change hands as result of the execution
	/// that this meter was passed into. This will also perform all the charges accumulated
	/// in the whole contract stack.
	///
	/// This drops the root meter in order to make sure it is only called when the whole
	/// execution did finish.
	pub fn try_into_deposit(
		self,
		origin: &Origin<T>,
		skip_transfer: bool,
	) -> Result<DepositOf<T>, DispatchError> {
		if !skip_transfer {
			// Only refund or charge deposit if the origin is not root.
			let origin = match origin {
				Origin::Root => return Ok(Deposit::Charge(Zero::zero())),
				Origin::Signed(o) => o,
			};
			let try_charge = || {
				for charge in self.charges.iter().filter(|c| matches!(c.amount, Deposit::Refund(_)))
				{
					E::charge(origin, &charge.contract, &charge.amount, &charge.state)?;
				}
				for charge in self.charges.iter().filter(|c| matches!(c.amount, Deposit::Charge(_)))
				{
					E::charge(origin, &charge.contract, &charge.amount, &charge.state)?;
				}
				Ok(())
			};
			try_charge().map_err(|_: DispatchError| <Error<T>>::StorageDepositNotEnoughFunds)?;
		}

		Ok(self.total_deposit)
	}
}

/// Functions that only apply to the nested state.
impl<T: Config, E: Ext<T>> RawMeter<T, E, Nested> {
	/// Charges `diff` from the meter.
	pub fn charge(&mut self, diff: &Diff) {
		match &mut self.own_contribution {
			Contribution::Alive(own) => *own = own.saturating_add(diff),
			_ => panic!("Charge is never called after termination; qed"),
		};
	}

	/// Adds a charge without recording it in the contract info.
	///
	/// Use this method instead of [`Self::charge`] when the charge is not the result of a storage
	/// change within the contract's child trie. This is the case when when the `code_hash` is
	/// updated. [`Self::charge`] cannot be used here because we keep track of the deposit charge
	/// separately from the storage charge.
	///
	/// If this functions is used the amount of the charge has to be stored by the caller somewhere
	/// alese in order to be able to refund it.
	pub fn charge_deposit(&mut self, contract: T::AccountId, amount: DepositOf<T>) {
		self.record_charge(&amount);
		self.charges.push(Charge { contract, amount, state: ContractState::Alive });
	}

	/// Call to tell the meter that the currently executing contract was terminated.
	///
	/// This will manipulate the meter so that all storage deposit accumulated in
	/// `contract_info` will be refunded to the `origin` of the meter. And the free
	/// (`reducible_balance`) will be sent to the `beneficiary`.
	pub fn terminate(&mut self, info: &ContractInfo<T>, beneficiary: T::AccountId) {
		debug_assert!(matches!(self.contract_state(), ContractState::Alive));
		self.own_contribution = Contribution::Terminated {
			deposit: Deposit::Refund(info.total_deposit()),
			beneficiary,
		};
	}

	/// [`Self::charge`] does not enforce the storage limit since we want to do this check as late
	/// as possible to allow later refunds to offset earlier charges.
	pub fn enforce_limit(
		&mut self,
		info: Option<&mut ContractInfo<T>>,
	) -> Result<(), DispatchError> {
		let deposit = self.own_contribution.update_contract(info);
		let total_deposit = self.total_deposit.saturating_add(&deposit);
		// We don't want to override a `Terminated` with a `Checked`.
		if matches!(self.contract_state(), ContractState::Alive) {
			self.own_contribution = Contribution::Checked(deposit);
		}
		if let Deposit::Charge(amount) = total_deposit {
			if amount > self.limit {
				log::debug!( target: LOG_TARGET, "Storage deposit limit exhausted: {:?} > {:?}", amount, self.limit);
				return Err(<Error<T>>::StorageDepositLimitExhausted.into())
			}
		}
		Ok(())
	}
}

impl<T: Config> Ext<T> for ReservingExt {
	fn charge(
		origin: &T::AccountId,
		contract: &T::AccountId,
		amount: &DepositOf<T>,
		state: &ContractState<T>,
	) -> Result<(), DispatchError> {
		match amount {
			Deposit::Charge(amount) | Deposit::Refund(amount) if amount.is_zero() => return Ok(()),
			Deposit::Charge(amount) => {
				T::Currency::transfer_and_hold(
					&HoldReason::StorageDepositReserve.into(),
					origin,
					contract,
					*amount,
					Precision::Exact,
					Preservation::Preserve,
					Fortitude::Polite,
				)?;
			},
			Deposit::Refund(amount) => {
				let transferred = T::Currency::transfer_on_hold(
					&HoldReason::StorageDepositReserve.into(),
					contract,
					origin,
					*amount,
					Precision::BestEffort,
					Restriction::Free,
					Fortitude::Polite,
				)?;

				if transferred < *amount {
					// This should never happen, if it does it means that there is a bug in the
					// runtime logic. In the rare case this happens we try to refund as much as we
					// can, thus the `Precision::BestEffort`.
					log::error!(
						target: LOG_TARGET,
						"Failed to repatriate full storage deposit {:?} from contract {:?} to origin {:?}. Transferred {:?}.",
						amount, contract, origin, transferred,
					);
				}
			},
		}
		if let ContractState::<T>::Terminated { beneficiary } = state {
			System::<T>::dec_consumers(&contract);
			// Whatever is left in the contract is sent to the termination beneficiary.
			T::Currency::transfer(
				&contract,
				&beneficiary,
				T::Currency::reducible_balance(&contract, Preservation::Expendable, Polite),
				Preservation::Expendable,
			)?;
		}
		Ok(())
	}
}

mod private {
	pub trait Sealed {}
	impl Sealed for super::Root {}
	impl Sealed for super::Nested {}
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::{exec::AccountIdOf, test_utils::*, tests::Test};
	use frame_support::parameter_types;
	use pretty_assertions::assert_eq;

	type TestMeter = RawMeter<Test, TestExt, Root>;

	parameter_types! {
		static TestExtTestValue: TestExt = Default::default();
	}

	#[derive(Debug, PartialEq, Eq, Clone)]
	struct Charge {
		origin: AccountIdOf<Test>,
		contract: AccountIdOf<Test>,
		amount: DepositOf<Test>,
		state: ContractState<Test>,
	}

	#[derive(Default, Debug, PartialEq, Eq, Clone)]
	pub struct TestExt {
		charges: Vec<Charge>,
	}

	impl TestExt {
		fn clear(&mut self) {
			self.charges.clear();
		}
	}

	impl Ext<Test> for TestExt {
		fn charge(
			origin: &AccountIdOf<Test>,
			contract: &AccountIdOf<Test>,
			amount: &DepositOf<Test>,
			state: &ContractState<Test>,
		) -> Result<(), DispatchError> {
			TestExtTestValue::mutate(|ext| {
				ext.charges.push(Charge {
					origin: origin.clone(),
					contract: contract.clone(),
					amount: amount.clone(),
					state: state.clone(),
				})
			});
			Ok(())
		}
	}

	fn clear_ext() {
		TestExtTestValue::mutate(|ext| ext.clear())
	}

	struct ChargingTestCase {
		origin: Origin<Test>,
		deposit: DepositOf<Test>,
		expected: TestExt,
	}

	#[derive(Default)]
	struct StorageInfo {
		bytes: u32,
		items: u32,
		bytes_deposit: BalanceOf<Test>,
		items_deposit: BalanceOf<Test>,
		immutable_data_len: u32,
	}

	fn new_info(info: StorageInfo) -> ContractInfo<Test> {
		ContractInfo::<Test> {
			trie_id: Default::default(),
			code_hash: Default::default(),
			storage_bytes: info.bytes,
			storage_items: info.items,
			storage_byte_deposit: info.bytes_deposit,
			storage_item_deposit: info.items_deposit,
			storage_base_deposit: Default::default(),
			immutable_data_len: info.immutable_data_len,
		}
	}

	#[test]
	fn new_reserves_balance_works() {
		clear_ext();

		TestMeter::new(1_000);

		assert_eq!(TestExtTestValue::get(), TestExt { ..Default::default() })
	}

	/// Previously, passing a limit of 0 meant unlimited storage for a nested call.
	///
	/// Now, a limit of 0 means the subcall will not be able to use any storage.
	#[test]
	fn nested_zero_limit_requested() {
		clear_ext();

		let meter = TestMeter::new(1_000);
		assert_eq!(meter.available(), 1_000);
		let nested0 = meter.nested(BalanceOf::<Test>::zero());
		assert_eq!(nested0.available(), 0);
	}

	#[test]
	fn nested_some_limit_requested() {
		clear_ext();

		let meter = TestMeter::new(1_000);
		assert_eq!(meter.available(), 1_000);
		let nested0 = meter.nested(500);
		assert_eq!(nested0.available(), 500);
	}

	#[test]
	fn nested_all_limit_requested() {
		clear_ext();

		let meter = TestMeter::new(1_000);
		assert_eq!(meter.available(), 1_000);
		let nested0 = meter.nested(1_000);
		assert_eq!(nested0.available(), 1_000);
	}

	#[test]
	fn nested_over_limit_requested() {
		clear_ext();

		let meter = TestMeter::new(1_000);
		assert_eq!(meter.available(), 1_000);
		let nested0 = meter.nested(2_000);
		assert_eq!(nested0.available(), 1_000);
	}

	#[test]
	fn empty_charge_works() {
		clear_ext();

		let mut meter = TestMeter::new(1_000);
		assert_eq!(meter.available(), 1_000);

		// an empty charge does not create a `Charge` entry
		let mut nested0 = meter.nested(BalanceOf::<Test>::zero());
		nested0.charge(&Default::default());
		meter.absorb(nested0, &BOB, None);

		assert_eq!(TestExtTestValue::get(), TestExt { ..Default::default() })
	}

	#[test]
	fn charging_works() {
		let test_cases = vec![
			ChargingTestCase {
				origin: Origin::<Test>::from_account_id(ALICE),
				deposit: Deposit::Refund(28),
				expected: TestExt {
					charges: vec![
						Charge {
							origin: ALICE,
							contract: CHARLIE,
							amount: Deposit::Refund(10),
							state: ContractState::Alive,
						},
						Charge {
							origin: ALICE,
							contract: CHARLIE,
							amount: Deposit::Refund(20),
							state: ContractState::Alive,
						},
						Charge {
							origin: ALICE,
							contract: BOB,
							amount: Deposit::Charge(2),
							state: ContractState::Alive,
						},
					],
				},
			},
			ChargingTestCase {
				origin: Origin::<Test>::Root,
				deposit: Deposit::Charge(0),
				expected: TestExt { charges: vec![] },
			},
		];

		for test_case in test_cases {
			clear_ext();

			let mut meter = TestMeter::new(100);
			assert_eq!(meter.available(), 100);

			let mut nested0_info = new_info(StorageInfo {
				bytes: 100,
				items: 5,
				bytes_deposit: 100,
				items_deposit: 10,
				immutable_data_len: 0,
			});
			let mut nested0 = meter.nested(BalanceOf::<Test>::zero());
			nested0.charge(&Diff {
				bytes_added: 108,
				bytes_removed: 5,
				items_added: 1,
				items_removed: 2,
			});
			nested0.charge(&Diff { bytes_removed: 99, ..Default::default() });

			let mut nested1_info = new_info(StorageInfo {
				bytes: 100,
				items: 10,
				bytes_deposit: 100,
				items_deposit: 20,
				immutable_data_len: 0,
			});
			let mut nested1 = nested0.nested(BalanceOf::<Test>::zero());
			nested1.charge(&Diff { items_removed: 5, ..Default::default() });
			nested0.absorb(nested1, &CHARLIE, Some(&mut nested1_info));

			let mut nested2_info = new_info(StorageInfo {
				bytes: 100,
				items: 7,
				bytes_deposit: 100,
				items_deposit: 20,
				immutable_data_len: 0,
			});
			let mut nested2 = nested0.nested(BalanceOf::<Test>::zero());
			nested2.charge(&Diff { items_removed: 7, ..Default::default() });
			nested0.absorb(nested2, &CHARLIE, Some(&mut nested2_info));

			nested0.enforce_limit(Some(&mut nested0_info)).unwrap();
			meter.absorb(nested0, &BOB, Some(&mut nested0_info));

			assert_eq!(
				meter.try_into_deposit(&test_case.origin, false).unwrap(),
				test_case.deposit
			);

			assert_eq!(nested0_info.extra_deposit(), 112);
			assert_eq!(nested1_info.extra_deposit(), 110);
			assert_eq!(nested2_info.extra_deposit(), 100);

			assert_eq!(TestExtTestValue::get(), test_case.expected)
		}
	}

	#[test]
	fn termination_works() {
		let test_cases = vec![
			ChargingTestCase {
				origin: Origin::<Test>::from_account_id(ALICE),
				deposit: Deposit::Refund(108),
				expected: TestExt {
					charges: vec![
						Charge {
							origin: ALICE,
							contract: CHARLIE,
							amount: Deposit::Refund(120),
							state: ContractState::Terminated { beneficiary: CHARLIE },
						},
						Charge {
							origin: ALICE,
							contract: BOB,
							amount: Deposit::Charge(12),
							state: ContractState::Alive,
						},
					],
				},
			},
			ChargingTestCase {
				origin: Origin::<Test>::Root,
				deposit: Deposit::Charge(0),
				expected: TestExt { charges: vec![] },
			},
		];

		for test_case in test_cases {
			clear_ext();

			let mut meter = TestMeter::new(1_000);
			assert_eq!(meter.available(), 1_000);

			let mut nested0 = meter.nested(BalanceOf::<Test>::max_value());
			nested0.charge(&Diff {
				bytes_added: 5,
				bytes_removed: 1,
				items_added: 3,
				items_removed: 1,
			});
			nested0.charge(&Diff { items_added: 2, ..Default::default() });

			let mut nested1_info = new_info(StorageInfo {
				bytes: 100,
				items: 10,
				bytes_deposit: 100,
				items_deposit: 20,
				immutable_data_len: 0,
			});
			let mut nested1 = nested0.nested(BalanceOf::<Test>::max_value());
			nested1.charge(&Diff { items_removed: 5, ..Default::default() });
			nested1.charge(&Diff { bytes_added: 20, ..Default::default() });
			nested1.terminate(&nested1_info, CHARLIE);
			nested0.enforce_limit(Some(&mut nested1_info)).unwrap();
			nested0.absorb(nested1, &CHARLIE, None);

			meter.absorb(nested0, &BOB, None);
			assert_eq!(
				meter.try_into_deposit(&test_case.origin, false).unwrap(),
				test_case.deposit
			);
			assert_eq!(TestExtTestValue::get(), test_case.expected)
		}
	}
}
