use super::*;
use frame_support::BoundedVec;
use crate::{mock::*, Error, Event};
use frame_support::{assert_noop, assert_ok};

#[test]
fn it_works_for_default_value() {
	new_test_ext().execute_with(|| {
		// Go past genesis block so events get deposited
		System::set_block_number(1);
		// Dispatch a signed extrinsic.
		assert_ok!(CatModule::do_something(RuntimeOrigin::signed(1), 42));
		// Read pallet storage and assert an expected result.
		assert_eq!(CatModule::something(), Some(42));
		// Assert that the correct event was deposited
		System::assert_last_event(Event::SomethingStored { something: 42, who: 1 }.into());
	});
}

#[test]
fn correct_error_for_none_value() {
	new_test_ext().execute_with(|| {
		// Ensure the expected error is thrown when no value is present.
		assert_noop!(
			CatModule::cause_error(RuntimeOrigin::signed(1)),
			Error::<Test>::NoneValue
		);
	});
}

#[test]
fn create_claim_works() {
	new_test_ext().execute_with(|| {
		let input: Vec<u8> = vec![0, 1];
		assert_ok!(CatModule::create_claim(RuntimeOrigin::signed(1), input.clone()));
		let bounded_input =
			BoundedVec::<u8, <Test as Config>::ProofSizeLimit>::try_from(input.clone()).unwrap();
		assert_eq!(
			CatModule::proofs(&bounded_input),
			Some((1, frame_system::Pallet::<Test>::block_number()))
		);
	});
}

#[test]
fn create_existing_claim_fails() {
	new_test_ext().execute_with(|| {
		let input: Vec<u8> = vec![0, 1];
		let _ = CatModule::create_claim(RuntimeOrigin::signed(1), input.clone());
		assert_noop!(
			CatModule::create_claim(RuntimeOrigin::signed(1), input.clone()),
			Error::<Test>::ProofAlreadyClaimed
		);
	});
}

#[test]
fn create_large_claim_fails() {
	new_test_ext().execute_with(|| {
		let input: Vec<u8> = vec![
			0;
			TryInto::<usize>::try_into(<Test as Config>::ProofSizeLimit::get()).unwrap() +
				1
		];
		let _ = CatModule::create_claim(RuntimeOrigin::signed(1), input.clone());
		assert_noop!(
			CatModule::create_claim(RuntimeOrigin::signed(1), input.clone()),
			Error::<Test>::ProofTooLarge
		);
	});
}

#[test]
fn revoke_claim_works() {
	new_test_ext().execute_with(|| {
		let claim: Vec<u8> = vec![0, 1];
		let _ = CatModule::create_claim(RuntimeOrigin::signed(1), claim.clone());
		assert_ok!(CatModule::revoke_claim(RuntimeOrigin::signed(1), claim.clone()));
	});
}

#[test]
fn revoke_non_existent_claim_fails() {
	new_test_ext().execute_with(|| {
		let claim: Vec<u8> = vec![0, 1];
		assert_noop!(
			CatModule::revoke_claim(RuntimeOrigin::signed(1), claim.clone()),
			Error::<Test>::NoSuchProof
		);
	});
}

#[test]
fn revoke_non_claim_owner_fails() {
	new_test_ext().execute_with(|| {
		let claim: Vec<u8> = vec![0, 1];
		let _ = CatModule::create_claim(RuntimeOrigin::signed(1), claim.clone());
		assert_noop!(
			CatModule::revoke_claim(RuntimeOrigin::signed(2), claim.clone()),
			Error::<Test>::NotProofOwner
		);
	});
}

#[test]
fn transfer_claim_works() {
	new_test_ext().execute_with(|| {
		let input: Vec<u8> = vec![0, 1];
		let _ = CatModule::create_claim(RuntimeOrigin::signed(1), input.clone());
		assert_ok!(CatModule::transfer_claim(RuntimeOrigin::signed(1), input.clone(), 42));
		let bounded_input =
			BoundedVec::<u8, <Test as Config>::ProofSizeLimit>::try_from(input.clone()).unwrap();
		assert_eq!(
			CatModule::proofs(&bounded_input),
			Some((42, frame_system::Pallet::<Test>::block_number()))
		);
		assert_noop!(
			CatModule::revoke_claim(RuntimeOrigin::signed(1), input.clone()),
			Error::<Test>::NotProofOwner
		);
	});
}

#[test]
fn transfer_non_owned_claim_fails() {
	new_test_ext().execute_with(|| {
		let claim: Vec<u8> = vec![0, 1];
		let _ = CatModule::create_claim(RuntimeOrigin::signed(1), claim.clone());
		assert_noop!(
			CatModule::transfer_claim(RuntimeOrigin::signed(2), claim.clone(), 42),
			Error::<Test>::NotProofOwner
		);
	});
}


#[test]
fn transfer_non_existent_claim_fails() {
	new_test_ext().execute_with(|| {
		let claim: Vec<u8> = vec![0, 1];
		assert_noop!(
			CatModule::transfer_claim(RuntimeOrigin::signed(1), claim.clone(), 42),
			Error::<Test>::NoSuchProof
		);
	});
}
