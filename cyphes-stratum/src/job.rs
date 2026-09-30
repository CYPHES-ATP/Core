//! Jobs: block templates turned into stratum work, and the checks each
//! solution must pass before it counts as a share or goes to the node.

use std::{
    collections::HashSet,
    sync::{Arc, Mutex},
};

use hex::FromHex;
use serde::Deserialize;
use zebra_chain::{
    block::{self, merkle, Block, ChainHistoryBlockTxAuthCommitmentHash, Header},
    serialization::{BytesInDisplayOrder, DateTime32, ZcashDeserializeInto, ZcashSerialize},
    work::{
        beamhash::Solution,
        difficulty::{CompactDifficulty, ExpandedDifficulty, U256},
    },
};

use cyphes_pow::BeamDifficulty;

/// The fields of the node's `getblocktemplate` response the bridge uses.
#[derive(Clone, Debug, Deserialize)]
pub struct Template {
    pub version: u32,
    #[serde(rename = "previousblockhash")]
    pub previous_block_hash: String,
    #[serde(rename = "defaultroots")]
    pub default_roots: DefaultRoots,
    #[serde(rename = "coinbasetxn")]
    pub coinbase_txn: TransactionTemplate,
    pub transactions: Vec<TransactionTemplate>,
    #[serde(rename = "longpollid")]
    pub long_poll_id: String,
    #[serde(rename = "curtime")]
    pub cur_time: u32,
    pub bits: String,
    pub height: u32,
    /// `false` when work on earlier templates can no longer make a block.
    #[serde(rename = "submitold", default)]
    pub submit_old: Option<bool>,
}

#[derive(Clone, Debug, Deserialize)]
pub struct DefaultRoots {
    #[serde(rename = "merkleroot")]
    pub merkle_root: String,
    #[serde(rename = "blockcommitmentshash")]
    pub block_commitments_hash: String,
}

#[derive(Clone, Debug, Deserialize)]
pub struct TransactionTemplate {
    pub data: String,
}

/// A template the bridge cannot turn into a job.
#[derive(Debug, thiserror::Error)]
#[error("invalid block template: {0}")]
pub struct TemplateError(String);

fn field<T: FromHex>(name: &str, hex: &str) -> Result<T, TemplateError> {
    T::from_hex(hex).map_err(|_| TemplateError(format!("bad {name}")))
}

/// What a solution turned out to be.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Verdict {
    /// Not a valid solution for the job, below the share difficulty, or a
    /// repeat.
    Rejected(&'static str),
    /// A valid share that is not a block.
    Share,
    /// A block: submit it to the node.
    Block(Box<Block>),
}

/// Stratum work for one block template.
#[derive(Debug)]
pub struct Job {
    pub id: String,
    pub height: u32,
    /// The block this job builds on.
    pub previous_block_hash: block::Hash,
    /// The header with an empty nonce and solution.
    header: Header,
    /// Serialized transactions, coinbase first.
    transactions: Vec<Vec<u8>>,
    /// `BLAKE2b(header prefix)`: what miners hash with their nonce.
    pub input: [u8; 32],
    /// The consensus target for a block.
    pub block_target: ExpandedDifficulty,
    /// The difficulty miners are asked for, never harder than a block.
    pub share_difficulty: BeamDifficulty,
    /// Solutions already seen for this job, to refuse repeats.
    seen: Mutex<HashSet<[u8; 32]>>,
}

impl Job {
    /// Builds a job from a node template. `share_difficulty` is the raw Beam
    /// difficulty to ask miners for (`None` asks for blocks only).
    pub fn from_template(
        id: String,
        template: &Template,
        share_difficulty: Option<u64>,
    ) -> Result<Job, TemplateError> {
        let header = Header {
            version: template.version,
            previous_block_hash: field("previousblockhash", &template.previous_block_hash)?,
            merkle_root: field::<merkle::Root>("merkleroot", &template.default_roots.merkle_root)?,
            commitment_bytes: field::<ChainHistoryBlockTxAuthCommitmentHash>(
                "blockcommitmentshash",
                &template.default_roots.block_commitments_hash,
            )?
            .bytes_in_serialized_order()
            .into(),
            time: DateTime32::from(template.cur_time).into(),
            difficulty_threshold: field::<CompactDifficulty>("bits", &template.bits)?,
            nonce: [0; cyphes_pow::NONCE_LEN].into(),
            solution: Solution::for_proposal(),
        };
        let transactions = std::iter::once(&template.coinbase_txn)
            .chain(&template.transactions)
            .map(|tx| hex::decode(&tx.data).map_err(|_| TemplateError("bad transaction".into())))
            .collect::<Result<Vec<_>, _>>()?;
        Job::new(id, header, transactions, template.height, share_difficulty)
    }

