```
$ ./bench-test 
    Finished test [unoptimized + debuginfo] target(s) in 1.04s
     Running unittests src/lib.rs (/var/lib/buildkite-agent/pdk/target/debug/deps/pallet_poe-485d0292fa61208d)

running 13 tests
test mock::__construct_runtime_integrity_test::runtime_integrity_tests ... ok
test tests::revoke_non_existent_claim_fails ... ok
test tests::transfer_non_existent_claim_fails ... ok
test tests::correct_error_for_none_value ... ok
test tests::create_claim_works ... ok
test tests::revoke_claim_works ... ok
test tests::create_large_claim_fails ... ok
test tests::it_works_for_default_value ... ok
test tests::create_existing_claim_fails ... ok
test tests::revoke_non_claim_owner_fails ... ok
test tests::transfer_claim_works ... ok
test tests::transfer_non_owned_claim_fails ... ok
test benchmarking::benchmarks::benchmark_tests::test_benchmarks ... ok

test result: ok. 13 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s

   Doc-tests pallet-poe

running 0 tests

test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s

```

```
$ ./musl-build 
    Finished release [optimized] target(s) in 1.25s
../../../target/x86_64-unknown-linux-musl/release/node-template: ELF 64-bit LSB executable, x86-64, version 1 (SYSV), statically linked, with debug_info, not stripped
69M     ../../../target/x86_64-unknown-linux-musl/release/node-template
/var/lib/buildkite-agent/pdk/target/x86_64-unknown-linux-musl/release/node-template
```

```
$ ./bench-list 
pallet, benchmark
frame_benchmarking, addition
frame_benchmarking, division
frame_benchmarking, hashing
frame_benchmarking, multiplication
frame_benchmarking, sr25519_verification
frame_benchmarking, subtraction
frame_system, apply_authorized_upgrade
frame_system, authorize_upgrade
frame_system, kill_prefix
frame_system, kill_storage
frame_system, remark
frame_system, remark_with_event
frame_system, set_code
frame_system, set_heap_pages
frame_system, set_storage
pallet_balances, force_adjust_total_issuance
pallet_balances, force_set_balance_creating
pallet_balances, force_set_balance_killing
pallet_balances, force_transfer
pallet_balances, force_unreserve
pallet_balances, transfer_all
pallet_balances, transfer_allow_death
pallet_balances, transfer_keep_alive
pallet_balances, upgrade_accounts
pallet_poe, cause_error
pallet_poe, create_claim
pallet_poe, do_something
pallet_poe, revoke_claim
pallet_poe, transfer_claim
pallet_sudo, remove_key
pallet_sudo, set_key
pallet_sudo, sudo
pallet_sudo, sudo_as
pallet_template, cause_error
pallet_template, do_something
pallet_timestamp, on_finalize
pallet_timestamp, set
```

```
$ ./bench
2024-02-02 19:05:22 Starting benchmark: pallet_poe::do_something    
2024-02-02 19:05:22 Starting benchmark: pallet_poe::cause_error    
2024-02-02 19:05:22 Starting benchmark: pallet_poe::create_claim    
2024-02-02 19:05:22 Starting benchmark: pallet_poe::revoke_claim
```

```
$ ./genspec 
2024-02-02 19:18:30 Building chain spec    
2024-02-02 19:18:30 Building chain spec
```
