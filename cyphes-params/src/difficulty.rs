//! LWMA-1 difficulty adjustment for 25-second blocks.
//!
//! zawy12's linearly weighted moving average: the next target is the average
//! target of the last `N` blocks, scaled by their solvetimes weighted
//! 1..=N (most recent heaviest). It replaces Zcash's DigiShield/MTP-delayed
//! rule, which reacts too slowly for 25-second blocks and hash-rental swings.
//!
//! Exact rule, over the last `n = min(N, blocks - 1)` blocks:
//!
//! ```text
//! t_0      = time of the block before the window
//! t_j      = max(time_j, t_{j-1} + 1)          (monotonic, so solvetime >= 1)
//! st_j     = min(t_j - t_{j-1}, 6 * T)
//! next     = (sum_j target_j) * (sum_j j * st_j) / (n * n(n+1)/2 * T)
//! next     = clamp(next, 1, pow_limit)
//! ```
//!
//! computed with a 512-bit intermediate, so nothing overflows or truncates
//! before the final division.

pub use wide::U256;
use wide::U512;

// `construct_uint!` expansions trip clippy lints we can't fix at the call site.
#[allow(clippy::all)]
mod wide {
    use uint::construct_uint;

    construct_uint! {
        /// 256-bit unsigned integer for targets.
        pub struct U256(4);
    }

    construct_uint! {
        pub(super) struct U512(8);
    }
}

/// Blocks in the averaging window: about 50 minutes at 25 seconds.
pub const LWMA_WINDOW: usize = 120;

/// Solvetimes are capped at this many target spacings, so one slow block
/// cannot crash the difficulty.
pub const MAX_SOLVETIME_SPACINGS: i64 = 6;

/// Blocks the difficulty rule reads: the window plus the block before it.
pub const DIFFICULTY_LOOKBACK: usize = LWMA_WINDOW + 1;

/// The target for the next block.
///
/// `chain` holds the most recent blocks as `(time, target)`, oldest first,
/// ending at the current tip. Only the last [`DIFFICULTY_LOOKBACK`] entries
/// are read.
///
/// With only genesis there is no solvetime yet, so block 1 keeps the genesis
/// target. Genesis `bits` is therefore a launch parameter: an estimate of
/// launch hashrate, erring hard. Starting at `pow_limit` instead would keep
/// one very easy target in the arithmetic mean for a whole window, and the
/// first ~100 blocks would come several times too fast.
pub fn lwma_next_target(chain: &[(i64, U256)], pow_limit: U256, spacing: i64) -> U256 {
    let window = &chain[chain.len().saturating_sub(DIFFICULTY_LOOKBACK)..];
    if window.len() < 2 {
        return window
            .last()
            .map_or(pow_limit, |(_, target)| (*target).min(pow_limit));
    }
    let n = (window.len() - 1) as u64;

    let mut previous = window[0].0;
    let mut weighted_solvetimes = 0u64;
    let mut target_sum = U512::zero();
    for (j, (time, target)) in window[1..].iter().enumerate() {
        let this = if *time > previous {
            *time
        } else {
            previous + 1
        };
        let solvetime = (this - previous).min(MAX_SOLVETIME_SPACINGS * spacing);
        previous = this;
        weighted_solvetimes += solvetime as u64 * (j as u64 + 1);
        target_sum += widen(*target);
    }

    let k = n * (n + 1) / 2 * spacing as u64;
    let next = target_sum * U512::from(weighted_solvetimes) / U512::from(n * k);
    if next > widen(pow_limit) {
        pow_limit
    } else {
        narrow(next).max(U256::one())
    }
}

fn widen(x: U256) -> U512 {
    let mut words = [0u64; 8];
    words[..4].copy_from_slice(&x.0);
    U512(words)
}

fn narrow(x: U512) -> U256 {
    debug_assert!(x.0[4..].iter().all(|w| *w == 0));
    U256([x.0[0], x.0[1], x.0[2], x.0[3]])
}

#[cfg(test)]
mod tests {
    use super::*;

    const T: i64 = crate::TARGET_SPACING_SECS;

    fn limit() -> U256 {
        // Compact 0x2000ffff: 0xffff << 232.
        U256::from(0xffffu64) << 232
    }

    fn to_f64(x: U256) -> f64 {
        x.0.iter()
            .rev()
            .fold(0.0, |acc, w| acc * 18446744073709551616.0 + *w as f64)
    }

