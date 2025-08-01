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

use std::{
	collections::HashSet,
	sync::{Arc, Mutex},
	task::Poll,
	time::Instant,
};

use crate::tests::{create_spawner, test_config};

use super::*;
use futures::{
	channel::mpsc::{self, channel},
	executor::{block_on, LocalPool},
	future::FutureExt,
	sink::SinkExt,
	task::LocalSpawn,
};
use prometheus_endpoint::prometheus::default_registry;
use sc_client_api::HeaderBackend;
use sc_network::{
	service::signature::{Keypair, SigningError},
	PublicKey, Signature,
};
use sc_network_types::{
	kad::Key as KademliaKey,
	multiaddr::{Multiaddr, Protocol},
	PeerId,
};
use sp_api::{ApiRef, ProvideRuntimeApi};
use sp_keystore::{testing::MemoryKeystore, Keystore};
use sp_runtime::traits::{Block as BlockT, NumberFor, Zero};
use substrate_test_runtime_client::runtime::Block;

#[derive(Clone)]
pub(crate) struct TestApi {
	pub(crate) authorities: Vec<AuthorityId>,
}

impl ProvideRuntimeApi<Block> for TestApi {
	type Api = RuntimeApi;

	fn runtime_api(&self) -> ApiRef<'_, Self::Api> {
		RuntimeApi { authorities: self.authorities.clone() }.into()
	}
}

/// Blockchain database header backend. Does not perform any validation.
impl<Block: BlockT> HeaderBackend<Block> for TestApi {
	fn header(
		&self,
		_hash: Block::Hash,
	) -> std::result::Result<Option<Block::Header>, sp_blockchain::Error> {
		Ok(None)
	}

	fn info(&self) -> sc_client_api::blockchain::Info<Block> {
		sc_client_api::blockchain::Info {
			best_hash: Default::default(),
			best_number: Zero::zero(),
			finalized_hash: Default::default(),
			finalized_number: Zero::zero(),
			genesis_hash: Default::default(),
			number_leaves: Default::default(),
			finalized_state: None,
			block_gap: None,
		}
	}

	fn status(
		&self,
		_hash: Block::Hash,
	) -> std::result::Result<sc_client_api::blockchain::BlockStatus, sp_blockchain::Error> {
		Ok(sc_client_api::blockchain::BlockStatus::Unknown)
	}

	fn number(
		&self,
		_hash: Block::Hash,
	) -> std::result::Result<Option<NumberFor<Block>>, sp_blockchain::Error> {
		Ok(None)
	}

	fn hash(
		&self,
		_number: NumberFor<Block>,
	) -> std::result::Result<Option<Block::Hash>, sp_blockchain::Error> {
		Ok(None)
	}
}

pub(crate) struct RuntimeApi {
	authorities: Vec<AuthorityId>,
}

sp_api::mock_impl_runtime_apis! {
	impl AuthorityDiscoveryApi<Block> for RuntimeApi {
		fn authorities(&self) -> Vec<AuthorityId> {
			self.authorities.clone()
		}
	}
}

#[derive(Debug)]
pub enum TestNetworkEvent {
	GetCalled,
	PutCalled,
	PutToCalled,
	StoreRecordCalled,
}

pub struct TestNetwork {
	peer_id: sc_network_types::PeerId,
	identity: Keypair,
	external_addresses: Vec<Multiaddr>,
	// Whenever functions on `TestNetwork` are called, the function arguments are added to the
	// vectors below.
	pub put_value_call: Arc<Mutex<Vec<(KademliaKey, Vec<u8>)>>>,
	pub put_value_to_call: Arc<Mutex<Vec<(Record, HashSet<sc_network_types::PeerId>, bool)>>>,
	pub get_value_call: Arc<Mutex<Vec<KademliaKey>>>,
	pub store_value_call:
		Arc<Mutex<Vec<(KademliaKey, Vec<u8>, Option<sc_network_types::PeerId>, Option<Instant>)>>>,

	event_sender: mpsc::UnboundedSender<TestNetworkEvent>,
	event_receiver: Option<mpsc::UnboundedReceiver<TestNetworkEvent>>,
}

impl TestNetwork {
	fn get_event_receiver(&mut self) -> Option<mpsc::UnboundedReceiver<TestNetworkEvent>> {
		self.event_receiver.take()
	}
}

impl Default for TestNetwork {
	fn default() -> Self {
		let (tx, rx) = mpsc::unbounded();
		let identity = Keypair::generate_ed25519();
		TestNetwork {
			peer_id: identity.public().to_peer_id(),
			identity,
			external_addresses: vec!["/ip6/2001:db8::/tcp/30333".parse().unwrap()],
			put_value_call: Default::default(),
			get_value_call: Default::default(),
			put_value_to_call: Default::default(),
			store_value_call: Default::default(),
			event_sender: tx,
			event_receiver: Some(rx),
		}
	}
}

impl NetworkSigner for TestNetwork {
	fn sign_with_local_identity(
		&self,
		msg: Vec<u8>,
	) -> std::result::Result<Signature, SigningError> {
		Signature::sign_message(msg, &self.identity)
	}

	fn verify(
		&self,
		peer_id: sc_network_types::PeerId,
		public_key: &Vec<u8>,
		signature: &Vec<u8>,
		message: &Vec<u8>,
	) -> std::result::Result<bool, String> {
		let public_key =
			PublicKey::try_decode_protobuf(&public_key).map_err(|error| error.to_string())?;
		let peer_id: PeerId = peer_id.into();
		let remote: PeerId = public_key.to_peer_id().into();

		Ok(peer_id == remote && public_key.verify(message, signature))
	}
}