    /// Builds a job from a header template and its serialized transactions.
    pub fn new(
        id: String,
        header: Header,
        transactions: Vec<Vec<u8>>,
        height: u32,
        share_difficulty: Option<u64>,
    ) -> Result<Job, TemplateError> {
        let block_target = header
            .difficulty_threshold
            .to_expanded()
            .ok_or_else(|| TemplateError("bad bits".into()))?;
        let block_target_bytes = target_bytes(block_target);

        // A share target easier than the block's (a larger number), or the
        // block's own target.
        let share_target = share_difficulty
            .filter(|d| *d > 1)
            .map(|d| U256::MAX / U256::from(d))
            .filter(|t| *t > U256::from(block_target))
            .map(|t| target_bytes(t.into()))
            .unwrap_or(block_target_bytes);
        let share_difficulty = BeamDifficulty::from_target(&share_target)
            .ok_or_else(|| TemplateError("target too hard for Beam's difficulty format".into()))?;

        Ok(Job {
            id,
            height,
            previous_block_hash: header.previous_block_hash,
            input: Solution::pow_input(&header),
            header,
            transactions,
            block_target,
            share_difficulty,
            seen: Mutex::new(HashSet::new()),
        })
    }

    /// Checks a miner's solution, given as the protocol's hex strings.
    ///
    /// In order: the nonce starts with the connection's `nonce_prefix`
    /// (hex digits); the solution is a valid BeamHash III solution for this
    /// job; it meets the share difficulty; it is not a repeat; and whether it
    /// also meets the block target.
    pub fn check(&self, nonce_hex: &str, output_hex: &str, nonce_prefix: &str) -> Verdict {
        if !nonce_hex
            .to_ascii_lowercase()
            .starts_with(&nonce_prefix.to_ascii_lowercase())
        {
            return Verdict::Rejected("nonce outside the connection's prefix");
        }
        let Ok(nonce) = <[u8; cyphes_pow::NONCE_LEN]>::from_hex(nonce_hex) else {
            return Verdict::Rejected("malformed nonce");
        };
        let Ok(solution) = <[u8; cyphes_pow::SOLUTION_LEN]>::from_hex(output_hex) else {
            return Verdict::Rejected("malformed solution");
        };

        let mut header = self.header;
        header.nonce = nonce.into();
        header.solution = Solution(solution);
        if header.solution.check(&header).is_err() {
            return Verdict::Rejected("invalid BeamHash III solution");
        }

        let hash = cyphes_pow::solution_hash(&solution);
        if !self.share_difficulty.is_target_reached(&hash) {
            return Verdict::Rejected("below the share difficulty");
        }
        let mut key = [0; 32];
        key[..8].copy_from_slice(&nonce);
        key[8..].copy_from_slice(&hash[..24]);
        if !self.seen.lock().expect("unpoisoned").insert(key) {
            return Verdict::Rejected("duplicate solution");
        }

        if !header.solution.meets_threshold(self.block_target) {
            return Verdict::Share;
        }
        match self.block(header) {
            Some(block) => Verdict::Block(Box::new(block)),
            None => Verdict::Rejected("could not assemble the block"),
        }
    }

    /// The full block for a solved header.
    fn block(&self, header: Header) -> Option<Block> {
        let mut bytes = Vec::new();
        header.zcash_serialize(&mut bytes).ok()?;
        write_compact_size(&mut bytes, self.transactions.len() as u64);
        for tx in &self.transactions {
            bytes.extend_from_slice(tx);
        }
        bytes.as_slice().zcash_deserialize_into().ok()
    }
}

