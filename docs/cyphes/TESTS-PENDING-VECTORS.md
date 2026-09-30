# Zebra tests pending CYPHES vectors

These zebra-chain tests deserialize real Zcash block vectors from
`zebra-test`. Zcash blocks cannot parse under the CYPHES header (220-byte
BeamHash III header instead of Zcash's 1487-byte Equihash header), so they
fail until CYPHES block vectors replace them. Tests for deleted features
(Sprout, v4 JoinSplits, transparent addresses, Zcash checkpoints and
upgrade history) are deleted rather than ported.

Recorded at the BeamHash III header change (246 passing, 30 below):

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
- ~~`transaction::tests::vectors::fake_v5_librustzcash_round_trip`~~ (passes again)
- `transaction::tests::vectors::orchard_proof_size_is_canonical`
- `transaction::tests::vectors::v4_transactions_in_one_block_all_round_trip`
- `transaction::tests::vectors::v4_value_balance_check_locates_the_field_with_joinsplits`
- `transparent::tests::vectors::get_transparent_output_address_with_blocks`
- `work::difficulty::tests::vectors::block_difficulty`
- `work::difficulty::tests::vectors::genesis_block_difficulty`
- `work::difficulty::tests::vectors::testnet_minimum_difficulty`

## Blocked on the librustzcash fork

Raising `MAX_MONEY` to 10 billion CASH in Zebra (needed so the chain does not
halt when the Ironwood pool passes 21 million) lets proptest strategies
generate amounts that librustzcash's `Zatoshis` still rejects, because its
`MAX_MONEY` is 21 million. They fail with "Zebra Amount<NonNegative> is always
a valid Zatoshis: Overflow" (or, for Sprout, "vpub_new out of range") until the
`zcash_protocol` fork raises it too:

- `block::tests::prop::arbitrary_height_partial_chain_strategy`
- `block::tests::prop::block_genesis_strategy`
- `block::tests::prop::block_roundtrip`
- `block::tests::prop::blocks_have_coinbase`
- `block::tests::prop::genesis_partial_chain_strategy`
- `block::tests::vectors::chain_value_pool_change_propagates_transaction_value_balance_errors`
- `serialization::tests::prop::transaction_serialized_size`
- `transaction::tests::preallocate::tx_max_allocation_is_big_enough`
- `transaction::tests::preallocate::tx_size_is_small_enough`
- `transaction::tests::prop::arbitrary_transaction_version_strategy`
- `transaction::tests::prop::arbitrary_transaction_versions_cover_each_era`
- `transaction::tests::prop::transaction_roundtrip`
- `transaction::tests::prop::transaction_roundtrip_nu6_3`
- `transaction::tests::prop::transaction_valid_network_upgrade_strategy`
- `transaction::tests::vectors::sprout_aggregate_value_balance_out_of_range_is_rejected`

Production code converts amounts with fallible `TryFrom`, so this is a
test-strategy failure, not a reachable panic.

## Assert Zcash parameters that CYPHES changed on purpose

These check Zcash's subsidy and halving schedule, magic bytes, funding
streams, lockbox, the temporary Orchard soft fork, the multi-upgrade
activation list, `MAX_MONEY` overflow at 21 million, or the 2-hour future time
limit. Rewrite them for CYPHES values, or delete them with the feature:

- `amount::tests::vectors::test_sum`
- `block::tests::vectors::time_check_now`
- `parameters::network::magic::magic_proptest::magic_debug`
- `parameters::network::tests::block_subsidy_test`
- `parameters::network::tests::check_height_for_num_halvings`
- `parameters::network::tests::halving_test`
- `parameters::network::tests::vectors::check_configured_funding_stream_constraints`
- `parameters::network::tests::vectors::check_configured_funding_stream_regtest`
- `parameters::network::tests::vectors::check_parameters_impl`
- `parameters::network::tests::vectors::funding_streams_default_values`
- `parameters::network::tests::vectors::sum_of_one_time_lockbox_disbursements_is_correct`
- `parameters::network::tests::vectors::temporary_orchard_disabling_soft_fork_heights`
- `parameters::tests::activation_extremes_mainnet`
- `parameters::tests::activation_extremes_testnet`
- `parameters::tests::branch_id_consistent_mainnet`
- `parameters::tests::branch_id_consistent_testnet`

## Totals

zebra-chain library tests after the LWMA commit: 217 pass, 60 fail (the 29
remaining Zcash-vector tests above, plus these 31). Every CYPHES-specific test
passes: `cyphes-pow`, `cyphes-params`, `zebra-chain` `work::tests` and
`block::genesis`, `zebra-consensus` `ironwood_only`, `zebra-state`
`lwma_tests`. Other crates' suites have not been triaged yet.