impl NetworkDHTProvider for TestNetwork {
	fn put_value(&self, key: KademliaKey, value: Vec<u8>) {
		self.put_value_call.lock().unwrap().push((key.clone(), value.clone()));
		self.event_sender.clone().unbounded_send(TestNetworkEvent::PutCalled).unwrap();
	}
	fn get_value(&self, key: &KademliaKey) {
		self.get_value_call.lock().unwrap().push(key.clone());
		self.event_sender.clone().unbounded_send(TestNetworkEvent::GetCalled).unwrap();
	}

	fn put_record_to(
		&self,
		record: Record,
		peers: HashSet<sc_network_types::PeerId>,
		update_local_storage: bool,
	) {
		self.put_value_to_call.lock().unwrap().push((
			record.clone(),
			peers.clone(),
			update_local_storage,
		));
		self.event_sender.clone().unbounded_send(TestNetworkEvent::PutToCalled).unwrap();
	}

	fn store_record(
		&self,
		key: KademliaKey,
		value: Vec<u8>,
		publisher: Option<PeerId>,
		expires: Option<Instant>,
	) {
		self.store_value_call.lock().unwrap().push((
			key.clone(),
			value.clone(),
			publisher,
			expires,
		));
		self.event_sender
			.clone()
			.unbounded_send(TestNetworkEvent::StoreRecordCalled)
			.unwrap();
	}

	fn start_providing(&self, _: KademliaKey) {
		unimplemented!()
	}

	fn stop_providing(&self, _: KademliaKey) {
		unimplemented!()
	}

	fn get_providers(&self, _: KademliaKey) {
		unimplemented!()
	}

	fn find_closest_peers(&self, _: PeerId) {
		unimplemented!()
	}
}

impl NetworkStateInfo for TestNetwork {
	fn local_peer_id(&self) -> sc_network_types::PeerId {
		self.peer_id.into()
	}

	fn external_addresses(&self) -> Vec<Multiaddr> {
		self.external_addresses.clone()
	}

	fn listen_addresses(&self) -> Vec<Multiaddr> {
		self.external_addresses.clone()
	}
}

struct TestSigner<'a> {
	keypair: &'a Keypair,
}

impl<'a> NetworkSigner for TestSigner<'a> {
	fn sign_with_local_identity(
		&self,
		msg: Vec<u8>,
	) -> std::result::Result<Signature, SigningError> {
		Signature::sign_message(msg, self.keypair)
	}

	fn verify(
		&self,
		_: sc_network_types::PeerId,
		_: &Vec<u8>,
		_: &Vec<u8>,
		_: &Vec<u8>,
	) -> std::result::Result<bool, String> {
		unimplemented!();
	}
}

fn build_dht_event<Signer: NetworkSigner>(
	addresses: Vec<Multiaddr>,
	public_key: AuthorityId,
	key_store: &MemoryKeystore,
	network: Option<&Signer>,
	creation_time: Option<schema::TimestampInfo>,
) -> Vec<(KademliaKey, Vec<u8>)> {
	let serialized_record =
		serialize_authority_record(serialize_addresses(addresses.into_iter()), creation_time)
			.unwrap();

	let peer_signature = network.map(|n| sign_record_with_peer_id(&serialized_record, n).unwrap());
	let kv_pairs = sign_record_with_authority_ids(
		serialized_record,
		peer_signature,
		key_store,
		vec![public_key.into()],
	)
	.unwrap();
	// There is always a single item in it, because we signed it with a single key
	kv_pairs
}

#[tokio::test]
async fn new_registers_metrics() {
	let (_dht_event_tx, dht_event_rx) = mpsc::channel(1000);
	let network: Arc<TestNetwork> = Arc::new(Default::default());
	let key_store = MemoryKeystore::new();
	let test_api = Arc::new(TestApi { authorities: vec![] });

	let registry = prometheus_endpoint::Registry::new();

	let tempdir = tempfile::tempdir().unwrap();
	let path = tempdir.path().to_path_buf();
	let (_to_worker, from_service) = mpsc::channel(0);
	Worker::new(
		from_service,
		test_api,
		network.clone(),
		Box::pin(dht_event_rx),
		Role::PublishAndDiscover(key_store.into()),
		Some(registry.clone()),
		test_config(Some(path)),
		create_spawner(),
	);

	assert!(registry.gather().len() > 0);
}

#[tokio::test]
async fn triggers_dht_get_query() {
	sp_tracing::try_init_simple();
	let (_dht_event_tx, dht_event_rx) = channel(1000);

	// Generate authority keys
	let authority_1_key_pair = AuthorityPair::from_seed_slice(&[1; 32]).unwrap();
	let authority_2_key_pair = AuthorityPair::from_seed_slice(&[2; 32]).unwrap();
	let authorities = vec![authority_1_key_pair.public(), authority_2_key_pair.public()];

	let test_api = Arc::new(TestApi { authorities: authorities.clone() });

	let network = Arc::new(TestNetwork::default());
	let key_store = MemoryKeystore::new();

	let (_to_worker, from_service) = mpsc::channel(0);
	let tempdir = tempfile::tempdir().unwrap();
	let path = tempdir.path().to_path_buf();
	let mut worker = Worker::new(
		from_service,
		test_api,
		network.clone(),
		Box::pin(dht_event_rx),
		Role::PublishAndDiscover(key_store.into()),
		None,
		test_config(Some(path)),
		create_spawner(),
	);

	futures::executor::block_on(async {
		worker.refill_pending_lookups_queue().await.unwrap();
		worker.start_new_lookups();
		assert_eq!(network.get_value_call.lock().unwrap().len(), authorities.len());
	})
}

