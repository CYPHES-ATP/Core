//! Tests for the CYPHES rule that transactions only use the Ironwood pool.

use zebra_chain::{
    amount::Amount,
    block::Height,
    parameters::NetworkUpgrade,
    transaction::{LockTime, Transaction},
    transparent,
};

use crate::{error::TransactionError, transaction::check::ironwood_only};

fn spend() -> transparent::Input {
    transparent::Input::PrevOut {
        outpoint: transparent::OutPoint {
            hash: zebra_chain::transaction::Hash([1; 32]),
            index: 0,
        },
        unlock_script: transparent::Script::new(&[0x51]),
        sequence: u32::MAX,
    }
}

fn coinbase(height: u32) -> transparent::Input {
    transparent::Input::Coinbase {
        height: Height(height),
        data: vec![0; 4],
        sequence: u32::MAX,
    }
}

fn output() -> transparent::Output {
    transparent::Output {
        value: Amount::try_from(1_000).expect("valid amount"),
        lock_script: transparent::Script::new(&[0x51]),
    }
}

fn v6(inputs: Vec<transparent::Input>, outputs: Vec<transparent::Output>) -> Transaction {
    Transaction::test_v6(
        NetworkUpgrade::Nu6_3,
        inputs,
        outputs,
        LockTime::unlocked(),
        Height(10),
    )
}

fn rejection(tx: &Transaction, height: u32) -> Option<String> {
    match ironwood_only(tx, Height(height)) {
        Ok(()) => None,
        Err(TransactionError::NotIronwoodOnly(reason)) => Some(reason),
        Err(other) => panic!("unexpected error {other:?}"),
    }
}

#[test]
fn transparent_outputs_are_rejected_in_every_transaction() {
    let _init_guard = zebra_test::init();

    // A miner paying itself transparently, the Zcash-style coinbase.
    let transparent_coinbase = v6(vec![coinbase(5)], vec![output()]);
    assert!(transparent_coinbase.is_coinbase());
    assert_eq!(
        rejection(&transparent_coinbase, 5),
        Some(String::from("transparent outputs are not allowed"))
    );

    let transparent_spend = v6(vec![spend()], vec![output()]);
    assert_eq!(
        rejection(&transparent_spend, 5),
        Some(String::from("transparent outputs are not allowed"))
    );
}

#[test]
fn transparent_spends_are_rejected() {
    let _init_guard = zebra_test::init();

    let tx = v6(vec![spend()], Vec::new());
    assert_eq!(
        rejection(&tx, 5),
        Some(String::from("transparent inputs are not allowed"))
    );
}

#[test]
fn coinbase_height_input_is_allowed() {
    let _init_guard = zebra_test::init();

    // The coinbase input carries the height and no value; with no transparent
    // outputs the rule itself has nothing to reject. (A real coinbase also has
    // an Ironwood output, and other rules require outputs.)
    let tx = v6(vec![coinbase(5)], Vec::new());
    assert_eq!(rejection(&tx, 5), None);
}

#[test]
fn only_version_6_is_allowed_after_genesis() {
    let _init_guard = zebra_test::init();

    let v4 = Transaction::test_v4(
        vec![coinbase(5)],
        Vec::new(),
        LockTime::unlocked(),
        Height(10),
    );
    assert_eq!(
        rejection(&v4, 5),
        Some(String::from("transaction version must be 6"))
    );

    let v1 = Transaction::test_v1(vec![coinbase(5)], vec![output()], LockTime::unlocked());
    assert_eq!(
        rejection(&v1, 5),
        Some(String::from("transaction version must be 6"))
    );
}

#[test]
fn genesis_is_exempt() {
    let _init_guard = zebra_test::init();

    // The genesis coinbase is version 1 and checkpointed.
    assert_eq!(rejection(&Transaction::genesis_coinbase(), 0), None);
    assert_eq!(
        rejection(&Transaction::genesis_coinbase(), 1),
        Some(String::from("transaction version must be 6"))
    );
}

#[test]
fn mined_cyphes_blocks_are_ironwood_only() {
    use std::sync::Arc;

    use zebra_chain::{
        block::Block,
        serialization::{ZcashDeserializeInto, ZcashSerialize},
    };

    let _init_guard = zebra_test::init();

    let mut previous = zebra_chain::block::genesis::regtest_genesis_block().hash();
    for (i, bytes) in zebra_test::vectors::CYPHES_REGTEST_BLOCKS
        .iter()
        .enumerate()
    {
        let height = u32::try_from(i + 1).expect("small height");
        let block: Arc<Block> = bytes.zcash_deserialize_into().expect("vector deserializes");
        assert_eq!(&block.zcash_serialize_to_vec().expect("serializes"), bytes);
        assert_eq!(
            block.header.previous_block_hash, previous,
            "block {height} extends the chain"
        );
        assert_eq!(block.coinbase_height(), Some(Height(height)));
        block
            .header
            .solution
            .check(&block.header)
            .expect("valid BeamHash III solution");

        let coinbase = &block.transactions[0];
        assert_eq!(rejection(coinbase, height), None);
        assert!(coinbase.has_ironwood_shielded_data());
        assert!(coinbase.outputs().is_empty());

        previous = block.hash();
    }
}

#[test]
fn zcash_branch_id_transactions_do_not_parse() {
    use zebra_chain::{block::Block, serialization::ZcashDeserializeInto};

    let _init_guard = zebra_test::init();

    // A real CYPHES block: 220-byte header, 1-byte transaction count, then the
    // v6 coinbase: header, version group ID, consensus branch ID.
    let mut bytes = zebra_test::vectors::CYPHES_REGTEST_BLOCKS[0].clone();
    let branch_id_at = 220 + 1 + 4 + 4;
    assert_eq!(
        bytes[branch_id_at..branch_id_at + 4],
        cyphes_params::network::CONSENSUS_BRANCH_ID_V1.to_le_bytes()
    );
    assert!(bytes.zcash_deserialize_into::<Block>().is_ok());

    // Zcash's NU6.3 branch ID: the same transaction is not a CYPHES transaction.
    bytes[branch_id_at..branch_id_at + 4].copy_from_slice(&0x37a5_165bu32.to_le_bytes());
    assert!(bytes.zcash_deserialize_into::<Block>().is_err());
}
