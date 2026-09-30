//! CYPHES genesis blocks.
//!
//! Each network's genesis block is embedded, so a node can start a chain with
//! no peers. They are produced by `examples/cyphes_genesis.rs`.

use std::sync::Arc;

use hex::FromHex;

use crate::{block::Block, parameters::Network, serialization::ZcashDeserializeInto};

/// The genesis block for `network`.
///
/// # Panics
///
/// If `network` is a configured testnet other than the default testnet or
/// Regtest, which has no embedded genesis block.
pub fn genesis_block(network: &Network) -> Arc<Block> {
    let hex = match network {
        Network::Testnet(params) if params.is_regtest() => {
            include_str!("genesis/block-regtest-0-000-000.txt")
        }
        _ => panic!("no embedded genesis block for {network}"),
    };

    <Vec<u8>>::from_hex(hex.trim())
        .expect("Block bytes are in valid hex representation")
        .zcash_deserialize_into()
        .map(Arc::new)
        .expect("hard-coded genesis block data must deserialize successfully")
}

/// Genesis block for Regtest.
pub fn regtest_genesis_block() -> Arc<Block> {
    genesis_block(&Network::new_regtest(Default::default()))
}
