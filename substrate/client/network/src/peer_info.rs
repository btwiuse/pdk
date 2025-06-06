// This file is part of Substrate.

// Copyright (C) Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with this program. If not, see <https://www.gnu.org/licenses/>.

//! [`PeerInfoBehaviour`] is implementation of `NetworkBehaviour` that holds information about peers
//! in cache.

use crate::{utils::interval, LOG_TARGET};
use either::Either;

use fnv::FnvHashMap;
use futures::prelude::*;
use libp2p::{
	core::{transport::PortUse, ConnectedPoint, Endpoint},
	identify::{
		Behaviour as Identify, Config as IdentifyConfig, Event as IdentifyEvent,
		Info as IdentifyInfo,
	},
	identity::PublicKey,
	multiaddr::Protocol,
	ping::{Behaviour as Ping, Config as PingConfig, Event as PingEvent},
	swarm::{
		behaviour::{
			AddressChange, ConnectionClosed, ConnectionEstablished, DialFailure, FromSwarm,
			ListenFailure,
		},
		ConnectionDenied, ConnectionHandler, ConnectionHandlerSelect, ConnectionId,
		NetworkBehaviour, THandler, THandlerInEvent, THandlerOutEvent, ToSwarm,
	},
	Multiaddr, PeerId,
};
use log::{debug, error, trace, warn};
use parking_lot::Mutex;
use schnellru::{ByLength, LruMap};
use smallvec::SmallVec;

use std::{
	collections::{hash_map::Entry, HashSet, VecDeque},
	iter,
	pin::Pin,
	sync::Arc,
	task::{Context, Poll},
	time::{Duration, Instant},
};

/// Time after we disconnect from a node before we purge its information from the cache.
const CACHE_EXPIRE: Duration = Duration::from_secs(10 * 60);
/// Interval at which we perform garbage collection on the node info.
const GARBAGE_COLLECT_INTERVAL: Duration = Duration::from_secs(2 * 60);
/// The maximum number of tracked external addresses we allow.
const MAX_EXTERNAL_ADDRESSES: u32 = 32;
/// Number of times observed address is received from different peers before it is confirmed as
/// external.
const MIN_ADDRESS_CONFIRMATIONS: usize = 3;

/// Implementation of `NetworkBehaviour` that holds information about peers in cache.
pub struct PeerInfoBehaviour {
	/// Periodically ping nodes, and close the connection if it's unresponsive.
	ping: Ping,
	/// Periodically identifies the remote and responds to incoming requests.
	identify: Identify,
	/// Information that we know about all nodes.
	nodes_info: FnvHashMap<PeerId, NodeInfo>,
	/// Interval at which we perform garbage collection in `nodes_info`.
	garbage_collect: Pin<Box<dyn Stream<Item = ()> + Send>>,
	/// PeerId of the local node.
	local_peer_id: PeerId,
	/// Public addresses supplied by the operator. Never expire.
	public_addresses: Vec<Multiaddr>,
	/// Listen addresses. External addresses matching listen addresses never expire.
	listen_addresses: HashSet<Multiaddr>,
	/// External address confirmations.
	address_confirmations: LruMap<Multiaddr, HashSet<PeerId>>,
	/// Record keeping of external addresses. Data is queried by the `NetworkService`.
	/// The addresses contain the `/p2p/...` part with local peer ID.
	external_addresses: ExternalAddresses,
	/// Pending events to emit to [`Swarm`](libp2p::swarm::Swarm).
	pending_actions: VecDeque<ToSwarm<PeerInfoEvent, THandlerInEvent<PeerInfoBehaviour>>>,
}

/// Information about a node we're connected to.
#[derive(Debug)]
struct NodeInfo {
	/// When we will remove the entry about this node from the list, or `None` if we're connected
	/// to the node.
	info_expire: Option<Instant>,
	/// Non-empty list of connected endpoints, one per connection.
	endpoints: SmallVec<[ConnectedPoint; crate::MAX_CONNECTIONS_PER_PEER]>,
	/// Version reported by the remote, or `None` if unknown.
	client_version: Option<String>,
	/// Latest ping time with this node.
	latest_ping: Option<Duration>,
}