#[tokio::test]
async fn publish_discover_cycle() {
	sp_tracing::try_init_simple();

	let mut pool = LocalPool::new();

	// Node A publishing its address.

	let (_dht_event_tx, dht_event_rx) = channel(1000);

	let network: Arc<TestNetwork> = Arc::new(Default::default());

	let key_store = MemoryKeystore::new();
	let _ = pool.spawner().spawn_local_obj(
		async move {
			let node_a_public =
				key_store.sr25519_generate_new(key_types::AUTHORITY_DISCOVERY, None).unwrap();
			let test_api = Arc::new(TestApi { authorities: vec![node_a_public.into()] });

			let (_to_worker, from_service) = mpsc::channel(0);
			let tempdir = tempfile::tempdir().unwrap();
			let temppath = tempdir.path();
			let path = temppath.to_path_buf();
			let mut worker = Worker::new(
				from_service,
				test_api,
				network.clone(),
				Box::pin(dht_event_rx),
				Role::PublishAndDiscover(key_store.into()),
				None,
				test_config(Some(path)),
				create_spawner(),
			);

			worker.publish_ext_addresses(false).await.unwrap();

			// Expect authority discovery to put a new record onto the dht.
			assert_eq!(network.put_value_call.lock().unwrap().len(), 1);

			let dht_event = {
				let (key, value) = network.put_value_call.lock().unwrap().pop().unwrap();
				DhtEvent::ValueFound(PeerRecord {
					peer: None,
					record: Record { key, value, publisher: None, expires: None },
				})
			};

			// Node B discovering node A's address.

			let (mut dht_event_tx, dht_event_rx) = channel(1000);
			let test_api = Arc::new(TestApi {
				// Make sure node B identifies node A as an authority.
				authorities: vec![node_a_public.into()],
			});
			let network: Arc<TestNetwork> = Arc::new(Default::default());
			let key_store = MemoryKeystore::new();

			let (_to_worker, from_service) = mpsc::channel(0);
			let tempdir = tempfile::tempdir().unwrap();
			let temppath = tempdir.path();
			let path = temppath.to_path_buf();
			let mut worker = Worker::new(
				from_service,
				test_api,
				network.clone(),
				Box::pin(dht_event_rx),
				Role::PublishAndDiscover(key_store.into()),
				None,
				test_config(Some(path)),
				create_spawner(),
			);

			dht_event_tx.try_send(dht_event.clone()).unwrap();

			worker.refill_pending_lookups_queue().await.unwrap();
			worker.start_new_lookups();

			// Make authority discovery handle the event.
			worker.handle_dht_event(dht_event).await;
		}
		.boxed_local()
		.into(),
	);

	pool.run();
}

/// Don't terminate when sender side of service channel is dropped. Terminate when network event
/// stream terminates.
#[tokio::test]
async fn terminate_when_event_stream_terminates() {
	let (dht_event_tx, dht_event_rx) = channel(1000);
	let network: Arc<TestNetwork> = Arc::new(Default::default());
	let key_store = MemoryKeystore::new();
	let test_api = Arc::new(TestApi { authorities: vec![] });
	let path = tempfile::tempdir().unwrap().path().to_path_buf();

	let (to_worker, from_service) = mpsc::channel(0);
	let worker = Worker::new(
		from_service,
		test_api,
		network.clone(),
		Box::pin(dht_event_rx),
		Role::PublishAndDiscover(key_store.into()),
		None,
		test_config(Some(path)),
		create_spawner(),
	)
	.run();
	futures::pin_mut!(worker);

	block_on(async {
		assert_eq!(Poll::Pending, futures::poll!(&mut worker));

		// Drop sender side of service channel.
		drop(to_worker);
		assert_eq!(
			Poll::Pending,
			futures::poll!(&mut worker),
			"Expect the authority discovery module not to terminate once the \
			sender side of the service channel is closed.",
		);

		// Simulate termination of the network through dropping the sender side
		// of the dht event channel.
		drop(dht_event_tx);

		assert_eq!(
			Poll::Ready(()),
			futures::poll!(&mut worker),
			"Expect the authority discovery module to terminate once the \
			 sending side of the dht event channel is closed.",
		);
	});
}

#[tokio::test]
async fn dont_stop_polling_dht_event_stream_after_bogus_event() {
	let remote_multiaddr = {
		let peer_id = PeerId::random();
		let address: Multiaddr = "/ip6/2001:db8:0:0:0:0:0:1/tcp/30333".parse().unwrap();

		address.with(Protocol::P2p(peer_id.into()))
	};
	let remote_key_store = MemoryKeystore::new();
	let remote_public_key: AuthorityId = remote_key_store
		.sr25519_generate_new(key_types::AUTHORITY_DISCOVERY, None)
		.unwrap()
		.into();

	let (mut dht_event_tx, dht_event_rx) = channel(1);
	let (network, mut network_events) = {
		let mut n = TestNetwork::default();
		let r = n.get_event_receiver().unwrap();
		(Arc::new(n), r)
	};

	let key_store = MemoryKeystore::new();
	let test_api = Arc::new(TestApi { authorities: vec![remote_public_key.clone()] });
	let mut pool = LocalPool::new();

	let (mut to_worker, from_service) = mpsc::channel(1);
	let tempdir = tempfile::tempdir().unwrap();
	let path = tempdir.path().to_path_buf();
	let mut worker = Worker::new(
		from_service,
		test_api,
		network.clone(),
		Box::pin(dht_event_rx),
		Role::PublishAndDiscover(Arc::new(key_store)),
		None,
		test_config(Some(path)),
		create_spawner(),
	);

	// Spawn the authority discovery to make sure it is polled independently.
	//
	// As this is a local pool, only one future at a time will have the CPU and
	// can make progress until the future returns `Pending`.
	let _ = pool.spawner().spawn_local_obj(
		async move {
			// Refilling `pending_lookups` only happens every X minutes. Fast
			// forward by calling `refill_pending_lookups_queue` directly.
			worker.refill_pending_lookups_queue().await.unwrap();
			worker.run().await
		}
		.boxed_local()
		.into(),
	);

	pool.run_until(async {
		// Assert worker to trigger a lookup for the one and only authority.
		assert!(matches!(network_events.next().await, Some(TestNetworkEvent::GetCalled)));

		// Send an event that should generate an error
		dht_event_tx
			.send(DhtEvent::ValueFound(PeerRecord {
				peer: None,
				record: Record {
					key: vec![0x9u8].into(),
					value: Default::default(),
					publisher: None,
					expires: None,
				},
			}))
			.await
			.expect("Channel has capacity of 1.");

		// Make previously triggered lookup succeed.
		let kv_pairs: Vec<PeerRecord> = build_dht_event::<TestNetwork>(
			vec![remote_multiaddr.clone()],
			remote_public_key.clone(),
			&remote_key_store,
			None,
			Some(build_creation_time()),
		)
		.into_iter()
		.map(|(key, value)| PeerRecord {
			peer: None,
			record: Record { key, value, publisher: None, expires: None },
		})
		.collect();

		for kv_pair in kv_pairs {
			dht_event_tx
				.send(DhtEvent::ValueFound(kv_pair))
				.await
				.expect("Channel has capacity of 1.");
		}

		// Expect authority discovery to function normally, now knowing the
		// address for the remote node.
		let (sender, addresses) = futures::channel::oneshot::channel();
		to_worker
			.send(ServicetoWorkerMsg::GetAddressesByAuthorityId(remote_public_key, sender))
			.await
			.expect("Channel has capacity of 1.");
		assert_eq!(Some(HashSet::from([remote_multiaddr])), addresses.await.unwrap());
	});
}

