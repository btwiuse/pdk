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

//! Implements the Runtime API Subsystem
//!
//! This provides a clean, ownerless wrapper around the parachain-related runtime APIs. This crate
//! can also be used to cache responses from heavy runtime APIs.

#![deny(unused_crate_dependencies)]
#![warn(missing_docs)]

use polkadot_node_subsystem::{
	errors::RuntimeApiError,
	messages::{RuntimeApiMessage, RuntimeApiRequest as Request},
	overseer, FromOrchestra, OverseerSignal, SpawnedSubsystem, SubsystemError, SubsystemResult,
};
use polkadot_node_subsystem_types::RuntimeApiSubsystemClient;
use polkadot_primitives::Hash;

use cache::{RequestResult, RequestResultCache};
use futures::{channel::oneshot, prelude::*, select, stream::FuturesUnordered};
use std::sync::Arc;

mod cache;

mod metrics;
use self::metrics::Metrics;

#[cfg(test)]
mod tests;

const LOG_TARGET: &str = "parachain::runtime-api";

/// The number of maximum runtime API requests can be executed in parallel.
/// Further requests will backpressure the bounded channel.
const MAX_PARALLEL_REQUESTS: usize = 4;

/// The name of the blocking task that executes a runtime API request.
const API_REQUEST_TASK_NAME: &str = "polkadot-runtime-api-request";

/// The `RuntimeApiSubsystem`. See module docs for more details.
pub struct RuntimeApiSubsystem<Client> {
	client: Arc<Client>,
	metrics: Metrics,
	spawn_handle: Box<dyn overseer::gen::Spawner>,
	/// All the active runtime API requests that are currently being executed.
	active_requests: FuturesUnordered<oneshot::Receiver<Option<RequestResult>>>,
	/// Requests results cache
	requests_cache: RequestResultCache,
}

impl<Client> RuntimeApiSubsystem<Client> {
	/// Create a new Runtime API subsystem wrapping the given client and metrics.
	pub fn new(
		client: Arc<Client>,
		metrics: Metrics,
		spawner: impl overseer::gen::Spawner + 'static,
	) -> Self {
		RuntimeApiSubsystem {
			client,
			metrics,
			spawn_handle: Box::new(spawner),
			active_requests: Default::default(),
			requests_cache: RequestResultCache::default(),
		}
	}
}

#[overseer::subsystem(RuntimeApi, error = SubsystemError, prefix = self::overseer)]
impl<Client, Context> RuntimeApiSubsystem<Client>
where
	Client: RuntimeApiSubsystemClient + Send + Sync + 'static,
{
	fn start(self, ctx: Context) -> SpawnedSubsystem {
		SpawnedSubsystem { future: run(ctx, self).boxed(), name: "runtime-api-subsystem" }
	}
}

