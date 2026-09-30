//! Difficulty checks on `SHA-256(solution)`.
//!
//! Consensus compares the solution hash, read as a big-endian 256-bit number,
//! against the block's expanded target ([`hash_meets_target`]).
//!
//! [`BeamDifficulty`] is Beam's packed difficulty. Stratum jobs sent to
//! BeamHash III miners carry share difficulty in this format, and miners only
//! submit solutions that satisfy it, so the stratum bridge needs the exact
//! rule (`beam::Difficulty::IsTargetReached`).

use primitive_types::U256;
use sha2::{Digest, Sha256};

/// `SHA-256(solution)`, the value difficulty is checked against.
pub fn solution_hash(solution: &[u8]) -> [u8; 32] {
    Sha256::digest(solution).into()
}

/// `hash <= target`, both big-endian 256-bit numbers.
pub fn hash_meets_target(hash: &[u8; 32], target: &[u8; 32]) -> bool {
    // Lexicographic order on big-endian bytes is numeric order.
    hash <= target
}

/// Beam's packed difficulty: `order << 24 | mantissa`, where the 24-bit
/// mantissa has an implicit leading one. The raw value is
/// `(2^24 | mantissa) * 2^(order - 24)`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct BeamDifficulty(pub u32);

impl BeamDifficulty {
    const MANTISSA_BITS: u32 = 24;
    const MAX_ORDER: u32 = 256 - Self::MANTISSA_BITS - 1;
    /// Encodes an unreachable difficulty.
    pub const INFINITE: Self = Self((Self::MAX_ORDER + 1) << Self::MANTISSA_BITS);

    /// Pack a raw difficulty, truncating to 25 significant bits like
    /// `beam::Difficulty::PackLo`. Zero packs to `BeamDifficulty(0)`, which
    /// Beam treats as difficulty one.
    pub fn from_raw(raw: u128) -> Self {
        if raw == 0 {
            return Self(0);
        }
        let order = 127 - raw.leading_zeros();
        let mantissa = if order <= Self::MANTISSA_BITS {
            raw << (Self::MANTISSA_BITS - order)
        } else {
            raw >> (order - Self::MANTISSA_BITS)
        } as u32;
        Self((mantissa & ((1 << Self::MANTISSA_BITS) - 1)) | (order << Self::MANTISSA_BITS))
    }

    /// The easiest difficulty whose accepted hashes all meet `target`
    /// (`hash <= target`, big-endian): `2^256 / (target + 1)`, packed
    /// rounding the mantissa up.
    ///
    /// A stratum server sends this as the job difficulty for a share target.
    /// Miners only submit solutions that pass [`Self::is_target_reached`],
    /// `hash * mantissa < 2^(280 - order)`; with the mantissa rounded up that
    /// implies `hash < target + 1`, so every submitted solution meets
    /// `target`. Rounding costs at most a 2^-24 fraction of the target.
    ///
    /// Returns `None` for targets below about 2^25, harder than the hardest
    /// difficulty the packing can express.
    pub fn from_target(target: &[u8; 32]) -> Option<Self> {
        use primitive_types::U512;

        let target = U256::from_big_endian(target);
        // The difficulty in fixed point with MANTISSA_BITS fractional bits,
        // rounded up: q = ceil(2^280 / (target + 1)). It is at least 2^24,
        // since target + 1 <= 2^256.
        let n = U512::from(target) + 1;
        let q = ((U512::one() << (256 + Self::MANTISSA_BITS)) + n - 1) / n;
        let mut order = q.bits() as u32 - 1 - Self::MANTISSA_BITS;
        let remainder = q & ((U512::one() << order) - 1);
        let mut mantissa = (q >> order).low_u64() + u64::from(!remainder.is_zero());
        if mantissa == 1 << (Self::MANTISSA_BITS + 1) {
            mantissa = 1 << Self::MANTISSA_BITS;
            order += 1;
        }
        (order <= Self::MAX_ORDER).then_some(Self(
            (mantissa as u32 & ((1 << Self::MANTISSA_BITS) - 1)) | (order << Self::MANTISSA_BITS),
        ))
    }

