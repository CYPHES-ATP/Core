//! Block difficulty adjustment calculations for contextual validation.
//!
//! This module supports the following consensus rule calculations:
//!  * CYPHES's LWMA-1 difficulty adjustment (`cyphes_params::lwma_next_target`),
//!    which replaces Zcash's `ThresholdBits` and has no testnet minimum
//!    difficulty rule, and
//!  * `median-time-past`.

use chrono::{DateTime, Utc};

use zebra_chain::{
    block::{self, Block},
    parameters::Network,
    work::difficulty::{CompactDifficulty, ExpandedDifficulty, ParameterDifficulty as _, U256},
    BoundedVec,
};

/// The median block span for time median calculations.
///
/// `PoWMedianBlockSpan` in the Zcash specification.
pub const POW_MEDIAN_BLOCK_SPAN: usize = 11;

/// The number of previous blocks read to adjust the difficulty and check times:
/// the LWMA window plus the block before it (121), which also covers the
/// median-time-past span.
pub const POW_ADJUSTMENT_BLOCK_SPAN: usize = cyphes_params::difficulty::DIFFICULTY_LOOKBACK;

const _: () = assert!(POW_ADJUSTMENT_BLOCK_SPAN >= POW_MEDIAN_BLOCK_SPAN);

/// The maximum number of seconds between the `median-time-past` of a block,
/// and the block's `time` field: 10 minutes, see
/// `cyphes_params::MAX_TIME_SINCE_MEDIAN_SECS`.
pub const BLOCK_MAX_TIME_SINCE_MEDIAN: u32 = cyphes_params::MAX_TIME_SINCE_MEDIAN_SECS as u32;

/// Contains the context needed to calculate the adjusted difficulty for a block.
pub(crate) struct AdjustedDifficulty {
    /// The `header.time` field from the candidate block
    candidate_time: DateTime<Utc>,
    /// The coinbase height from the candidate block
    ///
    /// If we only have the header, this field is calculated from the previous
    /// block height.
    candidate_height: block::Height,
    /// The configured network
    network: Network,
    /// The `header.difficulty_threshold`s from the previous
    /// `PoWAveragingWindow + PoWMedianBlockSpan` (28) blocks, in reverse height
    /// order.
    relevant_difficulty_thresholds: BoundedVec<CompactDifficulty, 1, POW_ADJUSTMENT_BLOCK_SPAN>,
    /// The `header.time`s from the previous
    /// `PoWAveragingWindow + PoWMedianBlockSpan` (28) blocks, in reverse height
    /// order.
    ///
    /// Only the first and last `PoWMedianBlockSpan` times are used. Times
    /// `11..=16` are ignored.
    relevant_times: BoundedVec<DateTime<Utc>, 1, POW_ADJUSTMENT_BLOCK_SPAN>,
}

impl AdjustedDifficulty {
    /// Initialise and return a new `AdjustedDifficulty` using a `candidate_block`,
    /// `network`, and a `context`.
    ///
    /// The `context` contains the previous
    /// `PoWAveragingWindow + PoWMedianBlockSpan` (28) `difficulty_threshold`s and
    /// `time`s from the relevant chain for `candidate_block`, in reverse height
    /// order, starting with the previous block.
    ///
    /// Note that the `time`s might not be in reverse chronological order, because
    /// block times are supplied by miners.
    ///
    /// # Panics
    ///
    /// This function may panic in the following cases:
    /// - The `candidate_block` has no coinbase height (should never happen for valid blocks).
    /// - The `candidate_block` is the genesis block, so `previous_block_height` cannot be computed.
    /// - `AdjustedDifficulty::new_from_header_time` panics.
    pub fn new_from_block<C>(
        candidate_block: &Block,
        network: &Network,
        context: C,
    ) -> AdjustedDifficulty
    where
        C: IntoIterator<Item = (CompactDifficulty, DateTime<Utc>)>,
    {
        let candidate_block_height = candidate_block
            .coinbase_height()
            .expect("semantically valid blocks have a coinbase height");
        let previous_block_height = (candidate_block_height - 1)
            .expect("contextual validation is never run on the genesis block");

        AdjustedDifficulty::new_from_header_time(
            candidate_block.header.time,
            previous_block_height,
            network,
            context,
        )
    }