impl<Client> RuntimeApiSubsystem<Client>
where
	Client: RuntimeApiSubsystemClient + Send + 'static + Sync,
{
	fn store_cache(&mut self, result: RequestResult) {
		use RequestResult::*;

		match result {
			Authorities(relay_parent, authorities) =>
				self.requests_cache.cache_authorities(relay_parent, authorities),
			Validators(relay_parent, validators) =>
				self.requests_cache.cache_validators(relay_parent, validators),
			MinimumBackingVotes(session_index, minimum_backing_votes) => self
				.requests_cache
				.cache_minimum_backing_votes(session_index, minimum_backing_votes),
			ValidatorGroups(relay_parent, groups) =>
				self.requests_cache.cache_validator_groups(relay_parent, groups),
			AvailabilityCores(relay_parent, cores) =>
				self.requests_cache.cache_availability_cores(relay_parent, cores),
			PersistedValidationData(relay_parent, para_id, assumption, data) => self
				.requests_cache
				.cache_persisted_validation_data((relay_parent, para_id, assumption), data),
			AssumedValidationData(
				_relay_parent,
				para_id,
				expected_persisted_validation_data_hash,
				data,
			) => self.requests_cache.cache_assumed_validation_data(
				(para_id, expected_persisted_validation_data_hash),
				data,
			),
			CheckValidationOutputs(relay_parent, para_id, commitments, b) => self
				.requests_cache
				.cache_check_validation_outputs((relay_parent, para_id, commitments), b),
			SessionIndexForChild(relay_parent, session_index) =>
				self.requests_cache.cache_session_index_for_child(relay_parent, session_index),
			ValidationCode(relay_parent, para_id, assumption, code) => self
				.requests_cache
				.cache_validation_code((relay_parent, para_id, assumption), code),
			ValidationCodeByHash(_relay_parent, validation_code_hash, code) =>
				self.requests_cache.cache_validation_code_by_hash(validation_code_hash, code),
			CandidatePendingAvailability(relay_parent, para_id, candidate) => self
				.requests_cache
				.cache_candidate_pending_availability((relay_parent, para_id), candidate),
			CandidatesPendingAvailability(relay_parent, para_id, candidates) => self
				.requests_cache
				.cache_candidates_pending_availability((relay_parent, para_id), candidates),
			CandidateEvents(relay_parent, events) =>
				self.requests_cache.cache_candidate_events(relay_parent, events),
			SessionExecutorParams(_relay_parent, session_index, index) =>
				self.requests_cache.cache_session_executor_params(session_index, index),
			SessionInfo(_relay_parent, session_index, info) =>
				if let Some(info) = info {
					self.requests_cache.cache_session_info(session_index, info);
				},
			DmqContents(relay_parent, para_id, messages) =>
				self.requests_cache.cache_dmq_contents((relay_parent, para_id), messages),
			InboundHrmpChannelsContents(relay_parent, para_id, contents) => self
				.requests_cache
				.cache_inbound_hrmp_channel_contents((relay_parent, para_id), contents),
			CurrentBabeEpoch(relay_parent, epoch) =>
				self.requests_cache.cache_current_babe_epoch(relay_parent, epoch),
			FetchOnChainVotes(relay_parent, scraped) =>
				self.requests_cache.cache_on_chain_votes(relay_parent, scraped),
			PvfsRequirePrecheck(relay_parent, pvfs) =>
				self.requests_cache.cache_pvfs_require_precheck(relay_parent, pvfs),
			SubmitPvfCheckStatement(()) => {},
			ValidationCodeHash(relay_parent, para_id, assumption, hash) => self
				.requests_cache
				.cache_validation_code_hash((relay_parent, para_id, assumption), hash),
			Version(relay_parent, version) =>
				self.requests_cache.cache_version(relay_parent, version),
			Disputes(relay_parent, disputes) =>
				self.requests_cache.cache_disputes(relay_parent, disputes),
			UnappliedSlashes(relay_parent, unapplied_slashes) =>
				self.requests_cache.cache_unapplied_slashes(relay_parent, unapplied_slashes),
			KeyOwnershipProof(relay_parent, validator_id, key_ownership_proof) => self
				.requests_cache
				.cache_key_ownership_proof((relay_parent, validator_id), key_ownership_proof),
			ApprovalVotingParams(_relay_parent, session_index, params) =>
				self.requests_cache.cache_approval_voting_params(session_index, params),
			SubmitReportDisputeLost(_) => {},
			DisabledValidators(relay_parent, disabled_validators) =>
				self.requests_cache.cache_disabled_validators(relay_parent, disabled_validators),
			ParaBackingState(relay_parent, para_id, constraints) => self
				.requests_cache
				.cache_para_backing_state((relay_parent, para_id), constraints),
			AsyncBackingParams(relay_parent, params) =>
				self.requests_cache.cache_async_backing_params(relay_parent, params),
			NodeFeatures(session_index, params) =>
				self.requests_cache.cache_node_features(session_index, params),
			ClaimQueue(relay_parent, sender) => {
				self.requests_cache.cache_claim_queue(relay_parent, sender);
			},
			BackingConstraints(relay_parent, para_id, constraints) => self
				.requests_cache
				.cache_backing_constraints((relay_parent, para_id), constraints),
			SchedulingLookahead(session_index, scheduling_lookahead) => self
				.requests_cache
				.cache_scheduling_lookahead(session_index, scheduling_lookahead),
			ValidationCodeBombLimit(session_index, limit) =>
				self.requests_cache.cache_validation_code_bomb_limit(session_index, limit),
			ParaIds(session_index, para_ids) => {
				self.requests_cache.cache_para_ids(session_index, para_ids);
			},
		}
	}

	fn query_cache(&mut self, relay_parent: Hash, request: Request) -> Option<Request> {
		macro_rules! query {
			// Just query by relay parent
			($cache_api_name:ident (), $sender:expr) => {{
				let sender = $sender;
				if let Some(value) = self.requests_cache.$cache_api_name(&relay_parent) {
					let _ = sender.send(Ok(value.clone()));
					self.metrics.on_cached_request();
					None
				} else {
					Some(sender)
				}
			}};
			// Query by relay parent + additional parameters
			($cache_api_name:ident ($($param:expr),+), $sender:expr) => {{
				let sender = $sender;
				if let Some(value) = self.requests_cache.$cache_api_name((relay_parent.clone(), $($param.clone()),+)) {
					self.metrics.on_cached_request();
					let _ = sender.send(Ok(value.clone()));
					None
				} else {
					Some(sender)
				}
			}}
		}

		match request {
			Request::Version(sender) =>
				query!(version(), sender).map(|sender| Request::Version(sender)),
			Request::Authorities(sender) =>
				query!(authorities(), sender).map(|sender| Request::Authorities(sender)),
			Request::Validators(sender) =>
				query!(validators(), sender).map(|sender| Request::Validators(sender)),
			Request::ValidatorGroups(sender) =>
				query!(validator_groups(), sender).map(|sender| Request::ValidatorGroups(sender)),
			Request::AvailabilityCores(sender) => query!(availability_cores(), sender)
				.map(|sender| Request::AvailabilityCores(sender)),
			Request::PersistedValidationData(para, assumption, sender) =>
				query!(persisted_validation_data(para, assumption), sender)
					.map(|sender| Request::PersistedValidationData(para, assumption, sender)),
			Request::AssumedValidationData(
				para,
				expected_persisted_validation_data_hash,
				sender,
			) => query!(
				assumed_validation_data(para, expected_persisted_validation_data_hash),
				sender
			)
			.map(|sender| {
				Request::AssumedValidationData(
					para,
					expected_persisted_validation_data_hash,
					sender,
				)
			}),
			Request::CheckValidationOutputs(para, commitments, sender) =>
				query!(check_validation_outputs(para, commitments), sender)
					.map(|sender| Request::CheckValidationOutputs(para, commitments, sender)),
			Request::SessionIndexForChild(sender) => query!(session_index_for_child(), sender)
				.map(|sender| Request::SessionIndexForChild(sender)),
			Request::ValidationCode(para, assumption, sender) =>
				query!(validation_code(para, assumption), sender)
					.map(|sender| Request::ValidationCode(para, assumption, sender)),
			Request::ValidationCodeByHash(validation_code_hash, sender) =>
				query!(validation_code_by_hash(validation_code_hash), sender)
					.map(|sender| Request::ValidationCodeByHash(validation_code_hash, sender)),
			Request::CandidatePendingAvailability(para, sender) =>
				query!(candidate_pending_availability(para), sender)
					.map(|sender| Request::CandidatePendingAvailability(para, sender)),
			Request::CandidatesPendingAvailability(para, sender) =>
				query!(candidates_pending_availability(para), sender)
					.map(|sender| Request::CandidatesPendingAvailability(para, sender)),
			Request::CandidateEvents(sender) =>
				query!(candidate_events(), sender).map(|sender| Request::CandidateEvents(sender)),
			Request::SessionExecutorParams(session_index, sender) => {
				if let Some(executor_params) =
					self.requests_cache.session_executor_params(session_index)
				{
					self.metrics.on_cached_request();
					let _ = sender.send(Ok(executor_params.clone()));
					None
				} else {
					Some(Request::SessionExecutorParams(session_index, sender))
				}
			},
			Request::SessionInfo(index, sender) => {
				if let Some(info) = self.requests_cache.session_info(index) {
					self.metrics.on_cached_request();
					let _ = sender.send(Ok(Some(info.clone())));
					None
				} else {
					Some(Request::SessionInfo(index, sender))
				}
			},
			Request::DmqContents(id, sender) =>
				query!(dmq_contents(id), sender).map(|sender| Request::DmqContents(id, sender)),
			Request::InboundHrmpChannelsContents(id, sender) =>
				query!(inbound_hrmp_channels_contents(id), sender)
					.map(|sender| Request::InboundHrmpChannelsContents(id, sender)),
			Request::CurrentBabeEpoch(sender) =>
				query!(current_babe_epoch(), sender).map(|sender| Request::CurrentBabeEpoch(sender)),
			Request::FetchOnChainVotes(sender) =>
				query!(on_chain_votes(), sender).map(|sender| Request::FetchOnChainVotes(sender)),
			Request::PvfsRequirePrecheck(sender) => query!(pvfs_require_precheck(), sender)
				.map(|sender| Request::PvfsRequirePrecheck(sender)),
			request @ Request::SubmitPvfCheckStatement(_, _, _) => {
				// This request is side-effecting and thus cannot be cached.
				Some(request)
			},
			Request::ValidationCodeHash(para, assumption, sender) =>
				query!(validation_code_hash(para, assumption), sender)
					.map(|sender| Request::ValidationCodeHash(para, assumption, sender)),
			Request::Disputes(sender) =>
				query!(disputes(), sender).map(|sender| Request::Disputes(sender)),
			Request::UnappliedSlashes(sender) =>
				query!(unapplied_slashes(), sender).map(|sender| Request::UnappliedSlashes(sender)),
			Request::KeyOwnershipProof(validator_id, sender) =>
				query!(key_ownership_proof(validator_id), sender)
					.map(|sender| Request::KeyOwnershipProof(validator_id, sender)),
			Request::SubmitReportDisputeLost(dispute_proof, key_ownership_proof, sender) =>
				query!(submit_report_dispute_lost(dispute_proof, key_ownership_proof), sender).map(
					|sender| {
						Request::SubmitReportDisputeLost(dispute_proof, key_ownership_proof, sender)
					},
				),
			Request::ApprovalVotingParams(session_index, sender) =>
				query!(approval_voting_params(session_index), sender)
					.map(|sender| Request::ApprovalVotingParams(session_index, sender)),
			Request::DisabledValidators(sender) => query!(disabled_validators(), sender)
				.map(|sender| Request::DisabledValidators(sender)),
			Request::ParaBackingState(para, sender) => query!(para_backing_state(para), sender)
				.map(|sender| Request::ParaBackingState(para, sender)),
			Request::AsyncBackingParams(sender) => query!(async_backing_params(), sender)
				.map(|sender| Request::AsyncBackingParams(sender)),
			Request::MinimumBackingVotes(index, sender) => {
				if let Some(value) = self.requests_cache.minimum_backing_votes(index) {
					self.metrics.on_cached_request();
					let _ = sender.send(Ok(value));
					None
				} else {
					Some(Request::MinimumBackingVotes(index, sender))
				}
			},
			Request::NodeFeatures(index, sender) => {
				if let Some(value) = self.requests_cache.node_features(index) {
					self.metrics.on_cached_request();
					let _ = sender.send(Ok(value.clone()));
					None
				} else {
					Some(Request::NodeFeatures(index, sender))
				}
			},
			Request::ClaimQueue(sender) =>
				query!(claim_queue(), sender).map(|sender| Request::ClaimQueue(sender)),
			Request::BackingConstraints(para, sender) => query!(backing_constraints(para), sender)
				.map(|sender| Request::BackingConstraints(para, sender)),
			Request::SchedulingLookahead(index, sender) => {
				if let Some(value) = self.requests_cache.scheduling_lookahead(index) {
					self.metrics.on_cached_request();
					let _ = sender.send(Ok(value));
					None
				} else {
					Some(Request::SchedulingLookahead(index, sender))
				}
			},
			Request::ValidationCodeBombLimit(index, sender) => {
				if let Some(value) = self.requests_cache.validation_code_bomb_limit(index) {
					self.metrics.on_cached_request();
					let _ = sender.send(Ok(value));
					None
				} else {
					Some(Request::ValidationCodeBombLimit(index, sender))
				}
			},
			Request::ParaIds(index, sender) => {
				if let Some(value) = self.requests_cache.para_ids(index) {
					self.metrics.on_cached_request();
					let _ = sender.send(Ok(value.clone()));
					None
				} else {
					Some(Request::ParaIds(index, sender))
				}
			},
		}
	}

	/// Spawn a runtime API request.
	fn spawn_request(&mut self, relay_parent: Hash, request: Request) {
		let client = self.client.clone();
		let metrics = self.metrics.clone();
		let (sender, receiver) = oneshot::channel();

		// TODO: make the cache great again https://github.com/paritytech/polkadot/issues/5546
		let request = match self.query_cache(relay_parent, request) {
			Some(request) => request,
			None => return,
		};

		let request = async move {
			let result = make_runtime_api_request(client, metrics, relay_parent, request).await;
			let _ = sender.send(result);
		}
		.boxed();

		self.spawn_handle
			.spawn_blocking(API_REQUEST_TASK_NAME, Some("runtime-api"), request);
		self.active_requests.push(receiver);
	}

	/// Poll the active runtime API requests.
	async fn poll_requests(&mut self) {
		// If there are no active requests, this future should be pending forever.
		if self.active_requests.len() == 0 {
			return futures::pending!();
		}

		// If there are active requests, this will always resolve to `Some(_)` when a request is
		// finished.
		if let Some(Ok(Some(result))) = self.active_requests.next().await {
			self.store_cache(result);
		}
	}

	/// Returns true if our `active_requests` queue is full.
	fn is_busy(&self) -> bool {
		self.active_requests.len() >= MAX_PARALLEL_REQUESTS
	}
}

