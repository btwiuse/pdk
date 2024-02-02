use crate::{mock::*, Error, Event, *};
use frame_support::{assert_noop, assert_ok};

const ACCOUNT_BALANCE: u128 = 100000;
const PALLET_BALANCE: u128 = 0;
const CAT_NAME: [u8; 8] = *b"test0000";

#[test]
fn test_create() {
	new_test_ext::<Test>().execute_with(|| {
		let cat_id = 0;
		let account_id = 1;

		assert_ok!(Balances::force_set_balance(RuntimeOrigin::root(), account_id, ACCOUNT_BALANCE));

		assert_eq!(CatModule::next_cat_id(), cat_id);
		assert_ok!(CatModule::create_cat(RuntimeOrigin::signed(account_id)));

		assert_eq!(CatModule::next_cat_id(), cat_id + 1);
		assert_eq!(CatModule::cats(cat_id).is_some(), true);
		assert_eq!(CatModule::cat_owner(cat_id), Some(account_id));
		assert_eq!(CatModule::cat_parents(cat_id), None);

		crate::NextCatId::<Test>::set(<Test as pallet::Config>::CatId::max_value());
		assert_noop!(
			CatModule::create_cat(RuntimeOrigin::signed(account_id)),
			Error::<Test>::InvalidCatId
		);

		let cat = CatModule::cats(cat_id).expect("there should be a cat");
		System::assert_last_event(Event::CatCreated(account_id, cat_id, cat).into());
	})
}

#[test]
fn test_breed() {
	new_test_ext::<Test>().execute_with(|| {
		let account_id = 1;
		let cat_id = 0;

		assert_ok!(Balances::force_set_balance(RuntimeOrigin::root(), account_id, ACCOUNT_BALANCE));

		assert_noop!(
			CatModule::breed_cats(RuntimeOrigin::signed(account_id), cat_id, cat_id),
			Error::<Test>::SameCatId
		);

		assert_noop!(
			CatModule::breed_cats(RuntimeOrigin::signed(account_id), cat_id, cat_id + 1),
			Error::<Test>::InvalidCatId
		);

		assert_ok!(CatModule::create_cat(RuntimeOrigin::signed(account_id)));
		assert_ok!(CatModule::create_cat(RuntimeOrigin::signed(account_id)));

		let child_id = cat_id + 2;
		assert_eq!(CatModule::next_cat_id(), child_id);
		assert_ok!(CatModule::breed_cats(RuntimeOrigin::signed(account_id), cat_id, cat_id + 1));
		assert_eq!(CatModule::next_cat_id(), child_id + 1);

		assert_eq!(CatModule::cats(child_id).is_some(), true);
		assert_eq!(CatModule::cat_owner(child_id), Some(account_id));
		assert_eq!(CatModule::cat_parents(child_id), Some((cat_id, cat_id + 1)));

		let cat = CatModule::cats(child_id).expect("there should be a cat");
		System::assert_last_event(Event::CatBred(account_id, child_id, cat).into());
	})
}

#[test]
fn test_transfer() {
	new_test_ext::<Test>().execute_with(|| {
		let account_id = 1;
		let cat_id = 0;
		let recipient = 2;

		assert_ok!(Balances::force_set_balance(RuntimeOrigin::root(), account_id, ACCOUNT_BALANCE));
		assert_ok!(Balances::force_set_balance(RuntimeOrigin::root(), recipient, ACCOUNT_BALANCE));

		assert_ok!(CatModule::create_cat(RuntimeOrigin::signed(account_id)));
		assert_eq!(CatModule::cat_owner(cat_id), Some(account_id));

		assert_noop!(
			CatModule::transfer_cat(RuntimeOrigin::signed(recipient), recipient, cat_id),
			Error::<Test>::NotCatOwner
		);

		assert_ok!(CatModule::transfer_cat(RuntimeOrigin::signed(account_id), recipient, cat_id));
		assert_eq!(CatModule::cat_owner(cat_id), Some(recipient));
		System::assert_last_event(Event::CatTransferred(account_id, recipient, cat_id).into());

		assert_ok!(CatModule::transfer_cat(RuntimeOrigin::signed(recipient), account_id, cat_id));
		assert_eq!(CatModule::cat_owner(cat_id), Some(account_id));
		System::assert_last_event(Event::CatTransferred(recipient, account_id, cat_id).into());
	})
}