impl NodeInfo {
	fn new(endpoint: ConnectedPoint) -> Self {
		let mut endpoints = SmallVec::new();
		endpoints.push(endpoint);
		Self { info_expire: None, endpoints, client_version: None, latest_ping: None }
	}
}

/// Utility struct for tracking external addresses. The data is shared with the `NetworkService`.
#[derive(Debug, Clone, Default)]
pub struct ExternalAddresses {
	addresses: Arc<Mutex<HashSet<Multiaddr>>>,
}

impl ExternalAddresses {
	/// Add an external address.
	pub fn add(&mut self, addr: Multiaddr) -> bool {
		self.addresses.lock().insert(addr)
	}

	/// Remove an external address.
	pub fn remove(&mut self, addr: &Multiaddr) -> bool {
		self.addresses.lock().remove(addr)
	}
}

impl PeerInfoBehaviour {
	/// Builds a new `PeerInfoBehaviour`.
	pub fn new(
		user_agent: String,
		local_public_key: PublicKey,
		external_addresses: Arc<Mutex<HashSet<Multiaddr>>>,
		public_addresses: Vec<Multiaddr>,
	) -> Self {
		let identify = {
			let cfg = IdentifyConfig::new("/substrate/1.0".to_string(), local_public_key.clone())
				.with_agent_version(user_agent)
				// We don't need any peer information cached.
				.with_cache_size(0);
			Identify::new(cfg)
		};

		Self {
			ping: Ping::new(PingConfig::new()),
			identify,
			nodes_info: FnvHashMap::default(),
			garbage_collect: Box::pin(interval(GARBAGE_COLLECT_INTERVAL)),
			local_peer_id: local_public_key.to_peer_id(),
			public_addresses,
			listen_addresses: HashSet::new(),
			address_confirmations: LruMap::new(ByLength::new(MAX_EXTERNAL_ADDRESSES)),
			external_addresses: ExternalAddresses { addresses: external_addresses },
			pending_actions: Default::default(),
		}
	}

	/// Borrows `self` and returns a struct giving access to the information about a node.
	///
	/// Returns `None` if we don't know anything about this node. Always returns `Some` for nodes
	/// we're connected to, meaning that if `None` is returned then we're not connected to that
	/// node.
	pub fn node(&self, peer_id: &PeerId) -> Option<Node> {
		self.nodes_info.get(peer_id).map(Node)
	}

	/// Inserts a ping time in the cache. Has no effect if we don't have any entry for that node,
	/// which shouldn't happen.
	fn handle_ping_report(
		&mut self,
		peer_id: &PeerId,
		ping_time: Duration,
		connection: ConnectionId,
	) {
		trace!(target: LOG_TARGET, "Ping time with {:?} via {:?}: {:?}", peer_id, connection, ping_time);
		if let Some(entry) = self.nodes_info.get_mut(peer_id) {
			entry.latest_ping = Some(ping_time);
		} else {
			error!(target: LOG_TARGET,
				"Received ping from node we're not connected to {:?} via {:?}", peer_id, connection);
		}
	}

	/// Ensure address has the `/p2p/...` part with local peer id. Returns `Err` if the address
	/// already contains a different peer id.
	fn with_local_peer_id(&self, address: Multiaddr) -> Result<Multiaddr, Multiaddr> {
		if let Some(Protocol::P2p(peer_id)) = address.iter().last() {
			if peer_id == self.local_peer_id {
				Ok(address)
			} else {
				Err(address)
			}
		} else {
			Ok(address.with(Protocol::P2p(self.local_peer_id)))
		}
	}

