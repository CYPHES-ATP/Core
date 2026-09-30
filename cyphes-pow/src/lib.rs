//! BeamHash III proof of work for CYPHES.
//!
//! The verifier is a direct port of Beam's reference implementation
//! (`3rdparty/crypto/beamHashIII_impl.cpp` in BeamMW/beam, Apache-2.0).
//! Parameters, Blake2b personalization and bit layout are identical, so the
//! BeamHash III GPU miners that already exist (lolMiner, GMiner) produce
//! solutions this crate accepts without modification. `reference/` holds a
//! harness around Beam's C++ code; `tests/` checks the two agree.
//!
//! A proof of work is `(input, nonce, solution)`:
//!
//! - `input`: 32 bytes, the hash of the block header without its PoW fields
//!   (this is what a stratum job hands to a miner),
//! - `nonce`: 8 bytes,
//! - `solution`: 104 bytes, 32 packed 25-bit indices followed by a 4-byte
//!   extra nonce.
//!
//! [`verify_solution`] checks the Wagner-tree structure. Difficulty is a
//! separate check on `SHA-256(solution)`, see [`solution_hash`].

#![forbid(unsafe_code)]

mod difficulty;
#[cfg(feature = "solver")]
pub mod solver;

pub use difficulty::{hash_meets_target, solution_hash, BeamDifficulty};

/// Length of the PoW input (header hash) handed to miners.
pub const INPUT_LEN: usize = 32;
/// Length of the nonce.
pub const NONCE_LEN: usize = 8;
/// Length of a solution: 100 bytes of packed indices plus a 4-byte extra nonce.
pub const SOLUTION_LEN: usize = 104;

const WORK_BITS: u32 = 448;
const COLLISION_BITS: u32 = 24;
const ROUNDS: u32 = 5;
const INDEX_BITS: u32 = COLLISION_BITS + 1;
const NUM_INDICES: usize = 1 << ROUNDS;
const PACKED_INDICES_LEN: usize = 100;
const WORK_WORDS: usize = (WORK_BITS / 64) as usize;

/// 448 bits of work, least-significant word first (`std::bitset<448>` in Beam).
type Work = [u64; WORK_WORDS];

/// Why a solution was rejected.
#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
pub enum Error {
    #[error("solution must be {SOLUTION_LEN} bytes, got {0}")]
    BadLength(usize),
    #[error("round {0}: sibling collision bits differ")]
    NoCollision(u32),
    #[error("round {0}: siblings share an index")]
    DuplicateIndex(u32),
    #[error("round {0}: siblings are not in canonical order")]
    BadOrder(u32),
    #[error("final work bits are not zero")]
    NonZeroResult,
}

/// Blake2b-256 personalised with `"Beam-PoW" || le32(448) || le32(5)`.
fn base_state() -> blake2b_simd::State {
    let mut personal = [0u8; 16];
    personal[..8].copy_from_slice(b"Beam-PoW");
    personal[8..12].copy_from_slice(&WORK_BITS.to_le_bytes());
    personal[12..].copy_from_slice(&ROUNDS.to_le_bytes());
    blake2b_simd::Params::new()
        .hash_length(32)
        .personal(&personal)
        .to_state()
}

/// The four SipHash keys for one `(input, nonce, extra_nonce)` attempt.
fn pre_pow(input: &[u8], nonce: &[u8; NONCE_LEN], extra_nonce: &[u8]) -> [u64; 4] {
    let digest = base_state()
        .update(input)
        .update(nonce)
        .update(extra_nonce)
        .finalize();
    let b = digest.as_bytes();
    std::array::from_fn(|i| u64::from_le_bytes(b[i * 8..i * 8 + 8].try_into().expect("8 bytes")))
}

/// Beam's SipHash-2-4 variant: the state is used directly as `v0..v3`.
fn siphash24(k: &[u64; 4], nonce: u64) -> u64 {
    let [mut v0, mut v1, mut v2, mut v3] = *k;
    macro_rules! round {
        () => {
            v0 = v0.wrapping_add(v1);
            v2 = v2.wrapping_add(v3);
            v1 = v1.rotate_left(13);
            v3 = v3.rotate_left(16);
            v1 ^= v0;
            v3 ^= v2;
            v0 = v0.rotate_left(32);
            v2 = v2.wrapping_add(v1);
            v0 = v0.wrapping_add(v3);
            v1 = v1.rotate_left(17);
            v3 = v3.rotate_left(21);
            v1 ^= v2;
            v3 ^= v0;
            v2 = v2.rotate_left(32);
        };
    }
    v3 ^= nonce;
    round!();
    round!();
    v0 ^= nonce;
    v2 ^= 0xff;
    round!();
    round!();
    round!();
    round!();
    v0 ^ v1 ^ v2 ^ v3
}