#[test]
fn it_works_for_list() {
	new_test_ext::<Test>().execute_with(|| {
		use sp_runtime::traits::AccountIdConversion;
		use frame_support::PalletId;
		let CatPalletAccount: u64 = PalletId(*b"py/meoww").into_account_truncating();

		let account_id = 1;
		let account_id2 = 2;
		let cat_id = 0;
		
		assert_ok!(Balances::force_set_balance(RuntimeOrigin::root(), account_id, ACCOUNT_BALANCE));
		assert_ok!(Balances::force_set_balance(RuntimeOrigin::root(), account_id2, ACCOUNT_BALANCE));

		// 当不存在 kitty 时失败
		assert_noop!(
			CatModule::list(RuntimeOrigin::signed(account_id), cat_id),
			Error::<Test>::InvalidCatId
		);

		assert_ok!(CatModule::create_cat(RuntimeOrigin::signed(account_id) /*, CAT_NAME */));
		assert_eq!(Balances::free_balance(account_id), ACCOUNT_BALANCE - EXISTENTIAL_DEPOSIT * 10);
		assert_eq!(
			Balances::free_balance(&CatPalletAccount),
			PALLET_BALANCE + EXISTENTIAL_DEPOSIT * 10
		);
		// 当所有者不正确时失败
		assert_noop!(
			CatModule::list(RuntimeOrigin::signed(account_id2), cat_id),
			Error::<Test>::NotCatOwner
		);

		// 所有者正确，成功
		assert_ok!(CatModule::list(RuntimeOrigin::signed(account_id), cat_id));
		assert!(CatModule::cat_listing(cat_id).is_some());
		System::assert_last_event(Event::CatListed ( account_id, 0 ).into());

		// 重复 sale, 失败
		assert_noop!(
			CatModule::list(RuntimeOrigin::signed(account_id), cat_id),
			Error::<Test>::AlreadyListed
		);
	});
}

#[test]
fn it_works_for_buy() {
	new_test_ext::<Test>().execute_with(|| {

		let account_id = 1;
		let account_id2 = 2;
		let cat_id = 0;
		assert_ok!(Balances::force_set_balance(RuntimeOrigin::root(), account_id, ACCOUNT_BALANCE));
		assert_ok!(Balances::force_set_balance(RuntimeOrigin::root(), account_id2, ACCOUNT_BALANCE));

		// 当不存在 kitty 时失败
		assert_noop!(
			CatModule::buy(RuntimeOrigin::signed(account_id), cat_id),
			Error::<Test>::InvalidCatId
		);

		assert_ok!(CatModule::create_cat(RuntimeOrigin::signed(account_id)/* , CAT_NAME */ ));
		assert_eq!(Balances::free_balance(account_id), ACCOUNT_BALANCE - EXISTENTIAL_DEPOSIT * 10);

		// 当购买者与所有者相同时失败
		assert_noop!(
			CatModule::buy(RuntimeOrigin::signed(account_id), cat_id),
			Error::<Test>::AlreadyOwned
		);

		// 当没有上架时，失败
		assert_noop!(
			CatModule::buy(RuntimeOrigin::signed(account_id2), cat_id),
			Error::<Test>::NotListed
		);

		// 上述失败条件不存在时，成功
		assert_ok!(CatModule::list(RuntimeOrigin::signed(account_id), cat_id));
		assert_ok!(CatModule::buy(RuntimeOrigin::signed(account_id2), cat_id));
		assert!(CatModule::cat_listing(cat_id).is_none());
		assert_eq!(CatModule::cat_owner(cat_id), Some(account_id2));
		assert_eq!(Balances::free_balance(account_id), ACCOUNT_BALANCE);
		assert_eq!(Balances::free_balance(account_id2), ACCOUNT_BALANCE - EXISTENTIAL_DEPOSIT * 10);
		System::assert_last_event(Event::CatBought (account_id2, 0 ).into());
	});
}