	/// Inserts an identify record in the cache & discovers external addresses when multiple
	/// peers report the same address as observed.
	fn handle_identify_report(&mut self, peer_id: &PeerId, info: &IdentifyInfo) {
		trace!(target: LOG_TARGET, "Identified {:?} => {:?}", peer_id, info);
		if let Some(entry) = self.nodes_info.get_mut(peer_id) {
			entry.client_version = Some(info.agent_version.clone());
		} else {
			error!(target: LOG_TARGET,
				"Received identify message from node we're not connected to {peer_id:?}");
		}
		// Discover external addresses.
		match self.with_local_peer_id(info.observed_addr.clone()) {
			Ok(observed_addr) => {
				let (is_new, expired) = self.is_new_external_address(&observed_addr, *peer_id);
				if is_new && self.external_addresses.add(observed_addr.clone()) {
					trace!(
						target: LOG_TARGET,
						"Observed address reported by Identify confirmed as external {}",
						observed_addr,
					);
					self.pending_actions.push_back(ToSwarm::ExternalAddrConfirmed(observed_addr));
				}
				if let Some(expired) = expired {
					trace!(target: LOG_TARGET, "Removing replaced external address: {expired}");
					self.external_addresses.remove(&expired);
					self.pending_actions.push_back(ToSwarm::ExternalAddrExpired(expired));
				}
			},
			Err(addr) => {
				warn!(
					target: LOG_TARGET,
					"Identify reported observed address for a peer that is not us: {addr}",
				);
			},
		}
	}

	/// Check if addresses are equal taking into account they can contain or not contain
	/// the `/p2p/...` part.
	fn is_same_address(left: &Multiaddr, right: &Multiaddr) -> bool {
		let mut left = left.iter();
		let mut right = right.iter();

		loop {
			match (left.next(), right.next()) {
				(None, None) => return true,
				(None, Some(Protocol::P2p(_))) => return true,
				(Some(Protocol::P2p(_)), None) => return true,
				(left, right) if left != right => return false,
				_ => {},
			}
		}
	}

	/// Check if `address` can be considered a new external address.
	///
	/// If this address replaces an older address, the expired address is returned.
	fn is_new_external_address(
		&mut self,
		address: &Multiaddr,
		peer_id: PeerId,
	) -> (bool, Option<Multiaddr>) {
		trace!(target: LOG_TARGET, "Verify new external address: {address}");

		// Public and listen addresses don't count towards discovered external addresses
		// and are always confirmed.
		// Because they are not kept in the LRU, they are never replaced by discovered
		// external addresses.
		if self
			.listen_addresses
			.iter()
			.chain(self.public_addresses.iter())
			.any(|known_address| PeerInfoBehaviour::is_same_address(&known_address, &address))
		{
			return (true, None)
		}

		match self.address_confirmations.get(address) {
			Some(confirmations) => {
				confirmations.insert(peer_id);

				if confirmations.len() >= MIN_ADDRESS_CONFIRMATIONS {
					return (true, None)
				}
			},
			None => {
				let oldest = (self.address_confirmations.len() >=
					self.address_confirmations.limiter().max_length() as usize)
					.then(|| {
						self.address_confirmations.pop_oldest().map(|(address, peers)| {
							if peers.len() >= MIN_ADDRESS_CONFIRMATIONS {
								return Some(address)
							} else {
								None
							}
						})
					})
					.flatten()
					.flatten();

				self.address_confirmations
					.insert(address.clone(), iter::once(peer_id).collect());

				return (false, oldest)
			},
		}

		(false, None)
	}
}

/// Gives access to the information about a node.
pub struct Node<'a>(&'a NodeInfo);

impl<'a> Node<'a> {
	/// Returns the endpoint of an established connection to the peer.
	///
	/// Returns `None` if we are disconnected from the node.
	pub fn endpoint(&self) -> Option<&'a ConnectedPoint> {
		self.0.endpoints.get(0)
	}

	/// Returns the latest version information we know of.
	pub fn client_version(&self) -> Option<&'a str> {
		self.0.client_version.as_deref()
	}

	/// Returns the latest ping time we know of for this node. `None` if we never successfully
	/// pinged this node.
	pub fn latest_ping(&self) -> Option<Duration> {
		self.0.latest_ping
	}
}