struct DhtValueFoundTester {
	pub remote_key_store: MemoryKeystore,
	pub remote_authority_public: sp_core::sr25519::Public,
	pub remote_node_key: Keypair,
	pub local_worker: Option<
		Worker<
			TestApi,
			sp_runtime::generic::Block<
				sp_runtime::generic::Header<u64, sp_runtime::traits::BlakeTwo256>,
				substrate_test_runtime_client::runtime::Extrinsic,
			>,
			std::pin::Pin<Box<futures::channel::mpsc::Receiver<DhtEvent>>>,
		>,
	>,
}

impl DhtValueFoundTester {
	fn new() -> Self {
		let remote_key_store = MemoryKeystore::new();
		let remote_authority_public = remote_key_store
			.sr25519_generate_new(key_types::AUTHORITY_DISCOVERY, None)
			.unwrap();

		let remote_node_key = Keypair::generate_ed25519();
		Self { remote_key_store, remote_authority_public, remote_node_key, local_worker: None }
	}

	fn multiaddr_with_peer_id(&self, idx: u16) -> Multiaddr {
		let peer_id = self.remote_node_key.public().to_peer_id();
		let address: Multiaddr =
			format!("/ip6/2001:db8:0:0:0:0:0:{:x}/tcp/30333", idx).parse().unwrap();

		address.with(multiaddr::Protocol::P2p(peer_id.into()))
	}

	fn process_value_found(
		&mut self,
		strict_record_validation: bool,
		values: Vec<(KademliaKey, Vec<u8>)>,
	) -> (Option<HashSet<Multiaddr>>, Option<Arc<TestNetwork>>) {
		let (_dht_event_tx, dht_event_rx) = channel(1);
		let local_test_api =
			Arc::new(TestApi { authorities: vec![self.remote_authority_public.into()] });
		let local_key_store = MemoryKeystore::new();

		let (_to_worker, from_service) = mpsc::channel(0);
		let (local_worker, local_network) = if let Some(local_work) = self.local_worker.as_mut() {
			(local_work, None)
		} else {
			let local_network: Arc<TestNetwork> = Arc::new(Default::default());

			self.local_worker = Some(Worker::new(
				from_service,
				local_test_api,
				local_network.clone(),
				Box::pin(dht_event_rx),
				Role::PublishAndDiscover(Arc::new(local_key_store)),
				None,
				WorkerConfig {
					strict_record_validation,
					persisted_cache_directory: Some(
						tempfile::tempdir()
							.expect("Should be able to create tmp dir")
							.path()
							.to_path_buf(),
					),
					..Default::default()
				},
				create_spawner(),
			));
			(self.local_worker.as_mut().unwrap(), Some(local_network))
		};

		block_on(local_worker.refill_pending_lookups_queue()).unwrap();
		local_worker.start_new_lookups();

		for record in values.into_iter().map(|(key, value)| PeerRecord {
			peer: Some(PeerId::random().into()),
			record: Record { key, value, publisher: None, expires: None },
		}) {
			drop(local_worker.handle_dht_value_found_event(record))
		}

		(
			self.local_worker
				.as_ref()
				.map(|w| {
					w.addr_cache
						.get_addresses_by_authority_id(&self.remote_authority_public.into())
						.cloned()
				})
				.unwrap(),
			local_network,
		)
	}
}

#[tokio::test]
async fn limit_number_of_addresses_added_to_cache_per_authority() {
	let mut tester = DhtValueFoundTester::new();
	assert!(MAX_ADDRESSES_PER_AUTHORITY < 100);
	let addresses = (1..100).map(|i| tester.multiaddr_with_peer_id(i)).collect();
	let kv_pairs = build_dht_event::<TestNetwork>(
		addresses,
		tester.remote_authority_public.into(),
		&tester.remote_key_store,
		None,
		Some(build_creation_time()),
	);

	let cached_remote_addresses = tester.process_value_found(false, kv_pairs).0;
	assert_eq!(MAX_ADDRESSES_PER_AUTHORITY, cached_remote_addresses.unwrap().len());
}

#[tokio::test]
async fn strict_accept_address_with_peer_signature() {
	let mut tester = DhtValueFoundTester::new();
	let addr = tester.multiaddr_with_peer_id(1);
	let kv_pairs = build_dht_event(
		vec![addr.clone()],
		tester.remote_authority_public.into(),
		&tester.remote_key_store,
		Some(&TestSigner { keypair: &tester.remote_node_key }),
		Some(build_creation_time()),
	);

	let cached_remote_addresses = tester.process_value_found(true, kv_pairs).0;

	assert_eq!(
		Some(HashSet::from([addr])),
		cached_remote_addresses,
		"Expect worker to only cache `Multiaddr`s with `PeerId`s.",
	);
}