/// Initial work bits for leaf `index`: word `i` is `siphash(8 * index + i)`.
fn leaf_work(k: &[u64; 4], index: u32) -> Work {
    std::array::from_fn(|i| siphash24(k, ((index as u64) << 3) + i as u64))
}

/// `(a ^ b) >> 24`, truncated to the low `rem_len` bits.
fn merge_work(a: &Work, b: &Work, rem_len: u32) -> Work {
    let x: Work = std::array::from_fn(|i| a[i] ^ b[i]);
    let s = COLLISION_BITS;
    let mut out: Work = std::array::from_fn(|i| {
        let hi = if i + 1 < WORK_WORDS {
            x[i + 1] << (64 - s)
        } else {
            0
        };
        (x[i] >> s) | hi
    });
    for (i, w) in out.iter_mut().enumerate() {
        let lo = i as u32 * 64;
        if lo >= rem_len {
            *w = 0;
        } else if rem_len - lo < 64 {
            *w &= (1u64 << (rem_len - lo)) - 1;
        }
    }
    out
}

/// Mix the first few indices of the subtree into the low 64 work bits.
///
/// The work bits are zero-extended to 512 bits, index `i` is ORed in at bit
/// `rem_len + 25 * i` (bits past 512 fall off), the eight words are rotated
/// and summed, and the sum replaces the lowest work word.
fn apply_mix(work: &mut Work, indices: &[u32], rem_len: u32) {
    let mut t = [0u64; 8];
    t[..WORK_WORDS].copy_from_slice(work);

    let pad = (((512 - rem_len) + COLLISION_BITS) / INDEX_BITS) as usize;
    for (i, &index) in indices.iter().take(pad).enumerate() {
        let pos = rem_len + i as u32 * INDEX_BITS;
        let (word, off) = ((pos / 64) as usize, pos % 64);
        let v = index as u64;
        t[word] |= v << off;
        if off + INDEX_BITS > 64 && word + 1 < t.len() {
            t[word + 1] |= v >> (64 - off);
        }
    }

    let mut sum = 0u64;
    for (i, w) in t.iter().enumerate() {
        sum = sum.wrapping_add(w.rotate_left((29 * (i as u32 + 1)) & 0x3f));
    }
    work[0] = sum.rotate_left(24);
}

fn collision_bits(work: &Work) -> u32 {
    (work[0] & ((1 << COLLISION_BITS) - 1)) as u32
}

/// Bits of work fed to the mix in `round` (1-based).
const fn mix_len(round: u32) -> u32 {
    let len = WORK_BITS - (round - 1) * COLLISION_BITS;
    if round == ROUNDS {
        len - 64
    } else {
        len
    }
}

/// Bits of work kept after merging in `round` (1-based).
const fn merged_len(round: u32) -> u32 {
    match round {
        ROUNDS => COLLISION_BITS,
        4 => WORK_BITS - 4 * COLLISION_BITS - 64,
        r => WORK_BITS - r * COLLISION_BITS,
    }
}

/// Unpack the 32 little-endian 25-bit indices from the first 100 bytes.
fn unpack_indices(packed: &[u8]) -> [u32; NUM_INDICES] {
    debug_assert_eq!(packed.len(), PACKED_INDICES_LEN);
    std::array::from_fn(|i| {
        let bit = i * INDEX_BITS as usize;
        let mut v = 0u64;
        for (k, byte) in packed[bit / 8..].iter().take(5).enumerate() {
            v |= (*byte as u64) << (8 * k);
        }
        ((v >> (bit % 8)) & ((1 << INDEX_BITS) - 1)) as u32
    })
}