/// Event that can be emitted by the behaviour.
#[derive(Debug)]
pub enum PeerInfoEvent {
	/// We have obtained identity information from a peer, including the addresses it is listening
	/// on.
	Identified {
		/// Id of the peer that has been identified.
		peer_id: PeerId,
		/// Information about the peer.
		info: IdentifyInfo,
	},
}

impl NetworkBehaviour for PeerInfoBehaviour {
	type ConnectionHandler = ConnectionHandlerSelect<
		<Ping as NetworkBehaviour>::ConnectionHandler,
		<Identify as NetworkBehaviour>::ConnectionHandler,
	>;
	type ToSwarm = PeerInfoEvent;

	fn handle_pending_inbound_connection(
		&mut self,
		connection_id: ConnectionId,
		local_addr: &Multiaddr,
		remote_addr: &Multiaddr,
	) -> Result<(), ConnectionDenied> {
		self.ping
			.handle_pending_inbound_connection(connection_id, local_addr, remote_addr)?;
		self.identify
			.handle_pending_inbound_connection(connection_id, local_addr, remote_addr)
	}

	fn handle_pending_outbound_connection(
		&mut self,
		_connection_id: ConnectionId,
		_maybe_peer: Option<PeerId>,
		_addresses: &[Multiaddr],
		_effective_role: Endpoint,
	) -> Result<Vec<Multiaddr>, ConnectionDenied> {
		// Only `Discovery::handle_pending_outbound_connection` must be returning addresses to
		// ensure that we don't return unwanted addresses.
		Ok(Vec::new())
	}

	fn handle_established_inbound_connection(
		&mut self,
		connection_id: ConnectionId,
		peer: PeerId,
		local_addr: &Multiaddr,
		remote_addr: &Multiaddr,
	) -> Result<THandler<Self>, ConnectionDenied> {
		let ping_handler = self.ping.handle_established_inbound_connection(
			connection_id,
			peer,
			local_addr,
			remote_addr,
		)?;
		let identify_handler = self.identify.handle_established_inbound_connection(
			connection_id,
			peer,
			local_addr,
			remote_addr,
		)?;
		Ok(ping_handler.select(identify_handler))
	}

	fn handle_established_outbound_connection(
		&mut self,
		connection_id: ConnectionId,
		peer: PeerId,
		addr: &Multiaddr,
		role_override: Endpoint,
		port_use: PortUse,
	) -> Result<THandler<Self>, ConnectionDenied> {
		let ping_handler = self.ping.handle_established_outbound_connection(
			connection_id,
			peer,
			addr,
			role_override,
			port_use,
		)?;
		let identify_handler = self.identify.handle_established_outbound_connection(
			connection_id,
			peer,
			addr,
			role_override,
			port_use,
		)?;
		Ok(ping_handler.select(identify_handler))
	}