    /// Deterministic xorshift64* for reproducible simulations.
    struct Rng(u64);
    impl Rng {
        fn unit(&mut self) -> f64 {
            self.0 ^= self.0 >> 12;
            self.0 ^= self.0 << 25;
            self.0 ^= self.0 >> 27;
            ((self.0.wrapping_mul(0x2545_f491_4f6c_dd1d) >> 11) as f64 + 0.5) / (1u64 << 53) as f64
        }
    }

    /// Target at which `hashrate` solutions/s finds a block every `T` seconds.
    fn equilibrium(hashrate: f64) -> f64 {
        2f64.powi(256) / (hashrate * T as f64)
    }

    /// Mine `blocks` blocks after a genesis at `genesis_target`, against
    /// `hashrate(height)` (solutions/s) with exponential solvetimes. Returns
    /// the chain as (time, target).
    fn simulate_from(
        genesis_target: U256,
        blocks: usize,
        seed: u64,
        hashrate: impl Fn(usize) -> f64,
    ) -> Vec<(i64, U256)> {
        let mut rng = Rng(seed);
        let mut chain = vec![(0i64, genesis_target)];
        let mut clock = 0.0f64;
        for h in 1..=blocks {
            let target = lwma_next_target(&chain, limit(), T);
            // P(solution meets target) = target / 2^256.
            let p = to_f64(target) / 2f64.powi(256);
            let mean = 1.0 / (p * hashrate(h));
            clock += -rng.unit().ln() * mean;
            chain.push((clock as i64, target));
        }
        chain
    }

    fn simulate(blocks: usize, seed: u64, hashrate: impl Fn(usize) -> f64) -> Vec<(i64, U256)> {
        simulate_from(from_f64(equilibrium(hashrate(1))), blocks, seed, hashrate)
    }

    fn from_f64(x: f64) -> U256 {
        let mut words = [0u64; 4];
        let mut rest = x;
        for i in (0..4).rev() {
            let scale = 2f64.powi(64 * i as i32);
            let w = (rest / scale).floor();
            words[i] = w as u64;
            rest -= w * scale;
        }
        U256(words)
    }

    fn mean_solvetime(chain: &[(i64, U256)], from: usize) -> f64 {
        let span = &chain[from..];
        (span.last().unwrap().0 - span[0].0) as f64 / (span.len() - 1) as f64
    }

    #[test]
    fn genesis_successor_keeps_the_genesis_target() {
        let genesis = U256::from(1u64) << 230;
        assert_eq!(lwma_next_target(&[(0, genesis)], limit(), T), genesis);
        assert_eq!(lwma_next_target(&[(0, limit() * 2)], limit(), T), limit());
        assert_eq!(lwma_next_target(&[], limit(), T), limit());
    }

    #[test]
    fn on_schedule_blocks_keep_the_target() {
        let target = U256::from(1u64) << 200;
        let chain: Vec<_> = (0..=LWMA_WINDOW as i64).map(|i| (i * T, target)).collect();
        assert_eq!(lwma_next_target(&chain, limit(), T), target);
    }

    #[test]
    fn fast_blocks_make_it_harder_slow_blocks_easier() {
        let target = U256::from(1u64) << 200;
        let fast: Vec<_> = (0..=LWMA_WINDOW as i64).map(|i| (i * 10, target)).collect();
        let slow: Vec<_> = (0..=LWMA_WINDOW as i64).map(|i| (i * 50, target)).collect();
        assert_eq!(lwma_next_target(&fast, limit(), T), target * 10 / 25);
        assert_eq!(lwma_next_target(&slow, limit(), T), target * 2);
    }

    #[test]
    fn solvetimes_are_capped_and_never_negative() {
        let target = U256::from(1u64) << 200;
        // One enormous gap is capped at 6T; out-of-order timestamps count as 1s.
        let mut chain: Vec<_> = (0..=LWMA_WINDOW as i64).map(|i| (i * T, target)).collect();
        chain.last_mut().unwrap().0 += 1_000_000;
        let capped = lwma_next_target(&chain, limit(), T);
        let mut with_6t: Vec<_> = (0..=LWMA_WINDOW as i64).map(|i| (i * T, target)).collect();
        with_6t.last_mut().unwrap().0 += 5 * T;
        assert_eq!(capped, lwma_next_target(&with_6t, limit(), T));

        let backwards: Vec<_> = (0..=LWMA_WINDOW as i64)
            .map(|i| (-i * 1000, target))
            .collect();
        let next = lwma_next_target(&backwards, limit(), T);
        assert!(next > U256::zero() && next < target);
    }