#[tokio::test]
async fn strict_accept_address_without_creation_time() {
	let mut tester = DhtValueFoundTester::new();
	let addr = tester.multiaddr_with_peer_id(1);
	let kv_pairs = build_dht_event(
		vec![addr.clone()],
		tester.remote_authority_public.into(),
		&tester.remote_key_store,
		Some(&TestSigner { keypair: &tester.remote_node_key }),
		None,
	);

	let cached_remote_addresses = tester.process_value_found(true, kv_pairs).0;

	assert_eq!(
		Some(HashSet::from([addr])),
		cached_remote_addresses,
		"Expect worker to cache address without creation time",
	);
}

#[tokio::test]
async fn keep_last_received_if_no_creation_time() {
	let mut tester: DhtValueFoundTester = DhtValueFoundTester::new();
	let addr = tester.multiaddr_with_peer_id(1);
	let kv_pairs = build_dht_event(
		vec![addr.clone()],
		tester.remote_authority_public.into(),
		&tester.remote_key_store,
		Some(&TestSigner { keypair: &tester.remote_node_key }),
		None,
	);

	let (cached_remote_addresses, network) = tester.process_value_found(true, kv_pairs);

	assert_eq!(
		Some(HashSet::from([addr])),
		cached_remote_addresses,
		"Expect worker to cache address without creation time",
	);

	assert!(network
		.as_ref()
		.map(|network| network.put_value_to_call.lock().unwrap().is_empty())
		.unwrap_or_default());

	let addr2 = tester.multiaddr_with_peer_id(2);
	let kv_pairs = build_dht_event(
		vec![addr2.clone()],
		tester.remote_authority_public.into(),
		&tester.remote_key_store,
		Some(&TestSigner { keypair: &tester.remote_node_key }),
		None,
	);

	let cached_remote_addresses = tester.process_value_found(true, kv_pairs).0;

	assert_eq!(
		Some(HashSet::from([addr2])),
		cached_remote_addresses,
		"Expect worker to cache last received when no creation time",
	);
	assert!(network
		.as_ref()
		.map(|network| network.put_value_to_call.lock().unwrap().is_empty())
		.unwrap_or_default());
}

#[tokio::test]
async fn records_with_incorrectly_signed_creation_time_are_ignored() {
	let mut tester: DhtValueFoundTester = DhtValueFoundTester::new();
	let addr = tester.multiaddr_with_peer_id(1);
	let kv_pairs = build_dht_event(
		vec![addr.clone()],
		tester.remote_authority_public.into(),
		&tester.remote_key_store,
		Some(&TestSigner { keypair: &tester.remote_node_key }),
		Some(build_creation_time()),
	);

	let (cached_remote_addresses, network) = tester.process_value_found(true, kv_pairs);

	assert_eq!(
		Some(HashSet::from([addr.clone()])),
		cached_remote_addresses,
		"Expect worker to cache record with creation time",
	);
	assert!(network
		.as_ref()
		.map(|network| network.put_value_to_call.lock().unwrap().is_empty())
		.unwrap_or_default());

	let alternative_key = tester
		.remote_key_store
		.sr25519_generate_new(key_types::AUTHORITY_DISCOVERY, None)
		.unwrap();

	let addr2 = tester.multiaddr_with_peer_id(2);
	let mut kv_pairs = build_dht_event(
		vec![addr2.clone()],
		alternative_key.into(),
		&tester.remote_key_store,
		Some(&TestSigner { keypair: &tester.remote_node_key }),
		Some(build_creation_time()),
	);
	let kademlia_key = hash_authority_id(tester.remote_authority_public.as_slice());
	for key in kv_pairs.iter_mut() {
		key.0 = kademlia_key.clone();
	}
	let cached_remote_addresses = tester.process_value_found(true, kv_pairs).0;

	assert_eq!(
		Some(HashSet::from([addr])),
		cached_remote_addresses,
		"Expect `Multiaddr` to remain the same",
	);
	assert!(network
		.as_ref()
		.map(|network| network.put_value_to_call.lock().unwrap().is_empty())
		.unwrap_or_default());
}

#[tokio::test]
async fn newer_records_overwrite_older_ones() {
	let mut tester: DhtValueFoundTester = DhtValueFoundTester::new();
	let old_record = tester.multiaddr_with_peer_id(1);
	let kv_pairs = build_dht_event(
		vec![old_record.clone()],
		tester.remote_authority_public.into(),
		&tester.remote_key_store,
		Some(&TestSigner { keypair: &tester.remote_node_key }),
		Some(build_creation_time()),
	);

	let (cached_remote_addresses, network) = tester.process_value_found(true, kv_pairs);

	assert_eq!(
		Some(HashSet::from([old_record])),
		cached_remote_addresses,
		"Expect worker to cache record with creation time",
	);

	let nothing_updated = network
		.as_ref()
		.map(|network| network.put_value_to_call.lock().unwrap().is_empty())
		.unwrap();
	assert!(nothing_updated);

	let new_record = tester.multiaddr_with_peer_id(2);
	let kv_pairs = build_dht_event(
		vec![new_record.clone()],
		tester.remote_authority_public.into(),
		&tester.remote_key_store,
		Some(&TestSigner { keypair: &tester.remote_node_key }),
		Some(build_creation_time()),
	);

	let cached_remote_addresses = tester.process_value_found(true, kv_pairs).0;

	assert_eq!(
		Some(HashSet::from([new_record])),
		cached_remote_addresses,
		"Expect worker to store the newest recrod",
	);

	let result = network
		.as_ref()
		.map(|network| network.put_value_to_call.lock().unwrap().first().unwrap().clone())
		.unwrap();
	assert!(matches!(result, (_, _, false)));
	assert_eq!(result.1.len(), 1);
}

