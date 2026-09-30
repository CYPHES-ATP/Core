# Zebra tests pending CYPHES vectors

These zebra-chain tests deserialize real Zcash block vectors from
`zebra-test`. Zcash blocks cannot parse under the CYPHES header (220-byte
BeamHash III header instead of Zcash's 1487-byte Equihash header), so they
fail until CYPHES block vectors replace them. Tests for deleted features
(Sprout, v4 JoinSplits, transparent addresses, Zcash checkpoints and
upgrade history) are deleted rather than ported.

## Parse Zcash block vectors

- `block::merkle::tests::auth_data_pre_v5`
- `block::merkle::tests::auth_digest`
- `block::merkle::tests::block_test_vectors`
- `block::tests::vectors::block_commitment`
- `block::tests::vectors::block_test_vectors`
- `block::tests::vectors::block_test_vectors_unique`
- `block::tests::vectors::blockheaderhash_from_blockheader`
- `block::tests::vectors::round_trip_blocks`
- `history_tree::tests::vectors::push_and_prune`
- `history_tree::tests::vectors::upgrade`
- `parameters::checkpoint::list::tests::checkpoint_list_duplicate_blocks_fail`
- `parameters::checkpoint::list::tests::checkpoint_list_duplicate_hashes_fail`
- `parameters::checkpoint::list::tests::checkpoint_list_duplicate_heights_fail`
- `parameters::checkpoint::list::tests::checkpoint_list_genesis`
- `parameters::checkpoint::list::tests::checkpoint_list_multiple`
- `parameters::checkpoint::list::tests::checkpoint_list_no_genesis_fail`
- `primitives::zcash_history::tests::vectors::tree`
- `sprout::tests::tree::incremental_roots_with_blocks`
- `transaction::tests::vectors::binding_signatures`
- `transaction::tests::vectors::coinbase_script_len_bounds_enforced_at_parse`
- `transaction::tests::vectors::coinbase_v5_with_sapling_spends_deserializes_successfully`
- `transaction::tests::vectors::consensus_branch_id`
- `transaction::tests::vectors::orchard_proof_size_is_canonical`
- `transaction::tests::vectors::v4_transactions_in_one_block_all_round_trip`
- `transaction::tests::vectors::v4_value_balance_check_locates_the_field_with_joinsplits`
- `transparent::tests::vectors::get_transparent_output_address_with_blocks`
- `work::difficulty::tests::vectors::block_difficulty`
- `work::difficulty::tests::vectors::genesis_block_difficulty`
- `work::difficulty::tests::vectors::testnet_minimum_difficulty`

## Assert Zcash parameters that CYPHES changed on purpose

These check Zcash's subsidy and halving schedule, magic bytes, funding
streams, lockbox, the temporary Orchard soft fork, the multi-upgrade
activation list, `MAX_MONEY` overflow at 21 million, transparent `t1…` /
`tm…` address encodings (the librustzcash fork changes every Zcash prefix),
or the 2-hour future time limit. Rewrite them for CYPHES values, or delete
them with the feature:

- `amount::tests::vectors::test_sum`
- `block::tests::vectors::time_check_now`
- `parameters::network::magic::magic_proptest::magic_debug`
- `parameters::network::tests::block_subsidy_test`
- `parameters::network::tests::check_height_for_num_halvings`
- `parameters::network::tests::halving_test`
- `parameters::network::tests::vectors::check_configured_funding_stream_constraints`
- `parameters::network::tests::vectors::check_configured_funding_stream_regtest`
- `parameters::network::tests::vectors::funding_streams_default_values`
- `parameters::network::tests::vectors::sum_of_one_time_lockbox_disbursements_is_correct`
- `parameters::network::tests::vectors::temporary_orchard_disabling_soft_fork_heights`
- `parameters::tests::activation_extremes_mainnet`
- `parameters::tests::activation_extremes_testnet`
- `parameters::tests::branch_id_consistent_mainnet`
- `parameters::tests::branch_id_consistent_testnet`
- `transparent::address::tests::debug`
- `transparent::address::tests::empty_script_mainnet`
- `transparent::address::tests::empty_script_testnet`
- `transparent::address::tests::from_string`
- `transparent::address::tests::pubkey_mainnet`
- `transparent::address::tests::pubkey_testnet`
- `transparent::tests::vectors::get_transparent_output_address`

## Totals

zebra-chain library tests after the librustzcash fork: 229 pass,
51 fail (29 Zcash-vector tests, 22 Zcash-parameter tests). The
16 tests that were blocked on librustzcash's 21 million `MAX_MONEY` pass
since the fork. Every CYPHES-specific test passes: `cyphes-pow`,
`cyphes-params`, `zcash_protocol` (fork), `zebra-chain` `work::tests`,
`block::genesis` and `parameters::cyphes_consistency`, `zebra-consensus`
`ironwood_only`, `zebra-state` `lwma_tests` and `cyphes_isolation_tests`,
`zebra-network` `cyphes_isolation_tests`, the `zebra-state` backup tests
(the upstream round trip now uses CYPHES block 1), `cyphes-wallet` and
`cyphes-stratum`.

Other crates' suites have not been triaged yet. In `zebra-consensus`, for
example, tests built on Sprout or transparent transactions now fail on
`ironwood_only` (`transaction::tests::state_error_converted_correctly`,
`mempool_zip317_error`), and the script tests fail to parse Zcash blocks.