    /// `beam::Difficulty::IsTargetReached`: `hash * mantissa < 2^(280 - order)`.
    pub fn is_target_reached(&self, hash: &[u8; 32]) -> bool {
        if self.0 > Self::INFINITE.0 {
            return false;
        }
        let order = self.0 >> Self::MANTISSA_BITS;
        let mantissa =
            (1u64 << Self::MANTISSA_BITS) | (self.0 & ((1 << Self::MANTISSA_BITS) - 1)) as u64;

        // 256-bit hash (little-endian words) times a 25-bit mantissa.
        let words: [u64; 4] = std::array::from_fn(|i| {
            u64::from_be_bytes(hash[24 - 8 * i..32 - 8 * i].try_into().expect("8 bytes"))
        });
        let mut product = [0u64; 5];
        let mut carry = 0u128;
        for (i, w) in words.iter().enumerate() {
            let t = (*w as u128) * (mantissa as u128) + carry;
            product[i] = t as u64;
            carry = t >> 64;
        }
        product[4] = carry as u64;

        let bit_len = product
            .iter()
            .rposition(|w| *w != 0)
            .map_or(0, |i| 64 * i as u32 + 64 - product[i].leading_zeros());
        bit_len <= 256 + Self::MANTISSA_BITS - order
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hash_with_leading_zero_bits(n: u32) -> [u8; 32] {
        // 0000..0 1 1111..1 : exactly n leading zero bits.
        let mut h = [0xffu8; 32];
        for bit in 0..n {
            h[(bit / 8) as usize] &= !(0x80 >> (bit % 8));
        }
        h
    }

    #[test]
    fn packing_matches_beam_examples() {
        // block_crypt.cpp: Difficulty(8 << s_MantissaBits) is 2^8.
        assert_eq!(BeamDifficulty::from_raw(1 << 8), BeamDifficulty(8 << 24));
        assert_eq!(BeamDifficulty::from_raw(1 << 22), BeamDifficulty(22 << 24));
        assert_eq!(BeamDifficulty::from_raw(1), BeamDifficulty(0));
        // 3 = 0b11: order 1, mantissa 1.5 -> 0x800000.
        assert_eq!(
            BeamDifficulty::from_raw(3),
            BeamDifficulty((1 << 24) | 0x80_0000)
        );
    }

    #[test]
    fn power_of_two_difficulty_needs_that_many_zero_bits() {
        // Difficulty 2^k accepts hashes below 2^(256-k).
        for k in [0u32, 1, 8, 22, 40, 100] {
            let d = BeamDifficulty(k << 24);
            assert!(
                d.is_target_reached(&hash_with_leading_zero_bits(k)),
                "k={k}"
            );
            if k > 0 {
                assert!(
                    !d.is_target_reached(&hash_with_leading_zero_bits(k - 1)),
                    "k={k}"
                );
            }
        }
    }

    /// The largest hash `d` accepts: `floor((2^(280 - order) - 1) / mantissa)`.
    fn largest_accepted(d: BeamDifficulty) -> primitive_types::U512 {
        use primitive_types::U512;
        let order = d.0 >> 24;
        let mantissa = U512::from((1u64 << 24) | u64::from(d.0 & 0xff_ffff));
        ((U512::one() << (280 - order)) - 1) / mantissa
    }

    #[test]
    fn from_target_accepts_only_hashes_meeting_the_target() {
        use primitive_types::U512;
        let to_bytes = |v: U256| {
            let mut b = [0u8; 32];
            v.to_big_endian(&mut b);
            b
        };
        // Fixed cases, then pseudo-random targets of every magnitude.
        let mut targets = vec![
            U256::MAX,
            U256::MAX >> 1,
            U256::one() << 200,
            (U256::one() << 200) - 1,
            U256::from(0x7f_ffffu64) << 232, // regtest limit, compact 0x207fffff
            U256::from(0xffffu64) << 232,    // mainnet limit, compact 0x2000ffff
            U256::from(3u64),
            U256::one(),
        ];
        let mut x = 0x9e37_79b9_7f4a_7c15u64;
        let mut next = || {
            x ^= x << 13;
            x ^= x >> 7;
            x ^= x << 17;
            x
        };
        for _ in 0..2_000 {
            let words = [next(), next(), next(), next()];
            let t = U256(words) >> (next() % 250);
            if !t.is_zero() {
                targets.push(t);
            }
        }
        for t in targets {
            let Some(d) = BeamDifficulty::from_target(&to_bytes(t)) else {
                assert!(t < U256::one() << 25, "{t:x} is representable");
                continue;
            };
            let largest = largest_accepted(d);
            assert!(largest <= U512::from(t), "{t:x}: accepts {largest:x}");
            let largest = U256::try_from(largest).expect("at most the target");
            assert!(d.is_target_reached(&to_bytes(largest)), "{t:x}");
            if largest < U256::MAX {
                assert!(!d.is_target_reached(&to_bytes(largest + 1)), "{t:x}");
            }
            // Tight: loses at most about a 2^-24 fraction of the target.
            assert!(largest >= t - (t >> 23), "{t:x}: only up to {largest:x}");
        }
        assert_eq!(BeamDifficulty::from_target(&[0; 32]), None);
        assert_eq!(
            BeamDifficulty::from_target(&[0xff; 32]),
            Some(BeamDifficulty(0))
        );
    }

    #[test]
    fn invalid_packing_is_rejected() {
        assert!(!BeamDifficulty(BeamDifficulty::INFINITE.0 + 1).is_target_reached(&[0; 32]));
        assert!(!BeamDifficulty::INFINITE.is_target_reached(&hash_with_leading_zero_bits(200)));
    }

    #[test]
    fn target_comparison_is_numeric() {
        let mut lo = [0u8; 32];
        let mut hi = [0u8; 32];
        lo[31] = 0xff;
        hi[30] = 0x01;
        assert!(hash_meets_target(&lo, &hi));
        assert!(!hash_meets_target(&hi, &lo));
        assert!(hash_meets_target(&hi, &hi));
    }
}