#[tokio::test]
async fn older_records_dont_affect_newer_ones() {
	let mut tester: DhtValueFoundTester = DhtValueFoundTester::new();
	let old_record = tester.multiaddr_with_peer_id(1);
	let old_kv_pairs = build_dht_event(
		vec![old_record.clone()],
		tester.remote_authority_public.into(),
		&tester.remote_key_store,
		Some(&TestSigner { keypair: &tester.remote_node_key }),
		Some(build_creation_time()),
	);

	let new_record = tester.multiaddr_with_peer_id(2);
	let kv_pairs = build_dht_event(
		vec![new_record.clone()],
		tester.remote_authority_public.into(),
		&tester.remote_key_store,
		Some(&TestSigner { keypair: &tester.remote_node_key }),
		Some(build_creation_time()),
	);

	let (cached_remote_addresses, network) = tester.process_value_found(true, kv_pairs);

	assert_eq!(
		Some(HashSet::from([new_record.clone()])),
		cached_remote_addresses,
		"Expect worker to store new record",
	);

	let nothing_updated = network
		.as_ref()
		.map(|network| network.put_value_to_call.lock().unwrap().is_empty())
		.unwrap();
	assert!(nothing_updated);

	let cached_remote_addresses = tester.process_value_found(true, old_kv_pairs).0;

	assert_eq!(
		Some(HashSet::from([new_record])),
		cached_remote_addresses,
		"Expect worker to not update stored record",
	);

	let update_peers_info = network
		.as_ref()
		.map(|network| network.put_value_to_call.lock().unwrap().remove(0))
		.unwrap();
	assert!(matches!(update_peers_info, (_, _, false)));
	assert_eq!(update_peers_info.1.len(), 1);
}

#[tokio::test]
async fn reject_address_with_rogue_peer_signature() {
	let mut tester = DhtValueFoundTester::new();
	let rogue_remote_node_key = Keypair::generate_ed25519();
	let kv_pairs = build_dht_event(
		vec![tester.multiaddr_with_peer_id(1)],
		tester.remote_authority_public.into(),
		&tester.remote_key_store,
		Some(&TestSigner { keypair: &rogue_remote_node_key }),
		Some(build_creation_time()),
	);

	let cached_remote_addresses = tester.process_value_found(false, kv_pairs).0;

	assert!(
		cached_remote_addresses.is_none(),
		"Expected worker to ignore record signed by a different key.",
	);
}

#[tokio::test]
async fn reject_address_with_invalid_peer_signature() {
	let mut tester = DhtValueFoundTester::new();
	let mut kv_pairs = build_dht_event(
		vec![tester.multiaddr_with_peer_id(1)],
		tester.remote_authority_public.into(),
		&tester.remote_key_store,
		Some(&TestSigner { keypair: &tester.remote_node_key }),
		Some(build_creation_time()),
	);
	// tamper with the signature
	let mut record = schema::SignedAuthorityRecord::decode(kv_pairs[0].1.as_slice()).unwrap();
	record.peer_signature.as_mut().map(|p| p.signature[1] = !p.signature[1]);
	record.encode(&mut kv_pairs[0].1).unwrap();

	let cached_remote_addresses = tester.process_value_found(false, kv_pairs).0;

	assert!(
		cached_remote_addresses.is_none(),
		"Expected worker to ignore record with tampered signature.",
	);
}

#[tokio::test]
async fn reject_address_without_peer_signature() {
	let mut tester = DhtValueFoundTester::new();
	let kv_pairs = build_dht_event::<TestNetwork>(
		vec![tester.multiaddr_with_peer_id(1)],
		tester.remote_authority_public.into(),
		&tester.remote_key_store,
		None,
		Some(build_creation_time()),
	);

	let cached_remote_addresses = tester.process_value_found(true, kv_pairs).0;

	assert!(cached_remote_addresses.is_none(), "Expected worker to ignore unsigned record.",);
}

#[tokio::test]
async fn do_not_cache_addresses_without_peer_id() {
	let mut tester = DhtValueFoundTester::new();
	let multiaddr_with_peer_id = tester.multiaddr_with_peer_id(1);
	let multiaddr_without_peer_id: Multiaddr =
		"/ip6/2001:db8:0:0:0:0:0:2/tcp/30333".parse().unwrap();
	let kv_pairs = build_dht_event::<TestNetwork>(
		vec![multiaddr_with_peer_id.clone(), multiaddr_without_peer_id],
		tester.remote_authority_public.into(),
		&tester.remote_key_store,
		None,
		Some(build_creation_time()),
	);

	let cached_remote_addresses = tester.process_value_found(false, kv_pairs).0;

	assert_eq!(
		Some(HashSet::from([multiaddr_with_peer_id])),
		cached_remote_addresses,
		"Expect worker to only cache `Multiaddr`s with `PeerId`s.",
	);
}

#[tokio::test]
async fn addresses_to_publish_adds_p2p() {
	let (_dht_event_tx, dht_event_rx) = channel(1000);
	let network: Arc<TestNetwork> = Arc::new(Default::default());

	assert!(!matches!(
		network.external_addresses().pop().unwrap().pop().unwrap(),
		multiaddr::Protocol::P2p(_)
	));

	let (_to_worker, from_service) = mpsc::channel(0);
	let tempdir = tempfile::tempdir().unwrap();
	let path = tempdir.path().to_path_buf();
	let mut worker = Worker::new(
		from_service,
		Arc::new(TestApi { authorities: vec![] }),
		network.clone(),
		Box::pin(dht_event_rx),
		Role::PublishAndDiscover(MemoryKeystore::new().into()),
		Some(prometheus_endpoint::Registry::new()),
		test_config(Some(path)),
		create_spawner(),
	);

	assert!(
		matches!(
			worker.addresses_to_publish().next().unwrap().pop().unwrap(),
			multiaddr::Protocol::P2p(_)
		),
		"Expect `addresses_to_publish` to append `p2p` protocol component.",
	);
}

