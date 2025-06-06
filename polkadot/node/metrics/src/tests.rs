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

//! Polkadot runtime metrics integration test.

use http_body_util::BodyExt;
use hyper::Uri;
use hyper_util::{client::legacy::Client, rt::TokioExecutor};
use polkadot_primitives::metric_definitions::PARACHAIN_INHERENT_DATA_BITFIELDS_PROCESSED;
use polkadot_test_service::{node_config, run_validator_node, test_prometheus_config};
use sp_keyring::Sr25519Keyring::*;
use std::collections::HashMap;

const DEFAULT_PROMETHEUS_PORT: u16 = 9616;

#[tokio::test(flavor = "multi_thread")]
async fn runtime_can_publish_metrics() {
	let mut alice_config =
		node_config(|| {}, tokio::runtime::Handle::current(), Alice, Vec::new(), true);

	// Enable Prometheus metrics for Alice.
	alice_config.prometheus_config = Some(test_prometheus_config(DEFAULT_PROMETHEUS_PORT));

	let mut builder = sc_cli::LoggerBuilder::new("");

	// Enable profiling with `wasm_tracing` target.
	builder.with_profiling(Default::default(), String::from("wasm_tracing=trace"));

	// Setup the runtime metrics provider.
	crate::logger_hook()(&mut builder, &alice_config);

	builder.init().expect("Failed to set up the logger");

	// Start validator Alice.
	let alice = run_validator_node(alice_config, None).await;

	let bob_config =
		node_config(|| {}, tokio::runtime::Handle::current(), Bob, vec![alice.addr.clone()], true);

	// Start validator Bob.
	let _bob = run_validator_node(bob_config, None).await;

	// Wait for Alice to see two finalized blocks.
	alice.wait_for_finalized_blocks(2).await;

	let metrics_uri = format!("http://localhost:{}/metrics", DEFAULT_PROMETHEUS_PORT);
	let metrics = scrape_prometheus_metrics(&metrics_uri).await;

	// There should be at least 1 bitfield processed by now.
	assert!(
		*metrics
			.get(&PARACHAIN_INHERENT_DATA_BITFIELDS_PROCESSED.name.to_owned())
			.unwrap() > 1
	);
}

async fn scrape_prometheus_metrics(metrics_uri: &str) -> HashMap<String, u64> {
	let res = Client::builder(TokioExecutor::new())
		.build_http::<http_body_util::Full<hyper::body::Bytes>>()
		.get(Uri::try_from(metrics_uri).expect("bad URI"))
		.await
		.expect("GET request failed");

	// Retrieve the `HTTP` response body.
	let body = String::from_utf8(
		res.into_body()
			.collect()
			.await
			.expect("can't get body as bytes")
			.to_bytes()
			.to_vec(),
	)
	.expect("body is not an UTF8 string");

	let lines: Vec<_> = body.lines().map(|s| Ok(s.to_owned())).collect();
	prometheus_parse::Scrape::parse(lines.into_iter())
		.expect("Scraper failed to parse Prometheus metrics")
		.samples
		.into_iter()
		.filter_map(|prometheus_parse::Sample { metric, value, .. }| match value {
			prometheus_parse::Value::Counter(value) => Some((metric, value as u64)),
			prometheus_parse::Value::Gauge(value) => Some((metric, value as u64)),
			prometheus_parse::Value::Untyped(value) => Some((metric, value as u64)),
			_ => None,
		})
		.collect()
}
