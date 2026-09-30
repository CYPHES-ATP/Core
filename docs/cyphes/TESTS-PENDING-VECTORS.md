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
- `transaction::tests::vectors::fake_v5_librustzcash_round_trip`
- `transaction::tests::vectors::orchard_proof_size_is_canonical`
- `transaction::tests::vectors::v4_transactions_in_one_block_all_round_trip`
- `transaction::tests::vectors::v4_value_balance_check_locates_the_field_with_joinsplits`
- `transparent::tests::vectors::get_transparent_output_address_with_blocks`
- `work::difficulty::tests::vectors::block_difficulty`
- `work::difficulty::tests::vectors::genesis_block_difficulty`
- `work::difficulty::tests::vectors::testnet_minimum_difficulty`