/// A job shared between connections.
pub type SharedJob = Arc<Job>;

fn target_bytes(target: ExpandedDifficulty) -> [u8; 32] {
    U256::from(target).to_big_endian()
}

fn write_compact_size(out: &mut Vec<u8>, n: u64) {
    match n {
        0..=0xfc => out.push(n as u8),
        0xfd..=0xffff => {
            out.push(0xfd);
            out.extend_from_slice(&(n as u16).to_le_bytes());
        }
        0x1_0000..=0xffff_ffff => {
            out.push(0xfe);
            out.extend_from_slice(&(n as u32).to_le_bytes());
        }
        _ => {
            out.push(0xff);
            out.extend_from_slice(&n.to_le_bytes());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// CYPHES regtest block 1, mined by the node with BeamHash III.
    fn block_1() -> Block {
        zebra_test::vectors::CYPHES_REGTEST_BLOCKS[0]
            .zcash_deserialize_into()
            .expect("block 1")
    }

    fn job_for(block: &Block, share_difficulty: Option<u64>) -> Job {
        let mut header = *block.header;
        header.nonce = [0; 8].into();
        header.solution = Solution::for_proposal();
        let txs = block
            .transactions
            .iter()
            .map(|tx| tx.zcash_serialize_to_vec().expect("tx"))
            .collect();
        Job::new("1".into(), header, txs, 1, share_difficulty).expect("job")
    }

    fn solved(block: &Block) -> (String, String) {
        (
            hex::encode(*block.header.nonce),
            hex::encode(block.header.solution.0),
        )
    }

    #[test]
    fn a_solution_meeting_the_block_target_rebuilds_the_block() {
        let block = block_1();
        let job = job_for(&block, None);
        let (nonce, output) = solved(&block);
        assert_eq!(
            job.check(&nonce, &output, ""),
            Verdict::Block(Box::new(block.clone()))
        );
        assert_eq!(
            job.check(&nonce, &output, ""),
            Verdict::Rejected("duplicate solution"),
            "a repeat is refused"
        );
    }

    #[test]
    fn a_share_below_a_harder_block_target_is_only_a_share() {
        let block = block_1();
        // The header's bits are part of the PoW input, so keep them and make
        // the block target far harder than this solution instead.
        let mut job = job_for(&block, None);
        job.block_target = ExpandedDifficulty::from(U256::one() << 100);
        let (nonce, output) = solved(&block);
        assert_eq!(job.check(&nonce, &output, ""), Verdict::Share);
    }

    #[test]
    fn bad_solutions_are_rejected() {
        let block = block_1();
        let job = job_for(&block, None);
        let (nonce, output) = solved(&block);

        let mut corrupt = hex::decode(&output).expect("hex");
        corrupt[10] ^= 1;
        assert_eq!(
            job.check(&nonce, &hex::encode(corrupt), ""),
            Verdict::Rejected("invalid BeamHash III solution")
        );
        let mut other_nonce = hex::decode(&nonce).expect("hex");
        other_nonce[7] ^= 1;
        assert_eq!(
            job.check(&hex::encode(other_nonce), &output, ""),
            Verdict::Rejected("invalid BeamHash III solution")
        );
        assert_eq!(
            job.check("00", &output, ""),
            Verdict::Rejected("malformed nonce")
        );
        assert_eq!(
            job.check(&nonce, "00", ""),
            Verdict::Rejected("malformed solution")
        );
        let wrong_prefix = if nonce.starts_with('f') { "e" } else { "f" };
        assert_eq!(
            job.check(&nonce, &output, wrong_prefix),
            Verdict::Rejected("nonce outside the connection's prefix")
        );
        assert!(matches!(
            job.check(&nonce, &output, &nonce[..2]),
            Verdict::Block(_)
        ));
    }

    #[test]
    fn the_job_input_is_the_consensus_pow_input() {
        let block = block_1();
        let job = job_for(&block, None);
        assert_eq!(job.input, Solution::pow_input(&block.header));
        // Miners see exactly the difficulty they must meet for a block.
        assert_eq!(
            Some(job.share_difficulty),
            BeamDifficulty::from_target(&target_bytes(job.block_target))
        );
    }
}
