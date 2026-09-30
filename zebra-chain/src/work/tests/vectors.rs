//! Fixed test vectors for BeamHash III block headers.

use crate::{
    block::Header,
    serialization::{ZcashDeserializeInto, ZcashSerialize},
    work::beamhash::{Solution, SOLUTION_LEN},
};

/// A CYPHES header solved by `cyphes-pow`'s solver, whose solution meets the
/// regtest target (`0x207fffff`). Fields: version 4, previous block,
/// merkle root and commitments are SHA-256 of fixed strings, time
/// 2026-09-30T00:00:00Z, nonce 0.
const SOLVED_HEADER: &str = "0400000053af3eb579b01d521a36802061897d200f93f7dc88addfba37f26ecf3445f4e98386bb97613d52c653e5683bdabc377e615b67d98116792e65b7b37dbd50136b1f7fd4e70c95b7cf5d542f1c37128f14a96fada305bb7be15565c63c9da849880051bc6affff7f200000000000000000522208640f929b6e004d4fc60d8900ebde0ef2e74c6d2729fd44f02cf8bd1ec5b1868bce553b682d23e43ddeb5941fd9ee4c715d163e95f2b46d2c1b549efa151166fb763a166f1eb415da05ce825eb9a09556f85469a3deed23ef658c7b1eb4dead6acd00000000";

/// BLAKE2b-256 of the first 108 header bytes, personalised
/// `CYPHES_PowHeader`, computed independently with Python's hashlib.
const SOLVED_HEADER_POW_INPUT: &str =
    "90544b721537db764fdbacca9dc81ae6eea1909114a9bdc24c0108c814eb6016";

fn solved_header() -> Header {
    hex::decode(SOLVED_HEADER)
        .expect("vector is hex")
        .zcash_deserialize_into()
        .expect("vector deserializes")
}

#[test]
fn header_vector_roundtrips_at_fixed_size() {
    let _init_guard = zebra_test::init();

    let bytes = hex::decode(SOLVED_HEADER).expect("vector is hex");
    assert_eq!(bytes.len(), Header::SERIALIZED_SIZE);
    assert_eq!(Header::SERIALIZED_SIZE, 220);
    assert_eq!(
        solved_header()
            .zcash_serialize_to_vec()
            .expect("serializes"),
        bytes
    );
}

#[test]
fn pow_input_matches_independent_computation() {
    let _init_guard = zebra_test::init();

    assert_eq!(
        hex::encode(Solution::pow_input(&solved_header())),
        SOLVED_HEADER_POW_INPUT
    );
}

#[test]
fn solved_header_passes_both_pow_checks() {
    let _init_guard = zebra_test::init();

    let header = solved_header();
    header.solution.check(&header).expect("solution is valid");
    let threshold = header
        .difficulty_threshold
        .to_expanded()
        .expect("regtest threshold is valid");
    assert!(header.solution.meets_threshold(threshold));
}

#[test]
fn any_changed_field_invalidates_the_solution() {
    let _init_guard = zebra_test::init();

    let real = solved_header();
    let mut changed = Vec::new();

    let mut h = real;
    h.version += 1;
    changed.push(h);
    let mut h = real;
    h.previous_block_hash.0[0] ^= 1;
    changed.push(h);
    let mut h = real;
    h.merkle_root.0[31] ^= 1;
    changed.push(h);
    let mut h = real;
    h.commitment_bytes.0[7] ^= 1;
    changed.push(h);
    let mut h = real;
    h.time += chrono::Duration::seconds(1);
    changed.push(h);
    let mut h = real;
    h.difficulty_threshold.0 -= 1;
    changed.push(h);
    let mut h = real;
    h.nonce.0[7] ^= 1;
    changed.push(h);
    let mut h = real;
    h.solution.0[50] ^= 0x10;
    changed.push(h);

    for (i, header) in changed.iter().enumerate() {
        assert!(
            header.solution.check(header).is_err(),
            "change {i} still validates"
        );
    }
}

#[test]
fn solution_size_is_exact() {
    let _init_guard = zebra_test::init();

    for len in [0, 1, SOLUTION_LEN - 1, SOLUTION_LEN + 1, 1344] {
        assert!(Solution::from_bytes(&vec![0; len]).is_err(), "length {len}");
    }
    assert!(Solution::from_bytes(&[0; SOLUTION_LEN]).is_ok());

    // A truncated header fails to deserialize instead of reading past the end.
    let bytes = hex::decode(SOLVED_HEADER).expect("vector is hex");
    let truncated: Result<Header, _> = bytes[..bytes.len() - 1].zcash_deserialize_into();
    assert!(truncated.is_err());
}
