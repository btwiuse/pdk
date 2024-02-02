//! Benchmarking setup for pallet-template
#![cfg(feature = "runtime-benchmarks")]
use super::*;

#[allow(unused)]
use crate::Pallet as PoeModule;
use frame_benchmarking::v2::*;
use frame_system::RawOrigin;
use frame_system::EventRecord;

fn assert_last_event<T: Config>(generic_event: <T as Config>::RuntimeEvent) {
	let events = frame_system::Pallet::<T>::events();
	let system_event: <T as frame_system::Config>::RuntimeEvent = generic_event.into();
	// compare to the last event record
	let EventRecord { event, .. } = &events[events.len() - 1];
	assert_eq!(event, &system_event);
}

#[benchmarks]
mod benchmarks {
	use super::*;
	use sp_std::vec;

	#[benchmark]
	fn do_something() {
		let value = 100u32.into();
		let caller: T::AccountId = whitelisted_caller();
		#[extrinsic_call]
		do_something(RawOrigin::Signed(caller), value);

		assert_eq!(Something::<T>::get(), Some(value));
	}

	#[benchmark]
	fn cause_error() {
		Something::<T>::put(100u32);
		let caller: T::AccountId = whitelisted_caller();
		#[extrinsic_call]
		cause_error(RawOrigin::Signed(caller));

		assert_eq!(Something::<T>::get(), Some(101u32));
	}

	#[benchmark]
	fn create_claim() {
		let input = vec![100u8];
		let caller: T::AccountId = whitelisted_caller();
		#[extrinsic_call]
		create_claim(RawOrigin::Signed(caller.clone()), input.clone());

		assert_last_event::<T>(Event::<T>::ClaimCreated(caller, input).into());
	}
	#[benchmark]
	fn revoke_claim() {
		let input = vec![100u8];
		let caller: T::AccountId = whitelisted_caller();
		#[extrinsic_call]
		create_claim(RawOrigin::Signed(caller.clone()), input.clone());

		assert_last_event::<T>(Event::<T>::ClaimCreated(caller, input).into());
	}
	#[benchmark]
	fn transfer_claim() {
		let input = vec![100u8];
		let caller: T::AccountId = whitelisted_caller();
		#[extrinsic_call]
		create_claim(RawOrigin::Signed(caller.clone()), input.clone());

		assert_last_event::<T>(Event::<T>::ClaimCreated(caller, input).into());
	}
	impl_benchmark_test_suite!(PoeModule, crate::mock::new_test_ext(), crate::mock::Test);
}