#[overseer::contextbounds(RuntimeApi, prefix = self::overseer)]
async fn run<Client, Context>(
	mut ctx: Context,
	mut subsystem: RuntimeApiSubsystem<Client>,
) -> SubsystemResult<()>
where
	Client: RuntimeApiSubsystemClient + Send + Sync + 'static,
{
	loop {
		// Let's add some back pressure when the subsystem is running at `MAX_PARALLEL_REQUESTS`.
		// This can never block forever, because `active_requests` is owned by this task and any
		// mutations happen either in `poll_requests` or `spawn_request` - so if `is_busy` returns
		// true, then even if all of the requests finish before us calling `poll_requests` the
		// `active_requests` length remains invariant.
		if subsystem.is_busy() {
			// Since we are not using any internal waiting queues, we need to wait for exactly
			// one request to complete before we can read the next one from the overseer channel.
			let _ = subsystem.poll_requests().await;
		}

		select! {
			req = ctx.recv().fuse() => match req? {
				FromOrchestra::Signal(OverseerSignal::Conclude) => return Ok(()),
				FromOrchestra::Signal(OverseerSignal::ActiveLeaves(_)) => {},
				FromOrchestra::Signal(OverseerSignal::BlockFinalized(..)) => {},
				FromOrchestra::Communication { msg } => match msg {
					RuntimeApiMessage::Request(relay_parent, request) => {
						subsystem.spawn_request(relay_parent, request);
					},
				}
			},
			_ = subsystem.poll_requests().fuse() => {},
		}
	}
}

