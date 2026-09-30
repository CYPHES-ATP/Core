//! Builds and mines a CYPHES genesis block, printing it as hex.
//!
//! ```sh
//! cargo run --release -p zebra-chain --features internal-miner --example cyphes_genesis -- \
//!     <unix_time> <compact_bits_hex> > block.txt
//! ```
//!
//! The genesis block has one transaction: a version 1 coinbase whose script is
//! the launch message and which has no outputs, because genesis pays nothing.
//! Its header commits to that transaction, carries `bits` as the target, and
//! is solved with the BeamHash III reference solver.

use std::sync::Arc;

use chrono::{TimeZone, Utc};

use hex::FromHex;

use zebra_chain::{
    block::{merkle, Block, Header, ZCASH_BLOCK_VERSION},
    parameters::GENESIS_PREVIOUS_BLOCK_HASH,
    serialization::ZcashSerialize,
    transaction::Transaction,
    work::{beamhash::Solution, difficulty::CompactDifficulty},
};

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let [time, bits] = &args[..] else {
        eprintln!("usage: cyphes_genesis <unix_time> <compact_bits_hex>");
        std::process::exit(2);
    };
    let time = Utc
        .timestamp_opt(time.parse().expect("unix time"), 0)
        .single()
        .expect("valid time");
    let bits = CompactDifficulty::from_hex(bits).expect("compact bits hex, e.g. 207fffff");
    bits.to_expanded().expect("bits encode a valid target");

    let coinbase = Transaction::genesis_coinbase();
    let merkle_root: merkle::Root = [coinbase.hash()].into_iter().collect();

    let template = Header {
        version: ZCASH_BLOCK_VERSION,
        previous_block_hash: GENESIS_PREVIOUS_BLOCK_HASH,
        merkle_root,
        commitment_bytes: [0; 32].into(),
        time,
        difficulty_threshold: bits,
        nonce: [0; 8].into(),
        solution: Solution::for_proposal(),
    };

    let started = std::time::Instant::now();
    let header = *Solution::solve(template, || Ok(()))
        .expect("solver is never cancelled")
        .first();

    let block = Block {
        header: Arc::new(header),
        transactions: vec![Arc::new(coinbase)],
    };
    let bytes = block.zcash_serialize_to_vec().expect("block serializes");
    println!("{}", hex::encode(bytes));
    eprintln!(
        "genesis {} (nonce {}, {:.0?})",
        block.hash(),
        hex::encode(header.nonce.0),
        started.elapsed()
    );
}
