//! CYPHES genesis blocks.
//!
//! Each network's genesis block is embedded, so a node can start a chain with
//! no peers. They are produced by `examples/cyphes_genesis.rs`.

use std::sync::Arc;

use hex::FromHex;

use crate::{block::Block, parameters::Network, serialization::ZcashDeserializeInto};

/// The genesis block for `network`.
///
/// The mainnet genesis block is PROVISIONAL: it will be mined again at
/// launch, at the launch difficulty estimate, with a coinbase message that
/// commits to a recent Bitcoin block.
///
/// # Panics
///
/// If `network` is a configured testnet other than the default testnet or
/// Regtest, which has no embedded genesis block.
pub fn genesis_block(network: &Network) -> Arc<Block> {
    let hex = match network {
        Network::Mainnet => include_str!("genesis/block-mainnet-0-000-000.txt"),
        Network::Testnet(params) if params.is_regtest() => {
            include_str!("genesis/block-regtest-0-000-000.txt")
        }
        Network::Testnet(params) if params.is_default_testnet() => {
            include_str!("genesis/block-testnet-0-000-000.txt")
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parameters::testnet;

    #[test]
    fn embedded_genesis_blocks_match_network_genesis_hashes() {
        let _init_guard = zebra_test::init();

        let networks = [
            Network::Mainnet,
            Network::new_default_testnet(),
            Network::new_regtest(testnet::RegtestParameters::default()),
        ];
        for network in networks {
            let block = genesis_block(&network);
            assert_eq!(block.hash(), network.genesis_hash(), "{network}");
            assert_eq!(
                network.checkpoint_list().hash(crate::block::Height(0)),
                Some(block.hash())
            );

            // Each genesis is a valid BeamHash III block that pays nothing.
            let header = &block.header;
            header.solution.check(header).expect("valid solution");
            let threshold = header
                .difficulty_threshold
                .to_expanded()
                .expect("valid bits");
            assert!(header.solution.meets_threshold(threshold));
            assert_eq!(block.transactions.len(), 1);
            assert!(block.transactions[0].outputs().is_empty());
        }

        assert_ne!(
            genesis_block(&Network::Mainnet).hash(),
            genesis_block(&Network::new_default_testnet()).hash()
        );
    }
}