async fn make_runtime_api_request<Client>(
	client: Arc<Client>,
	metrics: Metrics,
	relay_parent: Hash,
	request: Request,
) -> Option<RequestResult>
where
	Client: RuntimeApiSubsystemClient + 'static,
{
	let _timer = metrics.time_make_runtime_api_request();

	macro_rules! query {
		($req_variant:ident, $api_name:ident ($($param:expr),*), ver = $version:expr, $sender:expr) => {{
			query!($req_variant, $api_name($($param),*), ver = $version, $sender, result = ( relay_parent $(, $param )* ) )
		}};
		($req_variant:ident, $api_name:ident ($($param:expr),*), ver = $version:expr, $sender:expr, result = ( $($results:expr),* ) ) => {{
			let sender = $sender;
			let version: u32 = $version; // enforce type for the version expression
			let runtime_version = client.api_version_parachain_host(relay_parent).await
				.unwrap_or_else(|e| {
					gum::warn!(
						target: LOG_TARGET,
						api = ?stringify!($api_name),
						"cannot query the runtime API version: {}",
						e,
					);
					Some(0)
				})
				.unwrap_or_else(|| {
					gum::warn!(
						target: LOG_TARGET,
						"no runtime version is reported"
					);
					0
				});

			let res = if runtime_version >= version {
				client.$api_name(relay_parent $(, $param.clone() )*).await
					.map_err(|e| RuntimeApiError::Execution {
						runtime_api_name: stringify!($api_name),
						source: std::sync::Arc::new(e),
					})
			} else {
				Err(RuntimeApiError::NotSupported {
					runtime_api_name: stringify!($api_name),
				})
			};
			metrics.on_request(res.is_ok());
			let _ = sender.send(res.clone());

			res.ok().map(|res| RequestResult::$req_variant($( $results, )* res))
		}}
	}

	match request {
		Request::Version(sender) => {
			let runtime_version = match client.api_version_parachain_host(relay_parent).await {
				Ok(Some(v)) => Ok(v),
				Ok(None) => Err(RuntimeApiError::NotSupported { runtime_api_name: "api_version" }),
				Err(e) => Err(RuntimeApiError::Execution {
					runtime_api_name: "api_version",
					source: std::sync::Arc::new(e),
				}),
			};

			let _ = sender.send(runtime_version.clone());
			runtime_version.ok().map(|v| RequestResult::Version(relay_parent, v))
		},

		Request::Authorities(sender) => query!(Authorities, authorities(), ver = 1, sender),
		Request::Validators(sender) => query!(Validators, validators(), ver = 1, sender),
		Request::ValidatorGroups(sender) => {
			query!(ValidatorGroups, validator_groups(), ver = 1, sender)
		},
		Request::AvailabilityCores(sender) => {
			query!(AvailabilityCores, availability_cores(), ver = 1, sender)
		},
		Request::PersistedValidationData(para, assumption, sender) => query!(
			PersistedValidationData,
			persisted_validation_data(para, assumption),
			ver = 1,
			sender
		),
		Request::AssumedValidationData(para, expected_persisted_validation_data_hash, sender) => {
			query!(
				AssumedValidationData,
				assumed_validation_data(para, expected_persisted_validation_data_hash),
				ver = 1,
				sender
			)
		},
		Request::CheckValidationOutputs(para, commitments, sender) => query!(
			CheckValidationOutputs,
			check_validation_outputs(para, commitments),
			ver = 1,
			sender
		),
		Request::SessionIndexForChild(sender) => {
			query!(SessionIndexForChild, session_index_for_child(), ver = 1, sender)
		},
		Request::ValidationCode(para, assumption, sender) => {
			query!(ValidationCode, validation_code(para, assumption), ver = 1, sender)
		},
		Request::ValidationCodeByHash(validation_code_hash, sender) => query!(
			ValidationCodeByHash,
			validation_code_by_hash(validation_code_hash),
			ver = 1,
			sender
		),
		Request::CandidatePendingAvailability(para, sender) => query!(
			CandidatePendingAvailability,
			candidate_pending_availability(para),
			ver = 1,
			sender
		),
		Request::CandidatesPendingAvailability(para, sender) => query!(
			CandidatesPendingAvailability,
			candidates_pending_availability(para),
			ver = Request::CANDIDATES_PENDING_AVAILABILITY_RUNTIME_REQUIREMENT,
			sender
		),
		Request::CandidateEvents(sender) => {
			query!(CandidateEvents, candidate_events(), ver = 1, sender)
		},
		Request::SessionInfo(index, sender) => {
			query!(SessionInfo, session_info(index), ver = 2, sender)
		},
		Request::SessionExecutorParams(session_index, sender) => query!(
			SessionExecutorParams,
			session_executor_params(session_index),
			ver = Request::EXECUTOR_PARAMS_RUNTIME_REQUIREMENT,
			sender
		),
		Request::DmqContents(id, sender) => query!(DmqContents, dmq_contents(id), ver = 1, sender),
		Request::InboundHrmpChannelsContents(id, sender) => {
			query!(InboundHrmpChannelsContents, inbound_hrmp_channels_contents(id), ver = 1, sender)
		},
		Request::CurrentBabeEpoch(sender) => {
			query!(CurrentBabeEpoch, current_epoch(), ver = 1, sender)
		},
		Request::FetchOnChainVotes(sender) => {
			query!(FetchOnChainVotes, on_chain_votes(), ver = 1, sender)
		},
		Request::SubmitPvfCheckStatement(stmt, signature, sender) => {
			query!(
				SubmitPvfCheckStatement,
				submit_pvf_check_statement(stmt, signature),
				ver = 2,
				sender,
				result = ()
			)
		},
		Request::PvfsRequirePrecheck(sender) => {
			query!(PvfsRequirePrecheck, pvfs_require_precheck(), ver = 2, sender)
		},
		Request::ValidationCodeHash(para, assumption, sender) => {
			query!(ValidationCodeHash, validation_code_hash(para, assumption), ver = 2, sender)
		},
		Request::Disputes(sender) => {
			query!(Disputes, disputes(), ver = Request::DISPUTES_RUNTIME_REQUIREMENT, sender)
		},
		Request::UnappliedSlashes(sender) => query!(
			UnappliedSlashes,
			unapplied_slashes(),
			ver = Request::UNAPPLIED_SLASHES_RUNTIME_REQUIREMENT,
			sender
		),
		Request::KeyOwnershipProof(validator_id, sender) => query!(
			KeyOwnershipProof,
			key_ownership_proof(validator_id),
			ver = Request::KEY_OWNERSHIP_PROOF_RUNTIME_REQUIREMENT,
			sender
		),
		Request::ApprovalVotingParams(session_index, sender) => {
			query!(
				ApprovalVotingParams,
				approval_voting_params(session_index),
				ver = Request::APPROVAL_VOTING_PARAMS_REQUIREMENT,
				sender
			)
		},
		Request::SubmitReportDisputeLost(dispute_proof, key_ownership_proof, sender) => query!(
			SubmitReportDisputeLost,
			submit_report_dispute_lost(dispute_proof, key_ownership_proof),
			ver = Request::SUBMIT_REPORT_DISPUTE_LOST_RUNTIME_REQUIREMENT,
			sender,
			result = ()
		),
		Request::MinimumBackingVotes(index, sender) => query!(
			MinimumBackingVotes,
			minimum_backing_votes(index),
			ver = Request::MINIMUM_BACKING_VOTES_RUNTIME_REQUIREMENT,
			sender,
			result = (index)
		),
		Request::DisabledValidators(sender) => query!(
			DisabledValidators,
			disabled_validators(),
			ver = Request::DISABLED_VALIDATORS_RUNTIME_REQUIREMENT,
			sender
		),
		Request::ParaBackingState(para, sender) => {
			query!(
				ParaBackingState,
				para_backing_state(para),
				ver = Request::ASYNC_BACKING_STATE_RUNTIME_REQUIREMENT,
				sender
			)
		},
		Request::AsyncBackingParams(sender) => {
			query!(
				AsyncBackingParams,
				async_backing_params(),
				ver = Request::ASYNC_BACKING_STATE_RUNTIME_REQUIREMENT,
				sender
			)
		},
		Request::NodeFeatures(index, sender) => query!(
			NodeFeatures,
			node_features(),
			ver = Request::NODE_FEATURES_RUNTIME_REQUIREMENT,
			sender,
			result = (index)
		),
		Request::ClaimQueue(sender) => query!(
			ClaimQueue,
			claim_queue(),
			ver = Request::CLAIM_QUEUE_RUNTIME_REQUIREMENT,
			sender
		),
		Request::BackingConstraints(para, sender) => {
			query!(
				BackingConstraints,
				backing_constraints(para),
				ver = Request::CONSTRAINTS_RUNTIME_REQUIREMENT,
				sender
			)
		},
		Request::SchedulingLookahead(index, sender) => query!(
			SchedulingLookahead,
			scheduling_lookahead(),
			ver = Request::SCHEDULING_LOOKAHEAD_RUNTIME_REQUIREMENT,
			sender,
			result = (index)
		),
		Request::ValidationCodeBombLimit(index, sender) => query!(
			ValidationCodeBombLimit,
			validation_code_bomb_limit(),
			ver = Request::VALIDATION_CODE_BOMB_LIMIT_RUNTIME_REQUIREMENT,
			sender,
			result = (index)
		),
		Request::ParaIds(index, sender) => query!(
			ParaIds,
			para_ids(),
			ver = Request::PARAIDS_RUNTIME_REQUIREMENT,
			sender,
			result = (index)
		),
	}
}
