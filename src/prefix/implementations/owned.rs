//! Consuming implementations for owned ranked-prefix storage.

use num_bigint::BigUint;

use super::{online_balanced::push_balanced, widening_u128};
use crate::{BigDyadic, RankedPrefix};

#[derive(Clone, Copy, Debug)]
pub struct Candidate {
    pub name: &'static str,
    implementation: fn(Box<RankedPrefix>) -> BigDyadic,
}

impl Candidate {
    const fn new(name: &'static str, implementation: fn(Box<RankedPrefix>) -> BigDyadic) -> Self {
        Self {
            name,
            implementation,
        }
    }

    pub fn compute(self, prefix: Box<RankedPrefix>) -> BigDyadic {
        (self.implementation)(prefix)
    }
}

pub const BORROWED_BASELINE: Candidate = Candidate::new("borrowed-baseline", borrowed_baseline);
pub const REUSED_BUFFER: Candidate = Candidate::new("reused-buffer", reused_buffer);
pub const CANDIDATES: &[Candidate] = &[BORROWED_BASELINE, REUSED_BUFFER];

/// Selected consuming implementation.
pub use borrowed_baseline as compute;

pub fn borrowed_baseline(prefix: Box<RankedPrefix>) -> BigDyadic {
    super::compute(&prefix)
}

pub fn reused_buffer(prefix: Box<RankedPrefix>) -> BigDyadic {
    let mut bytes = RankedPrefix::into_boxed_slice(prefix);
    let mut fractional_bits = 0usize;
    let mut levels = Vec::new();

    let mut chunks = bytes.chunks_exact_mut(16);
    for chunk in &mut chunks {
        let product = reduce_block_in_place(chunk, &mut fractional_bits);
        if product != 1 {
            push_balanced(&mut levels, BigUint::from(product));
        }
    }
    let remainder = chunks.into_remainder();
    if !remainder.is_empty() {
        let product = widening_u128::reduce_chunk(remainder, &mut fractional_bits);
        if product != 1 {
            push_balanced(&mut levels, BigUint::from(product));
        }
    }

    let numerator = levels
        .into_iter()
        .flatten()
        .fold(BigUint::from(1u8), |left, right| left * right);
    BigDyadic::from_scaled(numerator, fractional_bits)
}

fn reduce_block_in_place(block: &mut [u8], fractional_bits: &mut usize) -> u128 {
    debug_assert_eq!(block.len(), 16);
    for byte in block.iter_mut() {
        let factor = 256u16 - u16::from(*byte);
        let removable = factor.trailing_zeros().min(8) as usize;
        *fractional_bits = fractional_bits
            .checked_add(8 - removable)
            .expect("RankedPrefix precision overflowed usize");
        *byte = (factor >> removable) as u8;
    }
    for pair in 0..8 {
        let offset = pair * 2;
        let value = u16::from(block[offset]) * u16::from(block[offset + 1]);
        block[offset..offset + 2].copy_from_slice(&value.to_le_bytes());
    }
    for pair in 0..4 {
        let offset = pair * 4;
        let left = u16::from_le_bytes(block[offset..offset + 2].try_into().unwrap());
        let right = u16::from_le_bytes(block[offset + 2..offset + 4].try_into().unwrap());
        block[offset..offset + 4]
            .copy_from_slice(&(u32::from(left) * u32::from(right)).to_le_bytes());
    }
    for pair in 0..2 {
        let offset = pair * 8;
        let left = u32::from_le_bytes(block[offset..offset + 4].try_into().unwrap());
        let right = u32::from_le_bytes(block[offset + 4..offset + 8].try_into().unwrap());
        block[offset..offset + 8]
            .copy_from_slice(&(u64::from(left) * u64::from(right)).to_le_bytes());
    }
    let left = u64::from_le_bytes(block[..8].try_into().unwrap());
    let right = u64::from_le_bytes(block[8..].try_into().unwrap());
    let product = u128::from(left) * u128::from(right);
    block.copy_from_slice(&product.to_le_bytes());
    product
}
