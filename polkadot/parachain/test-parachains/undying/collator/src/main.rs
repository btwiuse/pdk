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

//! Collator for the `Undying` test parachain.

use polkadot_cli::{Error, Result};
use polkadot_node_primitives::CollationGenerationConfig;
use polkadot_node_subsystem::messages::{CollationGenerationMessage, CollatorProtocolMessage};
use polkadot_primitives::Id as ParaId;
use sc_cli::{Error as SubstrateCliError, SubstrateCli};
use sp_core::hexdisplay::HexDisplay;
use std::{
	fs,
	io::{self, Write},
};
use test_parachain_undying_collator::Collator;

mod cli;
use cli::{Cli, MalusType};

fn main() -> Result<()> {
	let cli = Cli::from_args();

	match cli.subcommand {
		Some(cli::Subcommand::ExportGenesisState(params)) => {
			// `pov_size`, `pvf_complexity` need to match the
			// ones that we start the collator with.
			let collator = Collator::new(
				params.pov_size,
				params.pvf_complexity,
				// The value of `experimental_send_approved_peer` doesn't matter because it's not
				// part of the state.
				false,
			);

			let output_buf =
				format!("0x{:?}", HexDisplay::from(&collator.genesis_head())).into_bytes();

			if let Some(output) = params.output {
				std::fs::write(output, output_buf)?;
			} else {
				std::io::stdout().write_all(&output_buf)?;
			}

			Ok::<_, Error>(())
		},
		Some(cli::Subcommand::ExportGenesisWasm(params)) => {
			// We pass some dummy values for `pov_size` and `pvf_complexity` as these don't
			// matter for `wasm` export.
			let collator = Collator::default();
			let output_buf =
				format!("0x{:?}", HexDisplay::from(&collator.validation_code())).into_bytes();

			if let Some(output) = params.output {
				fs::write(output, output_buf)?;
			} else {
				io::stdout().write_all(&output_buf)?;
			}

			Ok(())
		},
		None => {
			let runner = cli.create_runner(&cli.run.base).map_err(|e| {
				SubstrateCliError::Application(
					Box::new(e) as Box<(dyn 'static + Send + Sync + std::error::Error)>
				)
			})?;

			runner.run_node_until_exit(|config| async move {
				let collator = Collator::new(
					cli.run.pov_size,
					cli.run.pvf_complexity,
					cli.run.experimental_send_approved_peer,
				);

				let full_node = polkadot_service::build_full(
					config,
					polkadot_service::NewFullParams {
						is_parachain_node: polkadot_service::IsParachainNode::Collator(
							collator.collator_key(),
						),
						enable_beefy: false,
						force_authoring_backoff: false,
						telemetry_worker_handle: None,

						// Collators don't spawn PVF workers, so we can disable version checks.
						node_version: None,
						secure_validator_mode: false,
						workers_path: None,
						workers_names: None,

						overseer_gen: polkadot_service::CollatorOverseerGen,
						overseer_message_channel_capacity_override: None,
						malus_finality_delay: None,
						hwbench: None,
						execute_workers_max_num: None,
						prepare_workers_hard_max_num: None,
						prepare_workers_soft_max_num: None,
						keep_finalized_for: None,
					},
				)
				.map_err(|e| e.to_string())?;
				let mut overseer_handle = full_node
					.overseer_handle
					.clone()
					.expect("Overseer handle should be initialized for collators");

				let genesis_head_hex =
					format!("0x{:?}", HexDisplay::from(&collator.genesis_head()));
				let validation_code_hex =
					format!("0x{:?}", HexDisplay::from(&collator.validation_code()));

				let para_id = ParaId::from(cli.run.parachain_id);

				log::info!("Running `Undying` collator for parachain id: {}", para_id);
				log::info!("Genesis state: {}", genesis_head_hex);
				log::info!("Validation code: {}", validation_code_hex);

				let config = CollationGenerationConfig {
					key: collator.collator_key(),
					// If the collator is malicious, disable the collation function
					// (set to None) and manually handle collation submission later.
					collator: if cli.run.malus_type == MalusType::None {
						Some(
							collator
								.create_collation_function(full_node.task_manager.spawn_handle()),
						)
					} else {
						None
					},
					para_id,
				};
				overseer_handle
					.send_msg(CollationGenerationMessage::Initialize(config), "Collator")
					.await;

				overseer_handle
					.send_msg(CollatorProtocolMessage::CollateOn(para_id), "Collator")
					.await;

				// If the collator is configured to behave maliciously, simulate the specified
				// malicious behavior.
				if cli.run.malus_type == MalusType::DuplicateCollations {
					collator.send_same_collations_to_all_assigned_cores(
						&full_node,
						overseer_handle,
						para_id,
					);
				}

				Ok(full_node.task_manager)
			})
		},
	}?;
	Ok(())
}