/// Ensure [`Worker::addresses_to_publish`] does not add an additional `p2p` protocol component in
/// case one already exists.
#[tokio::test]
async fn addresses_to_publish_respects_existing_p2p_protocol() {
	let (_dht_event_tx, dht_event_rx) = channel(1000);
	let identity = Keypair::generate_ed25519();
	let peer_id = identity.public().to_peer_id();
	let external_address = "/ip6/2001:db8::/tcp/30333"
		.parse::<Multiaddr>()
		.unwrap()
		.with(multiaddr::Protocol::P2p(peer_id.into()));
	let network: Arc<TestNetwork> = Arc::new(TestNetwork {
		peer_id,
		identity,
		external_addresses: vec![external_address],
		..Default::default()
	});

	let (_to_worker, from_service) = mpsc::channel(0);
	let tempdir = tempfile::tempdir().unwrap();
	let path = tempdir.path().to_path_buf();
	let mut worker = Worker::new(
		from_service,
		Arc::new(TestApi { authorities: vec![] }),
		network.clone(),
		Box::pin(dht_event_rx),
		Role::PublishAndDiscover(MemoryKeystore::new().into()),
		Some(prometheus_endpoint::Registry::new()),
		test_config(Some(path)),
		create_spawner(),
	);

	assert_eq!(
		network.external_addresses,
		worker.addresses_to_publish().collect::<Vec<_>>(),
		"Expected Multiaddr from `TestNetwork` to not be altered.",
	);
}

#[tokio::test]
async fn lookup_throttling() {
	let remote_multiaddr = {
		let peer_id = PeerId::random();
		let address: Multiaddr = "/ip6/2001:db8:0:0:0:0:0:1/tcp/30333".parse().unwrap();

		address.with(multiaddr::Protocol::P2p(peer_id.into()))
	};
	let remote_key_store = MemoryKeystore::new();
	let remote_public_keys: Vec<AuthorityId> = (0..20)
		.map(|_| {
			remote_key_store
				.sr25519_generate_new(key_types::AUTHORITY_DISCOVERY, None)
				.unwrap()
				.into()
		})
		.collect();
	let remote_hash_to_key = remote_public_keys
		.iter()
		.map(|k| (hash_authority_id(k.as_ref()), k.clone()))
		.collect::<HashMap<_, _>>();

	let (mut dht_event_tx, dht_event_rx) = channel(1);
	let (_to_worker, from_service) = mpsc::channel(0);
	let mut network = TestNetwork::default();
	let mut receiver = network.get_event_receiver().unwrap();
	let network = Arc::new(network);
	let tempdir = tempfile::tempdir().unwrap();
	let path = tempdir.path().to_path_buf();
	let mut worker = Worker::new(
		from_service,
		Arc::new(TestApi { authorities: remote_public_keys.clone() }),
		network.clone(),
		dht_event_rx.boxed(),
		Role::Discover,
		Some(default_registry().clone()),
		test_config(Some(path)),
		create_spawner(),
	);

	let mut pool = LocalPool::new();
	let metrics = worker.metrics.clone().unwrap();

	let _ = pool.spawner().spawn_local_obj(
		async move {
			// Refilling `pending_lookups` only happens every X minutes. Fast
			// forward by calling `refill_pending_lookups_queue` directly.
			worker.refill_pending_lookups_queue().await.unwrap();
			worker.run().await
		}
		.boxed_local()
		.into(),
	);

	pool.run_until(
		async {
			// Assert worker to trigger MAX_IN_FLIGHT_LOOKUPS lookups.
			for _ in 0..MAX_IN_FLIGHT_LOOKUPS {
				assert!(matches!(receiver.next().await, Some(TestNetworkEvent::GetCalled)));
			}
			assert_eq!(
				metrics.requests_pending.get(),
				(remote_public_keys.len() - MAX_IN_FLIGHT_LOOKUPS) as u64
			);
			assert_eq!(network.get_value_call.lock().unwrap().len(), MAX_IN_FLIGHT_LOOKUPS);

			// Make first lookup succeed.
			let remote_hash = network.get_value_call.lock().unwrap().pop().unwrap();
			let remote_key: AuthorityId = remote_hash_to_key.get(&remote_hash).unwrap().clone();
			let kv_pairs = build_dht_event::<TestNetwork>(
				vec![remote_multiaddr.clone()],
				remote_key,
				&remote_key_store,
				None,
				Some(build_creation_time()),
			)
			.into_iter()
			.map(|(key, value)| PeerRecord {
				peer: None,
				record: Record { key, value, publisher: None, expires: None },
			});
			for kv_pair in kv_pairs {
				dht_event_tx
					.send(DhtEvent::ValueFound(kv_pair))
					.await
					.expect("Channel has capacity of 1.");
			}

			// Assert worker to trigger another lookup.
			assert!(matches!(receiver.next().await, Some(TestNetworkEvent::GetCalled)));
			assert_eq!(
				metrics.requests_pending.get(),
				(remote_public_keys.len() - MAX_IN_FLIGHT_LOOKUPS - 1) as u64
			);
			assert_eq!(network.get_value_call.lock().unwrap().len(), MAX_IN_FLIGHT_LOOKUPS);

			// Make second one fail.
			let remote_hash = network.get_value_call.lock().unwrap().pop().unwrap();
			let dht_event = DhtEvent::ValueNotFound(remote_hash);
			dht_event_tx.send(dht_event).await.expect("Channel has capacity of 1.");

			// Assert worker to trigger another lookup.
			assert!(matches!(receiver.next().await, Some(TestNetworkEvent::GetCalled)));
			assert_eq!(
				metrics.requests_pending.get(),
				(remote_public_keys.len() - MAX_IN_FLIGHT_LOOKUPS - 2) as u64
			);
			assert_eq!(network.get_value_call.lock().unwrap().len(), MAX_IN_FLIGHT_LOOKUPS);
		}
		.boxed_local(),
	);
}

