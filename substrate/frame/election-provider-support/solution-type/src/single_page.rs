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

use crate::{from_assignment_helpers::*, syn_err, vote_field};
use proc_macro2::TokenStream as TokenStream2;
use quote::quote;
use syn::parse::Result;

pub(crate) fn generate(def: crate::SolutionDef) -> Result<TokenStream2> {
	let crate::SolutionDef {
		vis,
		ident,
		count,
		voter_type,
		target_type,
		weight_type,
		max_voters,
		compact_encoding,
	} = def;

	if count <= 2 {
		return Err(syn_err("cannot build solution struct with capacity less than 3."))
	}

	let single = {
		let name = vote_field(1);
		// NOTE: we use the visibility of the struct for the fields as well.. could be made better.
		quote!(
			#vis #name: _fepsp::Vec<(#voter_type, #target_type)>,
		)
	};

	let rest = (2..=count)
		.map(|c| {
			let field_name = vote_field(c);
			let array_len = c - 1;
			quote!(
				#vis #field_name: _fepsp::Vec<(
					#voter_type,
					[(#target_type, #weight_type); #array_len],
					#target_type
				)>,
			)
		})
		.collect::<TokenStream2>();

	let len_impl = len_impl(count);
	let edge_count_impl = edge_count_impl(count);
	let unique_targets_impl = unique_targets_impl(count);
	let remove_voter_impl = remove_voter_impl(count);

	let derives_and_maybe_compact_encoding = if compact_encoding {
		// custom compact encoding.
		let compact_impl = crate::codec::codec_and_info_impl(
			ident.clone(),
			voter_type.clone(),
			target_type.clone(),
			weight_type.clone(),
			count,
		);
		quote! {
			#compact_impl
			#[derive(Default, PartialEq, Eq, Clone, Debug, PartialOrd, Ord, _fepsp::codec::DecodeWithMemTracking)]
		}
	} else {
		// automatically derived.
		quote!(#[derive(
			Default,
			PartialEq,
			Eq,
			Clone,
			Debug,
			Ord,
			PartialOrd,
			_fepsp::codec::Encode,
			_fepsp::codec::Decode,
			_fepsp::codec::DecodeWithMemTracking,
			_fepsp::scale_info::TypeInfo,
		)])
	};

	let struct_name = syn::Ident::new("solution", proc_macro2::Span::call_site());
	let assignment_name = syn::Ident::new("all_assignments", proc_macro2::Span::call_site());

	let from_impl = from_impl(&struct_name, count);
	let into_impl = into_impl(&assignment_name, count, weight_type.clone());
	let from_index_impl = crate::index_assignment::from_impl(&struct_name, count);
	let sort_impl = sort_impl(count);
	let remove_weakest_sorted_impl = remove_weakest_sorted_impl(count);

	Ok(quote! (
		/// A struct to encode a election assignment in a compact way.
		#derives_and_maybe_compact_encoding
		#vis struct #ident { #single #rest }

		use _fepsp::__OrInvalidIndex;
		impl _feps::NposSolution for #ident {
			const LIMIT: usize = #count;
			type VoterIndex = #voter_type;
			type TargetIndex = #target_type;
			type Accuracy = #weight_type;

			fn remove_voter(&mut self, to_remove: Self::VoterIndex) -> bool {
				#remove_voter_impl
				return false
			}

			fn from_assignment<FV, FT, A>(
				assignments: &[_feps::Assignment<A, #weight_type>],
				voter_index: FV,
				target_index: FT,
			) -> Result<Self, _feps::Error>
				where
					A: _feps::IdentifierT,
					for<'r> FV: Fn(&'r A) -> Option<Self::VoterIndex>,
					for<'r> FT: Fn(&'r A) -> Option<Self::TargetIndex>,
			{
				// Make sure that the voter bound is binding.
				// `assignments.len()` actually represents the number of voters
				if assignments.len() as u32 > <#max_voters as _feps::Get<u32>>::get() {
					return Err(_feps::Error::TooManyVoters);
				}
				let mut #struct_name: #ident = Default::default();
				for _feps::Assignment { who, distribution } in assignments {
					match distribution.len() {
						0 => continue,
						#from_impl
						_ => {
							return Err(_feps::Error::SolutionTargetOverflow);
						}
					}
				};

				Ok(#struct_name)
			}

			fn into_assignment<A: _feps::IdentifierT>(
				self,
				voter_at: impl Fn(Self::VoterIndex) -> Option<A>,
				target_at: impl Fn(Self::TargetIndex) -> Option<A>,
			) -> Result<_fepsp::Vec<_feps::Assignment<A, #weight_type>>, _feps::Error> {
				let mut #assignment_name: _fepsp::BTreeMap<Self::VoterIndex, _feps::Assignment<A, #weight_type>> = Default::default();
				#into_impl
				Ok(#assignment_name.into_values().collect())
			}

			fn voter_count(&self) -> usize {
				let mut all_len = 0usize;
				#len_impl
				all_len
			}

			fn edge_count(&self) -> usize {
				let mut all_edges = 0usize;
				#edge_count_impl
				all_edges
			}

			fn unique_targets(&self) -> _fepsp::Vec<Self::TargetIndex> {
				// NOTE: this implementation returns the targets sorted, but we don't use it yet per
				// se, nor is the API enforcing it.
				use _fepsp::BTreeSet;
				let mut all_targets: BTreeSet<Self::TargetIndex> = BTreeSet::new();
				let mut maybe_insert_target = |t: Self::TargetIndex| {
					all_targets.insert(t);
				};

				#unique_targets_impl

				all_targets.into_iter().collect()
			}

			fn sort<F>(&mut self, mut voter_stake: F)
			where
				F: FnMut(&Self::VoterIndex) -> _feps::VoteWeight
			{
				#sort_impl
			}

			fn remove_weakest_sorted<F>(&mut self, mut voter_stake: F) -> Option<Self::VoterIndex>
			where
				F: FnMut(&Self::VoterIndex) -> _feps::VoteWeight
			{
				#remove_weakest_sorted_impl
			}

			fn corrupt(&mut self) {
				self.votes1.push(
					(
						_fepsp::sp_arithmetic::traits::Bounded::max_value(),
						_fepsp::sp_arithmetic::traits::Bounded::max_value()
					)
				)
			}
		}

		type __IndexAssignment = _feps::IndexAssignment<
			<#ident as _feps::NposSolution>::VoterIndex,
			<#ident as _feps::NposSolution>::TargetIndex,
			<#ident as _feps::NposSolution>::Accuracy,
		>;

		impl _fepsp::codec::MaxEncodedLen for #ident {
			fn max_encoded_len() -> usize {
				use frame_support::traits::Get;
				use _fepsp::codec::Encode;
				let s: u32 = <#max_voters as _feps::Get<u32>>::get();
				let max_element_size =
					// the first voter..
					#voter_type::max_encoded_len()
					// #count - 1 tuples..
					.saturating_add(
						(#count - 1).saturating_mul(
							#target_type::max_encoded_len().saturating_add(#weight_type::max_encoded_len())))
					// and the last target.
					.saturating_add(#target_type::max_encoded_len());
				// The assumption is that it contains #count-1 empty elements
				// and then last element with full size
				#count
					.saturating_mul(_fepsp::codec::Compact(0u32).encoded_size())
					.saturating_add((s as usize).saturating_mul(max_element_size))
			}
		}

		impl<'a> core::convert::TryFrom<&'a [__IndexAssignment]> for #ident {
			type Error = _feps::Error;
			fn try_from(index_assignments: &'a [__IndexAssignment]) -> Result<Self, Self::Error> {
				let mut #struct_name =  #ident::default();

				for _feps::IndexAssignment { who, distribution } in index_assignments {
					match distribution.len() {
						0 => {}
						#from_index_impl
						_ => {
							return Err(_feps::Error::SolutionTargetOverflow);
						}
					}
				};

				Ok(#struct_name)
			}
		}
	))
}