/// Pack 32 indices into 100 bytes (inverse of [`unpack_indices`]).
fn pack_indices(indices: &[u32]) -> [u8; PACKED_INDICES_LEN] {
    debug_assert_eq!(indices.len(), NUM_INDICES);
    let mut out = [0u8; PACKED_INDICES_LEN];
    for (i, &index) in indices.iter().enumerate() {
        let bit = i * INDEX_BITS as usize;
        let v = ((index as u64) & ((1 << INDEX_BITS) - 1)) << (bit % 8);
        for (k, byte) in out[bit / 8..].iter_mut().take(5).enumerate() {
            *byte |= (v >> (8 * k)) as u8;
        }
    }
    out
}

/// Check that `solution` is a valid BeamHash III solution for `(input, nonce)`.
///
/// This checks the proof structure only; the caller checks difficulty
/// against [`solution_hash`].
pub fn verify_solution(
    input: &[u8],
    nonce: &[u8; NONCE_LEN],
    solution: &[u8],
) -> Result<(), Error> {
    if solution.len() != SOLUTION_LEN {
        return Err(Error::BadLength(solution.len()));
    }
    let keys = pre_pow(input, nonce, &solution[PACKED_INDICES_LEN..]);

    let mut level: Vec<(Work, Vec<u32>)> = unpack_indices(&solution[..PACKED_INDICES_LEN])
        .into_iter()
        .map(|i| (leaf_work(&keys, i), vec![i]))
        .collect();

    let mut round = 1;
    while level.len() > 1 {
        let mut next = Vec::with_capacity(level.len() / 2);
        for pair in level.chunks_exact_mut(2) {
            let [a, b] = pair else { unreachable!() };
            apply_mix(&mut a.0, &a.1, mix_len(round));
            apply_mix(&mut b.0, &b.1, mix_len(round));

            if collision_bits(&a.0) != collision_bits(&b.0) {
                return Err(Error::NoCollision(round));
            }
            if a.1.iter().any(|i| b.1.contains(i)) {
                return Err(Error::DuplicateIndex(round));
            }
            if a.1[0] >= b.1[0] {
                return Err(Error::BadOrder(round));
            }

            let mut indices = std::mem::take(&mut a.1);
            indices.extend_from_slice(&b.1);
            next.push((merge_work(&a.0, &b.0, merged_len(round)), indices));
        }
        level = next;
        round += 1;
    }

    if level[0].0.iter().all(|w| *w == 0) {
        Ok(())
    } else {
        Err(Error::NonZeroResult)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_lengths_match_reference() {
        // (mix, merged) per round, from IsValidSolution / OptimisedSolve.
        let expected = [(448, 424), (424, 400), (400, 376), (376, 288), (288, 24)];
        for (r, (mix, merged)) in (1..=ROUNDS).zip(expected) {
            assert_eq!((mix_len(r), merged_len(r)), (mix, merged), "round {r}");
        }
    }

    #[test]
    fn index_packing_round_trips() {
        let indices: Vec<u32> = (0..32u32)
            .map(|i| (i * 0x0012_3457 + 0x1ff_ffff) & 0x1ff_ffff)
            .collect();
        let packed = pack_indices(&indices);
        assert_eq!(unpack_indices(&packed).to_vec(), indices);
        // Every bit of the 800-bit stream is used, so all-ones round-trips too.
        let ones = [0x1ff_ffffu32; 32];
        assert_eq!(pack_indices(&ones), [0xff; 100]);
    }

    #[test]
    fn merge_shifts_across_words_and_masks() {
        let a: Work = [0, 1, 0, 0, 0, 0, 0];
        let b: Work = [0; 7];
        // Bit 64 moves down to bit 40.
        assert_eq!(merge_work(&a, &b, 424), [1 << 40, 0, 0, 0, 0, 0, 0]);
        // Masking to 24 bits drops it.
        assert_eq!(merge_work(&a, &b, 24), [0; 7]);
    }

    #[test]
    fn rejects_wrong_length() {
        assert_eq!(
            verify_solution(&[0; 32], &[0; 8], &[0; 103]),
            Err(Error::BadLength(103))
        );
    }

    #[test]
    fn rejects_all_zero_solution() {
        // Every index is 0, so round 1 siblings share an index (or fail to collide).
        assert!(verify_solution(&[0; 32], &[0; 8], &[0; SOLUTION_LEN]).is_err());
    }
}