    /// Initialise and return a new [`AdjustedDifficulty`] using a
    /// `candidate_header_time`, `previous_block_height`, `network`, and a `context`.
    ///
    /// Designed for use when validating block headers, where the full block has not
    /// been downloaded yet.
    ///
    /// See [`Self::new_from_block`] for detailed information about the `context`.
    ///
    /// # Panics
    ///
    /// This function may panic in the following cases:
    /// - The next block height is invalid.
    /// - The `context` iterator is empty, because at least one difficulty threshold
    ///   and block time are required to construct the `Bounded` vectors.
    /// - The context iterator is empty, because at least one difficulty threshold and block time are required.
    pub fn new_from_header_time<C>(
        candidate_header_time: DateTime<Utc>,
        previous_block_height: block::Height,
        network: &Network,
        context: C,
    ) -> AdjustedDifficulty
    where
        C: IntoIterator<Item = (CompactDifficulty, DateTime<Utc>)>,
    {
        let candidate_height = (previous_block_height + 1).expect("next block height is valid");

        let (thresholds, times) = context
            .into_iter()
            .take(POW_ADJUSTMENT_BLOCK_SPAN)
            .unzip::<_, _, Vec<_>, Vec<_>>();

        let relevant_difficulty_thresholds: BoundedVec<
            CompactDifficulty,
            1,
            POW_ADJUSTMENT_BLOCK_SPAN,
        > = thresholds
            .try_into()
            .expect("context must provide a bounded number of difficulty thresholds");
        let relevant_times: BoundedVec<DateTime<Utc>, 1, POW_ADJUSTMENT_BLOCK_SPAN> = times
            .try_into()
            .expect("context must provide a bounded number of block times");

        AdjustedDifficulty {
            candidate_time: candidate_header_time,
            candidate_height,
            network: network.clone(),
            relevant_difficulty_thresholds,
            relevant_times,
        }
    }

    /// Returns the candidate block's height.
    pub fn candidate_height(&self) -> block::Height {
        self.candidate_height
    }

    /// Returns the candidate block's time field.
    pub fn candidate_time(&self) -> DateTime<Utc> {
        self.candidate_time
    }

    /// Returns the configured network.
    pub fn network(&self) -> Network {
        self.network.clone()
    }

    /// Calculate the expected `difficulty_threshold` for a candidate block, based
    /// on the `candidate_time`, `candidate_height`, `network`, and the
    /// `difficulty_threshold`s and `time`s from the previous
    /// `PoWAveragingWindow + PoWMedianBlockSpan` (28) blocks in the relevant chain.
    ///
    /// Implements CYPHES's LWMA-1 rule. There is no testnet minimum difficulty
    /// rule.
    pub fn expected_difficulty_threshold(&self) -> CompactDifficulty {
        // Regtest uses fixed minimum difficulty with no retargeting, matching
        // Bitcoin's `fPowNoRetargeting`, so local test chains mine instantly.
        if self.network.is_regtest() {
            return self.network.target_difficulty_limit().to_compact();
        }

        self.lwma_threshold()
    }

    /// The LWMA-1 target for the candidate block, see
    /// [`cyphes_params::lwma_next_target`].
    fn lwma_threshold(&self) -> CompactDifficulty {
        // The context is newest first; LWMA reads it oldest first.
        let chain: Vec<(i64, cyphes_params::U256)> = self
            .relevant_times
            .iter()
            .zip(self.relevant_difficulty_thresholds.iter())
            .rev()
            .map(|(time, bits)| {
                let target: U256 = bits
                    .to_expanded()
                    .expect("previous blocks have valid difficulty thresholds")
                    .into();
                (time.timestamp(), cyphes_params::U256(target.0))
            })
            .collect();

        let pow_limit: U256 = self.network.target_difficulty_limit().into();
        let next = cyphes_params::lwma_next_target(
            &chain,
            cyphes_params::U256(pow_limit.0),
            cyphes_params::TARGET_SPACING_SECS,
        );

        ExpandedDifficulty::from(U256(next.0)).to_compact()
    }

    /// Calculate the median of the `time`s from the previous
    /// `PoWMedianBlockSpan` (11) blocks in the relevant chain.
    ///
    /// Implements `median-time-past` and `MedianTime(candidate_height)` from the
    /// Zcash specification. (These functions are identical, but they are
    /// specified in slightly different ways.)
    pub fn median_time_past(&self) -> DateTime<Utc> {
        let median_times: Vec<DateTime<Utc>> = self
            .relevant_times
            .iter()
            .take(POW_MEDIAN_BLOCK_SPAN)
            .cloned()
            .collect();

        AdjustedDifficulty::median_time(median_times)
    }

