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

//! Bandersnatch VRF application crypto types.

use crate::{KeyTypeId, RuntimePublic};
use alloc::vec::Vec;
pub use sp_core::bandersnatch::*;

use sp_core::{
	crypto::CryptoType,
	proof_of_possession::{NonAggregatable, ProofOfPossessionVerifier},
	Pair as TraitPair,
};

mod app {
	crate::app_crypto!(super, sp_core::testing::BANDERSNATCH);
}

#[cfg(feature = "full_crypto")]
pub use app::Pair as AppPair;
pub use app::{Public as AppPublic, Signature as AppSignature};

impl RuntimePublic for Public {
	type Signature = Signature;

	/// Dummy implementation. Returns an empty vector.
	fn all(_key_type: KeyTypeId) -> Vec<Self> {
		Vec::new()
	}

	fn generate_pair(key_type: KeyTypeId, seed: Option<Vec<u8>>) -> Self {
		sp_io::crypto::bandersnatch_generate(key_type, seed)
	}

	fn sign<M: AsRef<[u8]>>(&self, key_type: KeyTypeId, msg: &M) -> Option<Self::Signature> {
		sp_io::crypto::bandersnatch_sign(key_type, self, msg.as_ref())
	}

	fn verify<M: AsRef<[u8]>>(&self, msg: &M, signature: &Self::Signature) -> bool {
		let sig = AppSignature::from(*signature);
		let pub_key = AppPublic::from(*self);
		<AppPublic as CryptoType>::Pair::verify(&sig, msg.as_ref(), &pub_key)
	}

	fn generate_proof_of_possession(&mut self, key_type: KeyTypeId) -> Option<Self::Signature> {
		let proof_of_possession_statement = Pair::proof_of_possession_statement(self);
		sp_io::crypto::bandersnatch_sign(key_type, self, &proof_of_possession_statement)
	}

	fn verify_proof_of_possession(&self, proof_of_possession: &Self::Signature) -> bool {
		let proof_of_possession = AppSignature::from(*proof_of_possession);
		let pub_key = AppPublic::from(*self);
		<AppPublic as CryptoType>::Pair::verify_proof_of_possession(&proof_of_possession, &pub_key)
	}

	fn to_raw_vec(&self) -> Vec<u8> {
		sp_core::crypto::ByteArray::to_raw_vec(self)
	}
}