    #[test]
    fn never_exceeds_pow_limit_or_hits_zero() {
        let slow: Vec<_> = (0..=LWMA_WINDOW as i64)
            .map(|i| (i * 100 * T, limit()))
            .collect();
        assert_eq!(lwma_next_target(&slow, limit(), T), limit());
        let tiny: Vec<_> = (0..=LWMA_WINDOW as i64).map(|i| (i, U256::one())).collect();
        assert_eq!(lwma_next_target(&tiny, limit(), T), U256::one());
    }

    #[test]
    fn only_the_window_is_read() {
        let target = U256::from(1u64) << 200;
        let recent: Vec<_> = (0..=LWMA_WINDOW as i64)
            .map(|i| (1_000_000 + i * T, target))
            .collect();
        let mut long: Vec<_> = (0..500).map(|i| (i, U256::one())).collect();
        long.extend(recent.iter().copied());
        assert_eq!(
            lwma_next_target(&long, limit(), T),
            lwma_next_target(&recent, limit(), T)
        );
    }

    #[test]
    fn converges_to_25_seconds_under_steady_hashrate() {
        // 1,500 solutions/s is roughly fifty mid-range GPUs.
        let chain = simulate(20_000, 7, |_| 1_500.0);
        let mean = mean_solvetime(&chain, 2_000);
        assert!((24.0..26.0).contains(&mean), "mean solvetime {mean}");
    }

    /// Blocks mined beyond schedule: blocks found minus elapsed time / T.
    fn excess_blocks(chain: &[(i64, U256)]) -> f64 {
        (chain.len() - 1) as f64 - chain.last().unwrap().0 as f64 / T as f64
    }

    #[test]
    fn launch_estimate_too_hard_eases_within_a_few_blocks() {
        // Genesis assumed 10x the hashrate that shows up.
        let chain = simulate_from(from_f64(equilibrium(15_000.0)), 60, 5, |_| 1_500.0);
        let eased = chain
            .iter()
            .position(|(_, t)| to_f64(*t) > equilibrium(1_500.0) / 2.0)
            .unwrap();
        eprintln!(
            "10x-hard launch: within 2x of equilibrium at block {eased}, t={}s",
            chain[eased].0
        );
        assert!(eased <= 8, "within 2x of equilibrium at block {eased}");
        // The slow start costs minutes, not hours.
        assert!(chain[eased].0 < 30 * 60, "{}s", chain[eased].0);
    }

    #[test]
    fn launch_estimate_too_easy_bounds_the_instamine() {
        // Genesis assumed a quarter of the real hashrate. Arithmetic-mean LWMA
        // tightens slowly from an easy start, so measure the damage: blocks
        // mined ahead of the 25-second schedule over the first two windows.
        let chain = simulate_from(from_f64(equilibrium(375.0)), 2 * LWMA_WINDOW, 9, |_| {
            1_500.0
        });
        let excess = excess_blocks(&chain);
        eprintln!(
            "4x-easy launch: {excess:.1} blocks ahead; pow_limit launch: {:.1}",
            excess_blocks(&simulate_from(limit(), 2 * LWMA_WINDOW, 9, |_| 1_500.0))
        );
        assert!(excess < 60.0, "{excess} blocks ahead of schedule");
        // ...and a pow_limit start is far worse, which is why genesis bits is
        // a launch estimate rather than the limit.
        let from_limit = simulate_from(limit(), 2 * LWMA_WINDOW, 9, |_| 1_500.0);
        assert!(
            excess_blocks(&from_limit) > excess,
            "limit start {}",
            excess_blocks(&from_limit)
        );
    }

    #[test]
    fn recovers_from_a_10x_hashrate_swing() {
        // Hash rental arrives at block 3,000 and leaves at 6,000.
        let chain = simulate(9_000, 3, |h| {
            if (3_000..6_000).contains(&h) {
                15_000.0
            } else {
                1_500.0
            }
        });
        let during = mean_solvetime(&chain, 3_000 + 3 * LWMA_WINDOW);
        let after = mean_solvetime(&chain[..], 6_000 + 3 * LWMA_WINDOW);
        assert!((22.0..28.0).contains(&during), "during {during}");
        assert!((22.0..28.0).contains(&after), "after {after}");
        // Blocks right after the rental leaves are slow, but bounded.
        let exit_stall = chain[6_000 + LWMA_WINDOW].0 - chain[6_000].0;
        assert!(
            exit_stall < (LWMA_WINDOW as i64) * T * 6,
            "exit stall {exit_stall}s"
        );
    }
}