    /// Calculate the median of the `median_block_span_times`: the `time`s from a
    /// Vec of `PoWMedianBlockSpan` (11) or fewer blocks in the relevant chain.
    ///
    /// Implements `MedianTime` from the Zcash specification.
    ///
    /// # Panics
    ///
    /// If provided an empty Vec
    pub(crate) fn median_time(mut median_block_span_times: Vec<DateTime<Utc>>) -> DateTime<Utc> {
        median_block_span_times.sort_unstable();

        // > median(𝑆) := sorted(𝑆)_{ceiling((length(𝑆)+1)/2)}
        // <https://zips.z.cash/protocol/protocol.pdf>, section 7.7.3, Difficulty Adjustment (p. 132)
        let median_idx = median_block_span_times.len() / 2;
        median_block_span_times[median_idx]
    }
}

#[cfg(test)]
mod lwma_tests {
    use chrono::TimeZone;

    use super::*;

    /// `blocks` newest first, as the state supplies them.
    fn context(blocks: &[(i64, CompactDifficulty)]) -> Vec<(CompactDifficulty, DateTime<Utc>)> {
        blocks
            .iter()
            .map(|(t, bits)| {
                (
                    *bits,
                    Utc.timestamp_opt(*t, 0).single().expect("valid time"),
                )
            })
            .collect()
    }

    #[test]
    fn lwma_weights_the_most_recent_blocks() {
        let _init_guard = zebra_test::init();

        let network = Network::new_default_testnet();
        let bits = ExpandedDifficulty::from(U256::one() << 230).to_compact();
        let tip = 1_800_000_000i64;

        // Newest first. The newest 60 blocks took 10 s each, the 61 before them 40 s.
        let mut time = tip;
        let mut recent_fast = Vec::new();
        for i in 0..POW_ADJUSTMENT_BLOCK_SPAN {
            recent_fast.push((time, bits));
            time -= if i < 60 { 10 } else { 40 };
        }
        // The same solvetimes in the opposite order: old blocks fast, recent slow.
        let mut time = tip;
        let mut recent_slow = Vec::new();
        for i in 0..POW_ADJUSTMENT_BLOCK_SPAN {
            recent_slow.push((time, bits));
            time -= if i < 61 { 40 } else { 10 };
        }

        let expected = |blocks: &[(i64, CompactDifficulty)]| {
            AdjustedDifficulty::new_from_header_time(
                Utc.timestamp_opt(tip + 25, 0).single().expect("valid time"),
                block::Height(1_000),
                &network,
                context(blocks),
            )
            .expected_difficulty_threshold()
            .to_expanded()
            .expect("valid threshold")
        };

        let base = bits.to_expanded().expect("valid threshold");
        // Recent speed dominates: fast recent blocks raise the difficulty
        // (lower the target), slow recent blocks lower it.
        assert!(expected(&recent_fast) < base);
        assert!(expected(&recent_slow) > base);

        // The glue passes blocks to LWMA oldest first.
        let oldest_first: Vec<(i64, cyphes_params::U256)> = recent_fast
            .iter()
            .rev()
            .map(|(t, b)| {
                let target: U256 = b.to_expanded().expect("valid").into();
                (*t, cyphes_params::U256(target.0))
            })
            .collect();
        let limit: U256 = network.target_difficulty_limit().into();
        let direct = cyphes_params::lwma_next_target(
            &oldest_first,
            cyphes_params::U256(limit.0),
            cyphes_params::TARGET_SPACING_SECS,
        );
        assert_eq!(
            expected(&recent_fast).to_compact(),
            ExpandedDifficulty::from(U256(direct.0)).to_compact()
        );
    }

    #[test]
    fn block_after_genesis_keeps_the_genesis_target() {
        let _init_guard = zebra_test::init();

        let network = Network::new_default_testnet();
        let genesis_bits = ExpandedDifficulty::from(U256::one() << 240).to_compact();
        let adjusted = AdjustedDifficulty::new_from_header_time(
            Utc.timestamp_opt(1_800_000_030, 0)
                .single()
                .expect("valid time"),
            block::Height(0),
            &network,
            context(&[(1_800_000_000, genesis_bits)]),
        );
        assert_eq!(adjusted.expected_difficulty_threshold(), genesis_bits);
    }
}