	fn on_swarm_event(&mut self, event: FromSwarm) {
		match event {
			FromSwarm::ConnectionEstablished(
				e @ ConnectionEstablished { peer_id, endpoint, .. },
			) => {
				self.ping.on_swarm_event(FromSwarm::ConnectionEstablished(e));
				self.identify.on_swarm_event(FromSwarm::ConnectionEstablished(e));

				match self.nodes_info.entry(peer_id) {
					Entry::Vacant(e) => {
						e.insert(NodeInfo::new(endpoint.clone()));
					},
					Entry::Occupied(e) => {
						let e = e.into_mut();
						if e.info_expire.as_ref().map(|exp| *exp < Instant::now()).unwrap_or(false)
						{
							e.client_version = None;
							e.latest_ping = None;
						}
						e.info_expire = None;
						e.endpoints.push(endpoint.clone());
					},
				}
			},
			FromSwarm::ConnectionClosed(ConnectionClosed {
				peer_id,
				connection_id,
				endpoint,
				cause,
				remaining_established,
			}) => {
				self.ping.on_swarm_event(FromSwarm::ConnectionClosed(ConnectionClosed {
					peer_id,
					connection_id,
					endpoint,
					cause,
					remaining_established,
				}));
				self.identify.on_swarm_event(FromSwarm::ConnectionClosed(ConnectionClosed {
					peer_id,
					connection_id,
					endpoint,
					cause,
					remaining_established,
				}));

				if let Some(entry) = self.nodes_info.get_mut(&peer_id) {
					if remaining_established == 0 {
						entry.info_expire = Some(Instant::now() + CACHE_EXPIRE);
					}
					entry.endpoints.retain(|ep| ep != endpoint)
				} else {
					error!(target: LOG_TARGET,
						"Unknown connection to {:?} closed: {:?}", peer_id, endpoint);
				}
			},
			FromSwarm::DialFailure(DialFailure { peer_id, error, connection_id }) => {
				self.ping.on_swarm_event(FromSwarm::DialFailure(DialFailure {
					peer_id,
					error,
					connection_id,
				}));
				self.identify.on_swarm_event(FromSwarm::DialFailure(DialFailure {
					peer_id,
					error,
					connection_id,
				}));
			},
			FromSwarm::ListenerClosed(e) => {
				self.ping.on_swarm_event(FromSwarm::ListenerClosed(e));
				self.identify.on_swarm_event(FromSwarm::ListenerClosed(e));
			},
			FromSwarm::ListenFailure(ListenFailure {
				local_addr,
				send_back_addr,
				error,
				connection_id,
				peer_id,
			}) => {
				self.ping.on_swarm_event(FromSwarm::ListenFailure(ListenFailure {
					local_addr,
					send_back_addr,
					error,
					connection_id,
					peer_id,
				}));
				self.identify.on_swarm_event(FromSwarm::ListenFailure(ListenFailure {
					local_addr,
					send_back_addr,
					error,
					connection_id,
					peer_id,
				}));
			},
			FromSwarm::ListenerError(e) => {
				self.ping.on_swarm_event(FromSwarm::ListenerError(e));
				self.identify.on_swarm_event(FromSwarm::ListenerError(e));
			},
			FromSwarm::ExternalAddrExpired(e) => {
				self.ping.on_swarm_event(FromSwarm::ExternalAddrExpired(e));
				self.identify.on_swarm_event(FromSwarm::ExternalAddrExpired(e));
			},
			FromSwarm::NewListener(e) => {
				self.ping.on_swarm_event(FromSwarm::NewListener(e));
				self.identify.on_swarm_event(FromSwarm::NewListener(e));
			},
			FromSwarm::NewListenAddr(e) => {
				self.ping.on_swarm_event(FromSwarm::NewListenAddr(e));
				self.identify.on_swarm_event(FromSwarm::NewListenAddr(e));
				self.listen_addresses.insert(e.addr.clone());
			},
			FromSwarm::ExpiredListenAddr(e) => {
				self.ping.on_swarm_event(FromSwarm::ExpiredListenAddr(e));
				self.identify.on_swarm_event(FromSwarm::ExpiredListenAddr(e));
				self.listen_addresses.remove(e.addr);
				// Remove matching external address.
				match self.with_local_peer_id(e.addr.clone()) {
					Ok(addr) => {
						self.external_addresses.remove(&addr);
						self.pending_actions.push_back(ToSwarm::ExternalAddrExpired(addr));
					},
					Err(addr) => {
						warn!(
							target: LOG_TARGET,
							"Listen address expired with peer ID that is not us: {addr}",
						);
					},
				}
			},
			FromSwarm::NewExternalAddrCandidate(e) => {
				self.ping.on_swarm_event(FromSwarm::NewExternalAddrCandidate(e));
				self.identify.on_swarm_event(FromSwarm::NewExternalAddrCandidate(e));
			},
			FromSwarm::ExternalAddrConfirmed(e) => {
				self.ping.on_swarm_event(FromSwarm::ExternalAddrConfirmed(e));
				self.identify.on_swarm_event(FromSwarm::ExternalAddrConfirmed(e));
			},
			FromSwarm::AddressChange(e @ AddressChange { peer_id, old, new, .. }) => {
				self.ping.on_swarm_event(FromSwarm::AddressChange(e));
				self.identify.on_swarm_event(FromSwarm::AddressChange(e));

				if let Some(entry) = self.nodes_info.get_mut(&peer_id) {
					if let Some(endpoint) = entry.endpoints.iter_mut().find(|e| e == &old) {
						*endpoint = new.clone();
					} else {
						error!(target: LOG_TARGET,
							"Unknown address change for peer {:?} from {:?} to {:?}", peer_id, old, new);
					}
				} else {
					error!(target: LOG_TARGET,
						"Unknown peer {:?} to change address from {:?} to {:?}", peer_id, old, new);
				}
			},
			FromSwarm::NewExternalAddrOfPeer(e) => {
				self.ping.on_swarm_event(FromSwarm::NewExternalAddrOfPeer(e));
				self.identify.on_swarm_event(FromSwarm::NewExternalAddrOfPeer(e));
			},
			event => {
				debug!(target: LOG_TARGET, "New unknown `FromSwarm` libp2p event: {event:?}");
				self.ping.on_swarm_event(event);
				self.identify.on_swarm_event(event);
			},
		}
	}