#[tokio::test]
async fn test_handle_put_record_request() {
	let local_node_network = TestNetwork::default();
	let remote_node_network = TestNetwork::default();
	let peer_id = remote_node_network.peer_id;

	let remote_multiaddr = {
		let address: Multiaddr = "/ip6/2001:db8:0:0:0:0:0:1/tcp/30333".parse().unwrap();

		address.with(multiaddr::Protocol::P2p(remote_node_network.peer_id.into()))
	};

	println!("{:?}", remote_multiaddr);
	let remote_key_store = MemoryKeystore::new();
	let remote_public_keys: Vec<AuthorityId> = (0..20)
		.map(|_| {
			remote_key_store
				.sr25519_generate_new(key_types::AUTHORITY_DISCOVERY, None)
				.unwrap()
				.into()
		})
		.collect();

	let remote_non_authorithy_keys: Vec<AuthorityId> = (0..20)
		.map(|_| {
			remote_key_store
				.sr25519_generate_new(key_types::AUTHORITY_DISCOVERY, None)
				.unwrap()
				.into()
		})
		.collect();

	let (_dht_event_tx, dht_event_rx) = channel(1);
	let (_to_worker, from_service) = mpsc::channel(0);
	let network = Arc::new(local_node_network);
	let tempdir = tempfile::tempdir().unwrap();
	let path = tempdir.path().to_path_buf();
	let mut worker = Worker::new(
		from_service,
		Arc::new(TestApi { authorities: remote_public_keys.clone() }),
		network.clone(),
		dht_event_rx.boxed(),
		Role::Discover,
		Some(default_registry().clone()),
		test_config(Some(path)),
		create_spawner(),
	);

	let mut pool = LocalPool::new();

	let valid_authorithy_key = remote_public_keys.first().unwrap().clone();

	let kv_pairs = build_dht_event(
		vec![remote_multiaddr.clone()],
		valid_authorithy_key.clone().into(),
		&remote_key_store,
		Some(&TestSigner { keypair: &remote_node_network.identity }),
		Some(build_creation_time()),
	);

	pool.run_until(
		async {
			// Invalid format should return an error.
			for authority in remote_public_keys.iter() {
				let key = hash_authority_id(authority.as_ref());
				assert!(matches!(
					worker.handle_put_record_requested(key, vec![0x0], Some(peer_id), None).await,
					Err(Error::DecodingProto(_))
				));
			}
			let prev_requested_authorithies = worker.authorities_queried_at;

			// Unknown authority should return an error.
			for authority in remote_non_authorithy_keys.iter() {
				let key = hash_authority_id(authority.as_ref());
				assert!(matches!(
					worker.handle_put_record_requested(key, vec![0x0], Some(peer_id), None).await,
					Err(Error::UnknownAuthority)
				));
				assert!(prev_requested_authorithies == worker.authorities_queried_at);
			}
			assert_eq!(network.store_value_call.lock().unwrap().len(), 0);

			// Valid authority should return Ok.
			for (key, value) in kv_pairs.clone() {
				assert!(worker
					.handle_put_record_requested(key, value, Some(peer_id), None)
					.await
					.is_ok());
			}
			assert_eq!(network.store_value_call.lock().unwrap().len(), 1);

			let another_authorithy_id = remote_public_keys.get(3).unwrap().clone();
			let key = hash_authority_id(another_authorithy_id.as_ref());

			// Valid record signed with a different key should return error.
			for (_, value) in kv_pairs.clone() {
				assert!(matches!(
					worker
						.handle_put_record_requested(key.clone(), value, Some(peer_id), None)
						.await,
					Err(Error::VerifyingDhtPayload)
				));
			}
			assert_eq!(network.store_value_call.lock().unwrap().len(), 1);
			let newer_kv_pairs = build_dht_event(
				vec![remote_multiaddr],
				valid_authorithy_key.clone().into(),
				&remote_key_store,
				Some(&TestSigner { keypair: &remote_node_network.identity }),
				Some(build_creation_time()),
			);

			// Valid old authority, should not throw error, but it should not be stored since a
			// newer one already exists.
			for (new_key, new_value) in newer_kv_pairs.clone() {
				worker.in_flight_lookups.insert(new_key.clone(), valid_authorithy_key.clone());

				let found = PeerRecord {
					peer: Some(peer_id.into()),
					record: Record {
						key: new_key,
						value: new_value,
						publisher: Some(peer_id.into()),
						expires: None,
					},
				};
				assert!(worker.handle_dht_value_found_event(found).is_ok());
			}

			for (key, value) in kv_pairs.clone() {
				assert!(worker
					.handle_put_record_requested(key, value, Some(peer_id), None)
					.await
					.is_ok());
			}
			assert_eq!(network.store_value_call.lock().unwrap().len(), 1);

			// Newer kv pairs should always be stored.
			for (key, value) in newer_kv_pairs.clone() {
				assert!(worker
					.handle_put_record_requested(key, value, Some(peer_id), None)
					.await
					.is_ok());
			}

			assert_eq!(network.store_value_call.lock().unwrap().len(), 2);

			worker.refill_pending_lookups_queue().await.unwrap();
			assert_eq!(worker.last_known_records.len(), 1);

			// Check known records gets clean up, when an authorithy gets out of the
			// active set.
			worker.client = Arc::new(TestApi { authorities: Default::default() });
			worker.refill_pending_lookups_queue().await.unwrap();
			assert_eq!(worker.last_known_records.len(), 0);
		}
		.boxed_local(),
	);
}
