use crate::{*, mock::*, Error, Event};
use frame_support::{assert_noop, assert_ok};

#[test]
fn test_create() {
    new_test_ext::<Test>().execute_with(|| {
        let cat_id = 0;
        let account_id = 1;

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
        assert_ok!(
            CatModule::breed_cats(RuntimeOrigin::signed(account_id), cat_id, cat_id + 1)
        );
        assert_eq!(CatModule::next_cat_id(), child_id + 1);

        assert_eq!(CatModule::cats(child_id).is_some(), true);
        assert_eq!(CatModule::cat_owner(child_id), Some(account_id));
        assert_eq!(
            CatModule::cat_parents(child_id),
            Some((cat_id, cat_id + 1))
        );

        let cat = CatModule::cats(child_id).expect("there should be a cat");
        System::assert_last_event(
            Event::CatBred(account_id, child_id, cat).into(),
        );
    })
}

#[test]
fn test_transfer() {
    new_test_ext::<Test>().execute_with(|| {
        let account_id = 1;
        let cat_id = 0;
        let recipient = 2;

        assert_ok!(CatModule::create_cat(RuntimeOrigin::signed(account_id)));
        assert_eq!(CatModule::cat_owner(cat_id), Some(account_id));

        assert_noop!(
            CatModule::transfer_cat(RuntimeOrigin::signed(recipient), recipient, cat_id),
            Error::<Test>::NotCatOwner
        );

        assert_ok!(
            CatModule::transfer_cat(RuntimeOrigin::signed(account_id), recipient, cat_id)
        );
        assert_eq!(CatModule::cat_owner(cat_id), Some(recipient));
        System::assert_last_event(
            Event::CatTransferred(account_id, recipient, cat_id).into(),
        );

        assert_ok!(
            CatModule::transfer_cat(RuntimeOrigin::signed(recipient), account_id, cat_id)
        );
        assert_eq!(CatModule::cat_owner(cat_id), Some(account_id));
        System::assert_last_event(
            Event::CatTransferred(recipient, account_id, cat_id).into(),
        );
    })
}
