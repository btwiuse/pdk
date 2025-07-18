// Copyright (C) Parity Technologies (UK) Ltd.
// This file is part of Polkadot.

// Polkadot is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// Polkadot is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with Polkadot.  If not, see <http://www.gnu.org/licenses/>.

//! Runtime API module declares the `trait ParachainHost` which is part
//! of the Runtime API exposed from the Runtime to the Host.
//!
//! The functions in trait ParachainHost` can be part of the stable API
//! (which is versioned) or they can be staging (aka unstable/testing
//! functions).
//!
//! The separation outlined above is achieved with the versioned API feature
//! of `decl_runtime_apis!` and `impl_runtime_apis!`. Before moving on let's
//! see a quick example about how API versioning works.
//!
//! # Runtime API versioning crash course
//!
//! The versioning is achieved with the `api_version` attribute. It can be
//! placed on:
//! * trait declaration - represents the base version of the API.
//! * method declaration (inside a trait declaration) - represents a versioned method, which is not
//!   available in the base version.
//! * trait implementation - represents which version of the API is being implemented.
//!
//! Let's see a quick example:
//!
//! ```nocompile
//! sp_api::decl_runtime_apis! {
//! 	#[api_version(2)]
//! 	pub trait MyApi {
//! 		fn fn1();
//! 		fn fn2();
//! 		#[api_version(3)]
//! 		fn fn3();
//! 		#[api_version(4)]
//! 		fn fn4();
//! 	}
//! }
//!
//! struct Runtime {}
//!
//! sp_api::impl_runtime_apis! {
//!     #[api_version(3)]
//!     impl self::MyApi<Block> for Runtime {
//!         fn fn1() {}
//!         fn fn2() {}
//!         fn fn3() {}
//!     }
//! }
//! ```
//! A new API named `MyApi` is declared with `decl_runtime_apis!`. The trait declaration
//! has got an `api_version` attribute which represents its base version - 2 in this case.
//!
//! The API has got three methods - `fn1`, `fn2`, `fn3` and `fn4`. `fn3` and `fn4` has got
//! an `api_version` attribute which makes them versioned methods. These methods do not exist
//! in the base version of the API. Behind the scenes the declaration above creates three
//! runtime APIs:
//! * `MyApiV2` with `fn1` and `fn2`
//! * `MyApiV3` with `fn1`, `fn2` and `fn3`.
//! * `MyApiV4` with `fn1`, `fn2`, `fn3` and `fn4`.
//!
//! Please note that `v4` contains all methods from `v3`, `v3` all methods from `v2` and so on.
//!
//! Back to our example. At the end runtime API is implemented for `struct Runtime` with
//! `impl_runtime_apis` macro. `api_version` attribute is attached to the `impl` block which
//! means that a version different from the base one is being implemented - in our case this
//! is `v3`.
//!
//! This version of the API contains three methods so the `impl` block has got definitions
//! for them. Note that `fn4` is not implemented as it is not part of this version of the API.
//! `impl_runtime_apis` generates a default implementation for it calling `unimplemented!()`.
//!
//! Hopefully this should be all you need to know in order to use versioned methods in the node.
//! For more details about how the API versioning works refer to `spi_api`
//! documentation [here](https://docs.substrate.io/rustdocs/latest/sp_api/macro.decl_runtime_apis.html).
//!
//! # How versioned methods are used for `ParachainHost`
//!
//! Let's introduce two types of `ParachainHost` API implementation:
//! * stable - used on stable production networks like Polkadot and Kusama. There is only one stable
//!   API at a single point in time.
//! * staging - methods that are ready for production, but will be released on Rococo first. We can
//!   batch together multiple changes and then release all of them to production, by making staging
//!   production (bump base version). We can not change or remove any method in staging after a
//!   release, as this would break Rococo. It should be ok to keep adding methods to staging across
//!   several releases. For experimental methods, you have to keep them on a separate branch until
//!   ready.
//!
//! The stable version of `ParachainHost` is indicated by the base version of the API. Any staging
//! method must use `api_version` attribute so that it is assigned to a specific version of a
//! staging API. This way in a single declaration one can see what's the stable version of
//! `ParachainHost` and what staging versions/functions are available.
//!
//! All stable API functions should use primitives from the latest version.
//! In the time of writing of this document - this is `v2`. So for example:
//! ```ignore
//! fn validators() -> Vec<v2::ValidatorId>;
//! ```
//! indicates a function from the stable `v2` API.
//!
//! All staging API functions should use primitives from `vstaging`. They should be clearly
//! separated from the stable primitives.

