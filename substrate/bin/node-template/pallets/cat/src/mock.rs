use crate as pallet_cat;
use pallet_insecure_randomness_collective_flip;
use frame_support::traits::{ConstU16, ConstU64};
use frame_support::parameter_types;
use sp_core::H256;
use sp_runtime::{
	traits::{BlakeTwo256, IdentityLookup},
	BuildStorage,
};

type Block = frame_system::mocking::MockBlock<Test>;

// Configure a mock runtime to test the pallet.
frame_support::construct_runtime!(
	pub enum Test
	{
		System: frame_system,
		Randomness: pallet_insecure_randomness_collective_flip,
		CatModule: pallet_cat,
	}
);

impl frame_system::Config for Test {
	type RuntimeTask = RuntimeTask;
	type BaseCallFilter = frame_support::traits::Everything;
	type BlockWeights = ();
	type BlockLength = ();
	type DbWeight = ();
	type RuntimeOrigin = RuntimeOrigin;
	type RuntimeCall = RuntimeCall;
	type Nonce = u64;
	type Hash = H256;
	type Hashing = BlakeTwo256;
	type AccountId = u64;
	type Lookup = IdentityLookup<Self::AccountId>;
	type Block = Block;
	type RuntimeEvent = RuntimeEvent;
	type BlockHashCount = ConstU64<250>;
	type Version = ();
	type PalletInfo = PalletInfo;
	type AccountData = ();
	type OnNewAccount = ();
	type OnKilledAccount = ();
	type SystemWeightInfo = ();
	type SS58Prefix = ConstU16<42>;
	type OnSetCode = ();
	type MaxConsumers = frame_support::traits::ConstU32<16>;
}

impl pallet_insecure_randomness_collective_flip::Config for Test {}

impl pallet_cat::Config for Test {
	type RuntimeEvent = RuntimeEvent;
	type Randomness = Randomness;
	type CatId = u32;
}

// Build genesis storage according to the mock runtime.
pub fn new_test_ext<T: pallet_cat::Config>() -> sp_io::TestExternalities {
	let mut ext: sp_io::TestExternalities =
		frame_system::GenesisConfig::<T>::default().build_storage().unwrap().into();
	ext.execute_with(|| System::set_block_number(1));
	ext
}