fn sort_impl(count: usize) -> TokenStream2 {
	(1..=count)
		.map(|c| {
			let field = vote_field(c);
			quote! {
				// NOTE: self.filed here is sometimes `Vec<(voter, weight)>` and sometimes
				// `Vec<(voter, weights, last_weight)>`, but Rust's great patter matching makes it
				// all work super nice.
				self.#field.sort_by(|(a, ..), (b, ..)| voter_stake(&b).cmp(&voter_stake(&a)));
				// ---------------------------------^^ in all fields, the index 0 is the voter id.
			}
		})
		.collect::<TokenStream2>()
}

fn remove_weakest_sorted_impl(count: usize) -> TokenStream2 {
	// check minium from field 2 onwards. We assume 0 is minimum
	let check_minimum = (2..=count).map(|c| {
		let filed = vote_field(c);
		quote! {
			let filed_value = self.#filed
				.last()
				.map(|(x, ..)| voter_stake(x))
				.unwrap_or_else(|| _fepsp::sp_arithmetic::traits::Bounded::max_value());
			if filed_value < minimum {
				minimum = filed_value;
				minimum_filed = #c
			}
		}
	});

	let remove_minimum_match = (1..=count).map(|c| {
		let filed = vote_field(c);
		quote! {
			#c => self.#filed.pop().map(|(x, ..)| x),
		}
	});

	let first_filed = vote_field(1);
	quote! {
		// we assume first one is the minimum. No problem if it is empty.
		let mut minimum_filed = 1;
		let mut minimum = self.#first_filed
			.last()
			.map(|(x, ..)| voter_stake(x))
			.unwrap_or_else(|| _fepsp::sp_arithmetic::traits::Bounded::max_value());

		#( #check_minimum )*

		match minimum_filed {
			#( #remove_minimum_match )*
			_ => {
				debug_assert!(false);
				None
			}
		}
	}
}

