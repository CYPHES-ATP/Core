//! BeamHash III proof of work for CYPHES block headers.
//!
//! Replaces Zcash's Equihash (200, 9). The header's PoW input is BLAKE2b-256
//! of every header field before the nonce, personalised `CYPHES_PowHeader`;
//! this 32-byte value is exactly what a stratum job hands to a miner. The
//! nonce is 8 bytes and the solution 104 bytes, as in Beam, so existing
//! BeamHash III GPU miners work unmodified.
//!
//! A header's proof of work is valid when:
//!
//! 1. `solution` is a valid BeamHash III solution for `(pow_input, nonce)`, and
//! 2. `SHA-256(solution)`, read as a big-endian number, is at most the
//!    target encoded in `difficulty_threshold`.

use std::{fmt, io};

use hex::{FromHex, FromHexError, ToHex};
use serde_big_array::BigArray;

use crate::{
    block::Header,
    serialization::{SerializationError, ZcashDeserialize, ZcashSerialize},
    work::difficulty::{ExpandedDifficulty, U256},
};

#[cfg(feature = "internal-miner")]
use crate::serialization::AtLeastOne;

pub use cyphes_pow::{NONCE_LEN, SOLUTION_LEN};

/// BLAKE2b personalization for the header PoW input.
const POW_INPUT_PERSONALIZATION: &[u8; 16] = b"CYPHES_PowHeader";