	fn on_connection_handler_event(
		&mut self,
		peer_id: PeerId,
		connection_id: ConnectionId,
		event: THandlerOutEvent<Self>,
	) {
		match event {
			Either::Left(event) =>
				self.ping.on_connection_handler_event(peer_id, connection_id, event),
			Either::Right(event) =>
				self.identify.on_connection_handler_event(peer_id, connection_id, event),
		}
	}

	fn poll(&mut self, cx: &mut Context) -> Poll<ToSwarm<Self::ToSwarm, THandlerInEvent<Self>>> {
		if let Some(event) = self.pending_actions.pop_front() {
			return Poll::Ready(event)
		}

		loop {
			match self.ping.poll(cx) {
				Poll::Pending => break,
				Poll::Ready(ToSwarm::GenerateEvent(ev)) => {
					if let PingEvent { peer, result: Ok(rtt), connection } = ev {
						self.handle_ping_report(&peer, rtt, connection)
					}
				},
				Poll::Ready(event) => {
					return Poll::Ready(event.map_in(Either::Left).map_out(|_| {
						unreachable!("`GenerateEvent` is handled in a branch above; qed")
					}));
				},
			}
		}

		loop {
			match self.identify.poll(cx) {
				Poll::Pending => break,
				Poll::Ready(ToSwarm::GenerateEvent(event)) => match event {
					IdentifyEvent::Received { peer_id, info, .. } => {
						self.handle_identify_report(&peer_id, &info);
						let event = PeerInfoEvent::Identified { peer_id, info };
						return Poll::Ready(ToSwarm::GenerateEvent(event))
					},
					IdentifyEvent::Error { connection_id, peer_id, error } => {
						debug!(
							target: LOG_TARGET,
							"Identification with peer {peer_id:?}({connection_id}) failed => {error}"
						);
					},
					IdentifyEvent::Pushed { .. } => {},
					IdentifyEvent::Sent { .. } => {},
				},
				Poll::Ready(event) => {
					return Poll::Ready(event.map_in(Either::Right).map_out(|_| {
						unreachable!("`GenerateEvent` is handled in a branch above; qed")
					}));
				},
			}
		}

		while let Poll::Ready(Some(())) = self.garbage_collect.poll_next_unpin(cx) {
			self.nodes_info.retain(|_, node| {
				node.info_expire.as_ref().map(|exp| *exp >= Instant::now()).unwrap_or(true)
			});
		}

		Poll::Pending
	}
}
