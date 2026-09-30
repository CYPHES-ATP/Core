//! Reference CPU solver for BeamHash III.
//!
//! Same algorithm as Beam's `BeamHash_III::OptimisedSolve`: seed 2^25 leaves,
//! then per round mix, sort by the low 24 bits, and merge every colliding pair.
//! It is parallel but not memory-optimised (peak ~8 GiB), which is fine for a
//! devnet and for tests. Real mining uses GPU miners over stratum.

use rayon::prelude::*;

use crate::{
    apply_mix, collision_bits, leaf_work, merge_work, merged_len, mix_len, pack_indices, pre_pow,
    verify_solution, Work, INDEX_BITS, NONCE_LEN, PACKED_INDICES_LEN, ROUNDS, SOLUTION_LEN,
};

/// Every valid solution for `(input, nonce, extra_nonce)`, sorted.
///
/// Candidates are re-checked with [`verify_solution`], so trivial solutions
/// with repeated indices (which the Beam solver can emit) are dropped.
pub fn solve(
    input: &[u8],
    nonce: &[u8; NONCE_LEN],
    extra_nonce: [u8; 4],
) -> Vec<[u8; SOLUTION_LEN]> {
    let keys = pre_pow(input, nonce, &extra_nonce);
    let leaves = 1u32 << INDEX_BITS;

    let mut work: Vec<Work> = (0..leaves)
        .into_par_iter()
        .map(|i| leaf_work(&keys, i))
        .collect();
    let mut indices: Vec<u32> = (0..leaves).collect();
    let mut width = 1;

    for round in 1..ROUNDS {
        mix(&mut work, &indices, width, round);
        let (order, groups) = collision_groups(&work);
        (work, indices) = merge_groups(&work, &indices, width, &order, &groups, merged_len(round));
        width *= 2;
    }

    mix(&mut work, &indices, width, ROUNDS);
    let (order, groups) = collision_groups(&work);
    let mut solutions: Vec<[u8; SOLUTION_LEN]> = groups
        .par_iter()
        .flat_map_iter(|&(start, end)| pairs(&order, start, end))
        .filter_map(|(a, b)| {
            if merge_work(&work[a], &work[b], merged_len(ROUNDS))
                .iter()
                .any(|w| *w != 0)
            {
                return None;
            }
            let mut leaf_indices = Vec::with_capacity(2 * width);
            let (a, b) = canonical(&indices, width, a, b);
            leaf_indices.extend_from_slice(&indices[a * width..][..width]);
            leaf_indices.extend_from_slice(&indices[b * width..][..width]);

            let mut solution = [0u8; SOLUTION_LEN];
            solution[..PACKED_INDICES_LEN].copy_from_slice(&pack_indices(&leaf_indices));
            solution[PACKED_INDICES_LEN..].copy_from_slice(&extra_nonce);
            verify_solution(input, nonce, &solution)
                .ok()
                .map(|()| solution)
        })
        .collect();
    solutions.sort_unstable();
    solutions.dedup();
    solutions
}

fn mix(work: &mut [Work], indices: &[u32], width: usize, round: u32) {
    work.par_iter_mut()
        .zip(indices.par_chunks(width))
        .for_each(|(w, ix)| apply_mix(w, ix, mix_len(round)));
}

/// Element positions sorted by collision bits, and the `[start, end)` runs of
/// that order holding two or more elements with equal collision bits.
fn collision_groups(work: &[Work]) -> (Vec<u32>, Vec<(u32, u32)>) {
    let mut keyed: Vec<u64> = work
        .par_iter()
        .enumerate()
        .map(|(i, w)| ((collision_bits(w) as u64) << 32) | i as u64)
        .collect();
    keyed.par_sort_unstable();

    let mut groups = Vec::new();
    let mut start = 0;
    for i in 1..=keyed.len() {
        if i == keyed.len() || keyed[i] >> 32 != keyed[start] >> 32 {
            if i - start >= 2 {
                groups.push((start as u32, i as u32));
            }
            start = i;
        }
    }
    (keyed.into_iter().map(|k| k as u32).collect(), groups)
}

fn pairs(order: &[u32], start: u32, end: u32) -> impl Iterator<Item = (usize, usize)> + '_ {
    (start..end).flat_map(move |x| {
        ((x + 1)..end).map(move |y| (order[x as usize] as usize, order[y as usize] as usize))
    })
}

/// Put the subtree with the smaller first index first, as Beam's merge does.
fn canonical(indices: &[u32], width: usize, a: usize, b: usize) -> (usize, usize) {
    if indices[a * width] < indices[b * width] {
        (a, b)
    } else {
        (b, a)
    }
}

/// Merge every colliding pair into the next round's (work, indices) arrays.
fn merge_groups(
    work: &[Work],
    indices: &[u32],
    width: usize,
    order: &[u32],
    groups: &[(u32, u32)],
    rem_len: u32,
) -> (Vec<Work>, Vec<u32>) {
    let chunk = groups
        .len()
        .div_ceil(rayon::current_num_threads() * 16)
        .max(1);
    let counts: Vec<usize> = groups
        .par_chunks(chunk)
        .map(|gs| {
            gs.iter()
                .map(|&(s, e)| ((e - s) as usize) * ((e - s) as usize - 1) / 2)
                .sum()
        })
        .collect();
    let total: usize = counts.iter().sum();

    let mut next_work = vec![[0u64; 7]; total];
    let mut next_indices = vec![0u32; total * 2 * width];

    // Carve the outputs into one disjoint slice per chunk of groups.
    let mut work_parts = Vec::with_capacity(counts.len());
    let mut index_parts = Vec::with_capacity(counts.len());
    let (mut w_rest, mut i_rest) = (next_work.as_mut_slice(), next_indices.as_mut_slice());
    for &n in &counts {
        let (w, wr) = w_rest.split_at_mut(n);
        let (i, ir) = i_rest.split_at_mut(n * 2 * width);
        work_parts.push(w);
        index_parts.push(i);
        (w_rest, i_rest) = (wr, ir);
    }

    groups
        .par_chunks(chunk)
        .zip(work_parts.into_par_iter().zip(index_parts.into_par_iter()))
        .for_each(|(gs, (out_w, out_i))| {
            let mut k = 0;
            for &(s, e) in gs {
                for (a, b) in pairs(order, s, e) {
                    let (a, b) = canonical(indices, width, a, b);
                    out_w[k] = merge_work(&work[a], &work[b], rem_len);
                    let dst = &mut out_i[k * 2 * width..][..2 * width];
                    dst[..width].copy_from_slice(&indices[a * width..][..width]);
                    dst[width..].copy_from_slice(&indices[b * width..][..width]);
                    k += 1;
                }
            }
        });

    (next_work, next_indices)
}
