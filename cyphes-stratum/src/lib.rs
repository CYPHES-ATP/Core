//! Stratum bridge between a CYPHES node and BeamHash III GPU miners.
//!
//! The node builds block templates (`getblocktemplate`) and accepts blocks
//! (`submitblock`); GPU miners such as lolMiner and GMiner speak Beam's
//! stratum protocol. The bridge turns each template into a stratum job whose
//! `input` is the header's PoW input, checks every solution miners send
//! (BeamHash III, share difficulty, repeats), and submits the ones that meet
//! the block target. Blocks pay the node's configured `mining.miner_address`.

pub mod job;
pub mod node;
pub mod protocol;
pub mod server;
