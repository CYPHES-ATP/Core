//! CYPHES consensus and network parameters.
//!
//! Everything that makes CYPHES a different chain from Zcash lives here, in
//! one small crate the node and the wallet both depend on. The shielded
//! protocol itself (Ironwood: the `orchard` crate's post-NU6.3 circuit and V3
//! notes) is used unmodified and is deliberately not parameterised here.
//!
//! Monetary rule: 1,000 CYPH per block, a block every 25 seconds, halving
//! every 5,000,000 blocks. Proof of work is the only way CYPH is created.

#![forbid(unsafe_code)]

pub mod difficulty;
pub mod network;

pub use difficulty::{lwma_next_target, U256};
pub use network::{NetworkKind, NetworkParams};

/// Base units per CYPH.
pub const COIN: u64 = 100_000_000;

/// Block subsidy for the first era: 1,000 CYPH.
pub const INITIAL_SUBSIDY: u64 = 1_000 * COIN;

/// Blocks per halving era. Block `k * HALVING_INTERVAL` is the first block of era `k`.
pub const HALVING_INTERVAL: u32 = 5_000_000;

/// Target time between blocks.
pub const TARGET_SPACING_SECS: i64 = 25;

/// Upper bound on any amount, and on total supply: 10 billion CYPH.
///
/// Actual issuance ([`TOTAL_ISSUANCE`]) stays strictly below it.
pub const MAX_MONEY: u64 = 10_000_000_000 * COIN;

/// Every base unit proof of work will ever create: 9,999,998,999.25 CYPH.
///
/// `sum over eras k of HALVING_INTERVAL * (INITIAL_SUBSIDY >> k)`, minus the
/// genesis block, which pays nothing.
pub const TOTAL_ISSUANCE: u64 = 999_999_899_925_000_000;

// Issuance stays below the cap; Zebra's i64 `Amount` holds sums of two
// maximal values; Ironwood note values are u64.
const _: () = assert!(TOTAL_ISSUANCE < MAX_MONEY);
const _: () = assert!(2 * MAX_MONEY < i64::MAX as u64);

/// Blocks before a coinbase output may be spent.
pub const COINBASE_MATURITY: u32 = 100;

/// The block subsidy at `height`, in base units.
///
/// The genesis block pays nothing: there is no premine and no allocation.
/// There is no founders' reward, development fund or lockbox; the whole
/// subsidy goes to the miner.
pub const fn block_subsidy(height: u32) -> u64 {
    if height == 0 {
        return 0;
    }
    let halvings = height / HALVING_INTERVAL;
    if halvings >= u64::BITS {
        0
    } else {
        INITIAL_SUBSIDY >> halvings
    }
}

/// Total issuance of blocks `0..=height`, in base units.
pub fn issuance_through(height: u32) -> u64 {
    let mut total = 0u64;
    let mut era_start = 0u32;
    loop {
        let era = era_start / HALVING_INTERVAL;
        let era_end = era_start.saturating_add(HALVING_INTERVAL - 1).min(height);
        let first_paying = era_start.max(1);
        if first_paying <= era_end {
            total += u64::from(era_end - first_paying + 1) * block_subsidy(era_start.max(1));
        }
        if era_end == height || block_subsidy(era_start.max(1)) == 0 {
            return total;
        }
        era_start = (era + 1) * HALVING_INTERVAL;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn issuance_rule() {
        assert_eq!(block_subsidy(0), 0);
        assert_eq!(block_subsidy(1), 1_000 * COIN);
        assert_eq!(block_subsidy(HALVING_INTERVAL - 1), 1_000 * COIN);
        assert_eq!(block_subsidy(HALVING_INTERVAL), 500 * COIN);
        assert_eq!(block_subsidy(2 * HALVING_INTERVAL), 250 * COIN);
        assert_eq!(block_subsidy(3 * HALVING_INTERVAL), 125 * COIN);
        assert_eq!(block_subsidy(u32::MAX), 0);
    }

    #[test]
    fn subsidy_ends_after_era_36() {
        // 1,000 CYPH = 1e11 base units needs 37 bits, so era 37 pays nothing.
        assert_eq!(block_subsidy(37 * HALVING_INTERVAL - 1), 1);
        assert_eq!(block_subsidy(37 * HALVING_INTERVAL), 0);
    }

    #[test]
    fn total_issuance_is_exact_and_below_ten_billion() {
        let mut total = 0u64;
        for era in 0..64u32 {
            let Some(start) = era.checked_mul(HALVING_INTERVAL) else {
                break;
            };
            let paying = if era == 0 {
                HALVING_INTERVAL - 1
            } else {
                HALVING_INTERVAL
            };
            total += u64::from(paying) * block_subsidy(start.max(1));
        }
        assert_eq!(total, TOTAL_ISSUANCE);
        // Closed form: 2n - popcount(n) per era sum, times the era length, minus genesis.
        let n = INITIAL_SUBSIDY;
        assert_eq!(
            TOTAL_ISSUANCE,
            u64::from(HALVING_INTERVAL) * (2 * n - u64::from(n.count_ones())) - n
        );
        assert_eq!(issuance_through(u32::MAX), TOTAL_ISSUANCE);
    }

    #[test]
    fn issuance_through_matches_block_by_block_sum() {
        let heights = [
            0,
            1,
            2,
            99,
            HALVING_INTERVAL - 1,
            HALVING_INTERVAL,
            HALVING_INTERVAL + 7,
        ];
        for h in heights {
            // Sum eras in closed form, spot-checking against a direct loop near boundaries.
            let direct: u64 = if h <= 100 {
                (0..=h).map(block_subsidy).sum()
            } else {
                let tail_start = h.saturating_sub(100);
                issuance_through(tail_start - 1) + (tail_start..=h).map(block_subsidy).sum::<u64>()
            };
            assert_eq!(issuance_through(h), direct, "height {h}");
        }
        assert_eq!(
            issuance_through(HALVING_INTERVAL - 1),
            u64::from(HALVING_INTERVAL - 1) * INITIAL_SUBSIDY
        );
    }

    #[test]
    fn first_era_is_half_the_supply_and_about_four_years() {
        let first_era = issuance_through(HALVING_INTERVAL - 1);
        assert_eq!(first_era, u64::from(HALVING_INTERVAL - 1) * INITIAL_SUBSIDY);
        assert!(first_era as f64 / TOTAL_ISSUANCE as f64 > 0.4999);
        // 5M blocks * 25 s = 125M s.
        let years =
            (u64::from(HALVING_INTERVAL) * TARGET_SPACING_SECS as u64) as f64 / (365.25 * 86_400.0);
        assert!((3.9..4.0).contains(&years), "{years}");
    }
}
