//! CYPHES block test vectors.
//!
//! Blocks 1 to 3 of a CYPHES regtest chain, mined by zebrad's internal
//! BeamHash III miner on top of the embedded regtest genesis block. Each
//! coinbase pays 1,000 CASH to a single Ironwood output, under the CYPHES branch
//! ID and address prefixes (librustzcash fork), with proof of work enforced.

use hex::FromHex;
use lazy_static::lazy_static;

lazy_static! {
    /// Regtest blocks 1, 2 and 3, in height order.
    pub static ref CYPHES_REGTEST_BLOCKS: Vec<Vec<u8>> = [
        include_str!("cyphes/block-regtest-0-000-001.txt"),
        include_str!("cyphes/block-regtest-0-000-002.txt"),
        include_str!("cyphes/block-regtest-0-000-003.txt"),
    ]
    .iter()
    .map(|hex| <Vec<u8>>::from_hex(hex.trim()).expect("block vector is hex"))
    .collect();
}