/// The error type for BeamHash III validation.
#[non_exhaustive]
#[derive(Debug, thiserror::Error)]
#[error("invalid BeamHash III solution for block header: {0}")]
pub struct Error(#[from] cyphes_pow::Error);

/// The error type for BeamHash III solving.
#[derive(Copy, Clone, Debug, Eq, PartialEq, thiserror::Error)]
#[error("solver was cancelled")]
pub struct SolverCancelled;

/// A BeamHash III solution: 32 packed 25-bit indices and a 4-byte extra nonce.
#[derive(Copy, Clone, Eq, PartialEq, Deserialize, Serialize)]
pub struct Solution(#[serde(with = "BigArray")] pub [u8; SOLUTION_LEN]);

impl Solution {
    /// The length of the header prefix hashed into the PoW input: every field
    /// before the nonce.
    pub const INPUT_LENGTH: usize = 4 + 32 * 3 + 4 * 2;

    /// The serialized size of a solution, in bytes. It has no length prefix.
    pub const SERIALIZED_SIZE: usize = SOLUTION_LEN;

    /// The 32-byte PoW input for `header`, as sent to miners.
    #[allow(clippy::unwrap_in_result)]
    pub fn pow_input(header: &Header) -> [u8; 32] {
        let mut bytes = Vec::with_capacity(Header::SERIALIZED_SIZE);
        header
            .zcash_serialize(&mut bytes)
            .expect("serialization into a vec can't fail");
        blake2b_simd::Params::new()
            .hash_length(32)
            .personal(POW_INPUT_PERSONALIZATION)
            .hash(&bytes[..Self::INPUT_LENGTH])
            .as_bytes()
            .try_into()
            .expect("hash length is 32")
    }

    /// Returns `Ok(())` if this is a valid BeamHash III solution for `header`.
    ///
    /// This checks the proof structure; see [`Solution::meets_threshold`] for
    /// the difficulty check.
    pub fn check(&self, header: &Header) -> Result<(), Error> {
        cyphes_pow::verify_solution(&Self::pow_input(header), &header.nonce, &self.0)?;
        Ok(())
    }

    /// `SHA-256(solution)` as a big-endian number: the value compared against
    /// the difficulty threshold. Smaller values represent more work.
    pub fn pow_value(&self) -> ExpandedDifficulty {
        U256::from_big_endian(&cyphes_pow::solution_hash(&self.0)).into()
    }

    /// Returns `true` if this solution's [`pow_value`](Self::pow_value) is at
    /// most `threshold`.
    pub fn meets_threshold(&self, threshold: ExpandedDifficulty) -> bool {
        self.pow_value() <= threshold
    }

    /// Returns a [`Solution`] containing the bytes from `solution`.
    /// Returns an error if `solution` is the wrong length.
    pub fn from_bytes(solution: &[u8]) -> Result<Self, SerializationError> {
        solution
            .try_into()
            .map(Self)
            .map_err(|_| SerializationError::Parse("incorrect BeamHash III solution size"))
    }

    /// Returns an all-zero [`Solution`] to be used in block proposals.
    pub fn for_proposal() -> Self {
        Self([0; SOLUTION_LEN])
    }

    /// Mines and returns one or more headers based on a template `header`,
    /// each with a valid `nonce` and `solution`.
    ///
    /// If `cancel_fn()` returns an error, returns early with `Err(SolverCancelled)`.
    /// It is checked between nonces; one nonce takes seconds.
    ///
    /// The `nonce` in the header template is the starting nonce. If you run
    /// several solvers, start them with different nonces. The template's
    /// `solution` is ignored.
    ///
    /// This is the reference CPU solver: it uses all cores and ~8 GiB of RAM
    /// per nonce. It exists for devnets; real mining uses GPU miners.
    #[cfg(feature = "internal-miner")]
    pub fn solve<F>(
        mut header: Header,
        mut cancel_fn: F,
    ) -> Result<AtLeastOne<Header>, SolverCancelled>
    where
        F: FnMut() -> Result<(), SolverCancelled>,
    {
        use crate::shutdown::is_shutting_down;

        let threshold = header
            .difficulty_threshold
            .to_expanded()
            .expect("unexpected invalid header template: invalid difficulty threshold");
        let input = Self::pow_input(&header);

        while !is_shutting_down() {
            cancel_fn()?;

            let valid: Vec<Header> = cyphes_pow::solver::solve(&input, &header.nonce, [0; 4])
                .into_iter()
                .map(|solution| Header {
                    solution: Self(solution),
                    ..header
                })
                .filter(|h| h.solution.meets_threshold(threshold))
                .collect();

            if let Ok(at_least_one) = valid.try_into() {
                return Ok(at_least_one);
            }

            let _ignore_overflow =
                crate::primitives::byte_array::increment_big_endian(&mut header.nonce[..]);
        }

        Err(SolverCancelled)
    }
}

impl fmt::Debug for Solution {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.debug_tuple("BeamHashSolution")
            .field(&hex::encode(self.0))
            .finish()
    }
}

#[cfg(any(test, feature = "proptest-impl"))]
impl Default for Solution {
    fn default() -> Self {
        Self::for_proposal()
    }
}

impl ZcashSerialize for Solution {
    fn zcash_serialize<W: io::Write>(&self, mut writer: W) -> Result<(), io::Error> {
        writer.write_all(&self.0)
    }
}

impl ZcashDeserialize for Solution {
    fn zcash_deserialize<R: io::Read>(mut reader: R) -> Result<Self, SerializationError> {
        let mut bytes = [0; SOLUTION_LEN];
        reader.read_exact(&mut bytes)?;
        Ok(Self(bytes))
    }
}

impl ToHex for &Solution {
    fn encode_hex<T: FromIterator<char>>(&self) -> T {
        self.0.encode_hex()
    }

    fn encode_hex_upper<T: FromIterator<char>>(&self) -> T {
        self.0.encode_hex_upper()
    }
}

impl ToHex for Solution {
    fn encode_hex<T: FromIterator<char>>(&self) -> T {
        (&self).encode_hex()
    }

    fn encode_hex_upper<T: FromIterator<char>>(&self) -> T {
        (&self).encode_hex_upper()
    }
}

impl FromHex for Solution {
    type Error = FromHexError;

    fn from_hex<T: AsRef<[u8]>>(hex: T) -> Result<Self, Self::Error> {
        let bytes = Vec::from_hex(hex)?;
        Solution::from_bytes(&bytes).map_err(|_| FromHexError::InvalidStringLength)
    }
}