fn remove_voter_impl(count: usize) -> TokenStream2 {
	let field_name = vote_field(1);
	let single = quote! {
		if let Some(idx) = self.#field_name.iter().position(|(x, _)| *x == to_remove) {
			self.#field_name.remove(idx);
			return true
		}
	};

	let rest = (2..=count)
		.map(|c| {
			let field_name = vote_field(c);
			quote! {
				if let Some(idx) = self.#field_name.iter().position(|(x, _, _)| *x == to_remove) {
					self.#field_name.remove(idx);
					return true
				}
			}
		})
		.collect::<TokenStream2>();

	quote! {
		#single
		#rest
	}
}

fn len_impl(count: usize) -> TokenStream2 {
	(1..=count)
		.map(|c| {
			let field_name = vote_field(c);
			quote!(
				all_len = all_len.saturating_add(self.#field_name.len());
			)
		})
		.collect::<TokenStream2>()
}

fn edge_count_impl(count: usize) -> TokenStream2 {
	(1..=count)
		.map(|c| {
			let field_name = vote_field(c);
			quote!(
				all_edges = all_edges.saturating_add(
					self.#field_name.len().saturating_mul(#c as usize)
				);
			)
		})
		.collect::<TokenStream2>()
}

fn unique_targets_impl(count: usize) -> TokenStream2 {
	let unique_targets_impl_single = {
		let field_name = vote_field(1);
		quote! {
			self.#field_name.iter().for_each(|(_, t)| {
				maybe_insert_target(*t);
			});
		}
	};

	let unique_targets_impl_rest = (2..=count)
		.map(|c| {
			let field_name = vote_field(c);
			quote! {
				self.#field_name.iter().for_each(|(_, inners, t_last)| {
					inners.iter().for_each(|(t, _)| {
						maybe_insert_target(*t);
					});
					maybe_insert_target(*t_last);
				});
			}
		})
		.collect::<TokenStream2>();

	quote! {
		#unique_targets_impl_single
		#unique_targets_impl_rest
	}
}

pub(crate) fn from_impl(struct_name: &syn::Ident, count: usize) -> TokenStream2 {
	let from_impl_single = {
		let field = vote_field(1);
		let push_code = from_impl_single_push_code();
		quote!(1 => #struct_name.#field.#push_code,)
	};

	let from_impl_rest = (2..=count)
		.map(|c| {
			let field = vote_field(c);
			let push_code = from_impl_rest_push_code(c);
			quote!(#c => #struct_name.#field.#push_code,)
		})
		.collect::<TokenStream2>();

	quote!(
		#from_impl_single
		#from_impl_rest
	)
}

pub(crate) fn into_impl(
	assignments: &syn::Ident,
	count: usize,
	per_thing: syn::Type,
) -> TokenStream2 {
	let into_impl_single = {
		let name = vote_field(1);
		quote!(
			for (voter_index, target_index) in self.#name {;
				if #assignments.contains_key(&voter_index) {
					return Err(_feps::Error::DuplicateVoter);
				} else {
					#assignments.insert(
						voter_index,
						_feps::Assignment {
							who: voter_at(voter_index).or_invalid_index()?,
							distribution: vec![(target_at(target_index).or_invalid_index()?, #per_thing::one())],
						}
					);
				}
			}
		)
	};

	let into_impl_rest = (2..=count)
		.map(|c| {
			let name = vote_field(c);
			quote!(
				for (voter_index, inners, t_last_idx) in self.#name {
					if #assignments.contains_key(&voter_index) {
						return Err(_feps::Error::DuplicateVoter);
					}

					let mut targets_seen = _fepsp::BTreeSet::new();

					let mut sum = #per_thing::zero();
					let mut inners_parsed = inners
						.iter()
						.map(|(ref t_idx, p)| {
							if targets_seen.contains(t_idx) {
								return Err(_feps::Error::DuplicateTarget);
							} else {
								targets_seen.insert(t_idx);
							}
							sum = _fepsp::sp_arithmetic::traits::Saturating::saturating_add(sum, *p);
							let target = target_at(*t_idx).or_invalid_index()?;
							Ok((target, *p))
						})
						.collect::<Result<_fepsp::Vec<(A, #per_thing)>, _feps::Error>>()?;

					if sum >= #per_thing::one() {
						return Err(_feps::Error::SolutionWeightOverflow);
					}

					// check that the last target index is also unique.
					if targets_seen.contains(&t_last_idx) {
						return Err(_feps::Error::DuplicateTarget);
					} else {
						// no need to insert, we are done.
					}

					// defensive only. Since Percent doesn't have `Sub`.
					let p_last = _fepsp::sp_arithmetic::traits::Saturating::saturating_sub(
						#per_thing::one(),
						sum,
					);

					inners_parsed.push((target_at(t_last_idx).or_invalid_index()?, p_last));

					#assignments.insert(
						voter_index,
						_feps::Assignment {
							who: voter_at(voter_index).or_invalid_index()?,
							distribution: inners_parsed,
						}
					);
				}
			)
		})
		.collect::<TokenStream2>();

	quote!(
		#into_impl_single
		#into_impl_rest
	)
}
