//! Agreement with Beam's reference BeamHash III code.
//!
//! The fixture tests always run. `verdicts_match_reference` also runs every
//! case through Beam's C++ verifier when `BEAM3REF` names a binary built by
//! `reference/build.sh`, and is skipped otherwise.

use std::io::Write;
use std::process::{Command, Stdio};

use cyphes_pow::{verify_solution, SOLUTION_LEN};

#[derive(Clone)]
struct Case {
    input: Vec<u8>,
    nonce: [u8; 8],
    solution: Vec<u8>,
}

fn vectors() -> Vec<Case> {
    include_str!("vectors.txt")
        .lines()
        .filter(|l| !l.starts_with('#') && !l.trim().is_empty())
        .map(|l| {
            let f: Vec<_> = l
                .split_whitespace()
                .map(|h| hex::decode(h).expect("hex"))
                .collect();
            Case {
                input: f[0].clone(),
                nonce: f[1].clone().try_into().expect("8-byte nonce"),
                solution: f[2].clone(),
            }
        })
        .collect()
}

fn unpack(solution: &[u8]) -> Vec<u32> {
    (0..32)
        .map(|i| {
            let bit = i * 25;
            let v = (0..5)
                .filter(|k| bit / 8 + k < 100)
                .fold(0u64, |v, k| v | (solution[bit / 8 + k] as u64) << (8 * k));
            ((v >> (bit % 8)) & 0x1ff_ffff) as u32
        })
        .collect()
}

fn pack(indices: &[u32], extra_nonce: &[u8]) -> Vec<u8> {
    let mut out = vec![0u8; SOLUTION_LEN];
    for (i, &index) in indices.iter().enumerate() {
        let bit = i * 25;
        let v = (index as u64) << (bit % 8);
        for k in (0..5).filter(|k| bit / 8 + k < 100) {
            out[bit / 8 + k] |= (v >> (8 * k)) as u8;
        }
    }
    out[100..].copy_from_slice(extra_nonce);
    out
}

/// Near misses derived from a valid case: every single-bit flip of the
/// solution, nonce and input, swapped sibling subtrees at each level, and
/// truncated or extended solutions.
fn mutations(c: &Case) -> Vec<Case> {
    let mut out = Vec::new();
    for bit in 0..SOLUTION_LEN * 8 {
        let mut m = c.clone();
        m.solution[bit / 8] ^= 1 << (bit % 8);
        out.push(m);
    }
    for bit in 0..64 {
        let mut m = c.clone();
        m.nonce[bit / 8] ^= 1 << (bit % 8);
        out.push(m);
    }
    for bit in 0..c.input.len() * 8 {
        let mut m = c.clone();
        m.input[bit / 8] ^= 1 << (bit % 8);
        out.push(m);
    }
    let indices = unpack(&c.solution);
    assert_eq!(
        pack(&indices, &c.solution[100..]),
        c.solution,
        "test packing helper"
    );
    for width in [1, 2, 4, 8, 16] {
        let mut swapped = indices.clone();
        for pair in swapped.chunks_mut(2 * width) {
            pair.rotate_left(width);
        }
        out.push(Case {
            solution: pack(&swapped, &c.solution[100..]),
            ..c.clone()
        });
    }
    out.push(Case {
        solution: c.solution[..103].to_vec(),
        ..c.clone()
    });
    let mut longer = c.solution.clone();
    longer.push(0);
    out.push(Case {
        solution: longer,
        ..c.clone()
    });
    out
}

#[test]
fn fixtures_verify() {
    let vs = vectors();
    assert!(vs.len() >= 10, "expected the committed vector set");
    for (i, v) in vs.iter().enumerate() {
        verify_solution(&v.input, &v.nonce, &v.solution)
            .unwrap_or_else(|e| panic!("vector {i}: {e}"));
    }
}

#[test]
fn every_mutation_is_rejected() {
    for v in vectors() {
        for (i, m) in mutations(&v).iter().enumerate() {
            assert!(
                verify_solution(&m.input, &m.nonce, &m.solution).is_err(),
                "mutation {i} accepted"
            );
        }
    }
}

#[test]
fn verdicts_match_reference() {
    let Some(bin) = std::env::var_os("BEAM3REF") else {
        eprintln!("BEAM3REF not set; skipping differential run against Beam's C++ verifier");
        return;
    };

    let mut cases = Vec::new();
    for v in vectors() {
        cases.extend(mutations(&v));
        cases.push(v);
    }

    let mut child = Command::new(bin)
        .arg("verify")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .expect("run BEAM3REF");
    {
        let mut stdin = child.stdin.take().expect("stdin");
        for c in &cases {
            // The harness reads fixed-size tokens; an empty solution would desync it.
            writeln!(
                stdin,
                "{} {} {}",
                hex::encode(&c.input),
                hex::encode(c.nonce),
                hex::encode(&c.solution)
            )
            .expect("write case");
        }
    }
    let output = child.wait_with_output().expect("BEAM3REF output");
    let reference: Vec<bool> = String::from_utf8(output.stdout)
        .expect("utf8")
        .lines()
        .map(|l| l == "1")
        .collect();
    assert_eq!(
        reference.len(),
        cases.len(),
        "reference answered every case"
    );

    let accepted = reference.iter().filter(|r| **r).count();
    for (i, (c, want)) in cases.iter().zip(&reference).enumerate() {
        let got = verify_solution(&c.input, &c.nonce, &c.solution).is_ok();
        assert_eq!(got, *want, "case {i}: rust={got} beam={want}");
    }
    eprintln!(
        "{} cases agree with Beam's verifier ({accepted} valid)",
        cases.len()
    );
}