use crate::{
	slashing,
	vstaging::{
		self, async_backing::Constraints, CandidateEvent,
		CommittedCandidateReceiptV2 as CommittedCandidateReceipt, CoreState, ScrapedOnChainVotes,
	},
	ApprovalVotingParams, AsyncBackingParams, BlockNumber, CandidateCommitments, CandidateHash,
	CoreIndex, DisputeState, ExecutorParams, GroupRotationInfo, Hash, NodeFeatures,
	OccupiedCoreAssumption, PersistedValidationData, PvfCheckStatement, SessionIndex, SessionInfo,
	ValidatorId, ValidatorIndex, ValidatorSignature,
};

use alloc::{
	collections::{btree_map::BTreeMap, vec_deque::VecDeque},
	vec::Vec,
};
use polkadot_core_primitives as pcp;
use polkadot_parachain_primitives::primitives as ppp;

sp_api::decl_runtime_apis! {
	/// The API for querying the state of parachains on-chain.
	#[api_version(5)]
	pub trait ParachainHost {
		/// Get the current validators.
		fn validators() -> Vec<ValidatorId>;

		/// Returns the validator groups and rotation info localized based on the hypothetical child
		///  of a block whose state  this is invoked on. Note that `now` in the `GroupRotationInfo`
		/// should be the successor of the number of the block.
		fn validator_groups() -> (Vec<Vec<ValidatorIndex>>, GroupRotationInfo<BlockNumber>);

		/// Yields information on all availability cores as relevant to the child block.
		/// Cores are either free or occupied. Free cores can have paras assigned to them.
		fn availability_cores() -> Vec<CoreState<Hash, BlockNumber>>;

		/// Yields the persisted validation data for the given `ParaId` along with an assumption that
		/// should be used if the para currently occupies a core.
		///
		/// Returns `None` if either the para is not registered or the assumption is `Freed`
		/// and the para already occupies a core.
		fn persisted_validation_data(para_id: ppp::Id, assumption: OccupiedCoreAssumption)
			-> Option<PersistedValidationData<Hash, BlockNumber>>;

		/// Returns the persisted validation data for the given `ParaId` along with the corresponding
		/// validation code hash. Instead of accepting assumption about the para, matches the validation
		/// data hash against an expected one and yields `None` if they're not equal.
		fn assumed_validation_data(
			para_id: ppp::Id,
			expected_persisted_validation_data_hash: Hash,
		) -> Option<(PersistedValidationData<Hash, BlockNumber>, ppp::ValidationCodeHash)>;

		/// Checks if the given validation outputs pass the acceptance criteria.
		fn check_validation_outputs(para_id: ppp::Id, outputs: CandidateCommitments) -> bool;

		/// Returns the session index expected at a child of the block.
		///
		/// This can be used to instantiate a `SigningContext`.
		fn session_index_for_child() -> SessionIndex;

		/// Fetch the validation code used by a para, making the given `OccupiedCoreAssumption`.
		///
		/// Returns `None` if either the para is not registered or the assumption is `Freed`
		/// and the para already occupies a core.
		fn validation_code(
			para_id: ppp::Id,
			assumption: OccupiedCoreAssumption,
		) -> Option<ppp::ValidationCode>;

		/// Get the receipt of a candidate pending availability. This returns `Some` for any paras
		/// assigned to occupied cores in `availability_cores` and `None` otherwise.
		fn candidate_pending_availability(para_id: ppp::Id) -> Option<CommittedCandidateReceipt<Hash>>;

		/// Get a vector of events concerning candidates that occurred within a block.
		fn candidate_events() -> Vec<CandidateEvent<Hash>>;

		/// Get all the pending inbound messages in the downward message queue for a para.
		fn dmq_contents(
			recipient: ppp::Id,
		) -> Vec<pcp::v2::InboundDownwardMessage<BlockNumber>>;

		/// Get the contents of all channels addressed to the given recipient. Channels that have no
		/// messages in them are also included.
		fn inbound_hrmp_channels_contents(
			recipient: ppp::Id,
		) -> BTreeMap<ppp::Id, Vec<pcp::v2::InboundHrmpMessage<BlockNumber>>>;

		/// Get the validation code from its hash.
		fn validation_code_by_hash(hash: ppp::ValidationCodeHash) -> Option<ppp::ValidationCode>;

		/// Scrape dispute relevant from on-chain, backing votes and resolved disputes.
		fn on_chain_votes() -> Option<ScrapedOnChainVotes<Hash>>;

		/***** Added in v2 *****/

		/// Get the session info for the given session, if stored.
		///
		/// NOTE: This function is only available since parachain host version 2.
		fn session_info(index: SessionIndex) -> Option<SessionInfo>;

		/// Submits a PVF pre-checking statement into the transaction pool.
		///
		/// NOTE: This function is only available since parachain host version 2.
		fn submit_pvf_check_statement(stmt: PvfCheckStatement, signature: ValidatorSignature);

		/// Returns code hashes of PVFs that require pre-checking by validators in the active set.
		///
		/// NOTE: This function is only available since parachain host version 2.
		fn pvfs_require_precheck() -> Vec<ppp::ValidationCodeHash>;

		/// Fetch the hash of the validation code used by a para, making the given `OccupiedCoreAssumption`.
		///
		/// NOTE: This function is only available since parachain host version 2.
		fn validation_code_hash(para_id: ppp::Id, assumption: OccupiedCoreAssumption)
			-> Option<ppp::ValidationCodeHash>;

		/// Returns all onchain disputes.
		fn disputes() -> Vec<(SessionIndex, CandidateHash, DisputeState<BlockNumber>)>;

		/// Returns execution parameters for the session.
		fn session_executor_params(session_index: SessionIndex) -> Option<ExecutorParams>;

		/// Returns a list of validators that lost a past session dispute and need to be slashed.
		/// NOTE: This function is only available since parachain host version 5.
		fn unapplied_slashes() -> Vec<(SessionIndex, CandidateHash, slashing::PendingSlashes)>;

		/// Returns a merkle proof of a validator session key.
		/// NOTE: This function is only available since parachain host version 5.
		fn key_ownership_proof(
			validator_id: ValidatorId,
		) -> Option<slashing::OpaqueKeyOwnershipProof>;

		/// Submit an unsigned extrinsic to slash validators who lost a dispute about
		/// a candidate of a past session.
		/// NOTE: This function is only available since parachain host version 5.
		fn submit_report_dispute_lost(
			dispute_proof: slashing::DisputeProof,
			key_ownership_proof: slashing::OpaqueKeyOwnershipProof,
		) -> Option<()>;

		/***** Added in v6 *****/

		/// Get the minimum number of backing votes for a parachain candidate.
		/// This is a staging method! Do not use on production runtimes!
		#[api_version(6)]
		fn minimum_backing_votes() -> u32;


		/***** Added in v7: Asynchronous backing *****/

		/// Returns the state of parachain backing for a given para.
		#[api_version(7)]
		fn para_backing_state(_: ppp::Id) -> Option<vstaging::async_backing::BackingState<Hash, BlockNumber>>;

		/// Returns candidate's acceptance limitations for asynchronous backing for a relay parent.
		#[api_version(7)]
		fn async_backing_params() -> AsyncBackingParams;

		/***** Added in v8 *****/

		/// Returns a list of all disabled validators at the given block.
		#[api_version(8)]
		fn disabled_validators() -> Vec<ValidatorIndex>;

		/***** Added in v9 *****/

		/// Get node features.
		/// This is a staging method! Do not use on production runtimes!
		#[api_version(9)]
		fn node_features() -> NodeFeatures;

		/***** Added in v10 *****/
		/// Approval voting configuration parameters
		#[api_version(10)]
		fn approval_voting_params() -> ApprovalVotingParams;

		/***** Added in v11 *****/
		/// Claim queue
		#[api_version(11)]
		fn claim_queue() -> BTreeMap<CoreIndex, VecDeque<ppp::Id>>;

		/***** Added in v11 *****/
		/// Elastic scaling support
		#[api_version(11)]
		fn candidates_pending_availability(para_id: ppp::Id) -> Vec<CommittedCandidateReceipt<Hash>>;

		/***** Added in v12 *****/
		/// Retrieve the maximum uncompressed code size.
		#[api_version(12)]
		fn validation_code_bomb_limit() -> u32;

		/***** Added in v13 *****/
		/// Returns the constraints on the actions that can be taken by a new parachain
		/// block.
		#[api_version(13)]
		fn backing_constraints(para_id: ppp::Id) -> Option<Constraints>;

		/***** Added in v13 *****/
		/// Retrieve the scheduling lookahead
		#[api_version(13)]
		fn scheduling_lookahead() -> u32;

		/***** Added in v14 *****/
		/// Retrieve paraids at relay parent
		#[api_version(14)]
		fn para_ids() -> Vec<ppp::Id>;

	}
}
