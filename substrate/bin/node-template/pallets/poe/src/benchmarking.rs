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
	use frame_support::traits::Get;
	use frame_support::sp_runtime::BoundedVec;

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
	fn create_claim(p: Linear<1, { T::ProofSizeLimit::get() }>) {
		let input = (0..p).map(|a| a as u8).collect::<Vec<_>>();
		let caller: T::AccountId = whitelisted_caller();

		#[extrinsic_call]
		create_claim(RawOrigin::Signed(caller.clone()), input.clone());

		assert_last_event::<T>(Event::<T>::ClaimCreated(caller, input).into());
	}

	#[benchmark]
	fn revoke_claim(p: Linear<1, { T::ProofSizeLimit::get() }>) {
		let input = (0..p).map(|a| a as u8).collect::<Vec<_>>();
		let caller: T::AccountId = whitelisted_caller();
		let claim = BoundedVec::<u8, T::ProofSizeLimit>::try_from(input.clone())
		.map_err(|_| Error::<T>::ProofTooLarge).unwrap();
		Proofs::<T>::insert(&claim, (&caller, frame_system::Pallet::<T>::block_number()));

		#[extrinsic_call]
		revoke_claim(RawOrigin::Signed(caller.clone()), input.clone());

		assert_last_event::<T>(Event::<T>::ClaimRevoked(caller, input).into());
	}

	#[benchmark]
	fn transfer_claim(p: Linear<1, { T::ProofSizeLimit::get() }>) {
		let input = (0..p).map(|a| a as u8).collect::<Vec<_>>();
		let caller: T::AccountId = whitelisted_caller();
		let recver: T::AccountId = account("other", 0, 42);
		let claim = BoundedVec::<u8, T::ProofSizeLimit>::try_from(input.clone())
		.map_err(|_| Error::<T>::ProofTooLarge).unwrap();
		Proofs::<T>::insert(&claim, (&caller, frame_system::Pallet::<T>::block_number()));

		#[extrinsic_call]
		transfer_claim(RawOrigin::Signed(caller.clone()), input.clone(), recver.clone());

		assert_last_event::<T>(Event::<T>::ClaimTransferred(caller, input, recver).into());
	}

	impl_benchmark_test_suite!(PoeModule, crate::mock::new_test_ext(), crate::mock::Test);
}
