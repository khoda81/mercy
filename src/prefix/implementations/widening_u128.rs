//! Pairwise 16-factor widening tree ending in `u128`.

use num_bigint::BigUint;

use super::online_balanced::push_balanced;
use crate::{BigDyadic, RankedPrefix};

pub fn compute(prefix: &RankedPrefix) -> BigDyadic {
    let mut fractional_bits = 0usize;
    let mut levels = Vec::new();
    for chunk in prefix.as_slice().chunks(16) {
        let product = reduce_chunk(chunk, &mut fractional_bits);
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

pub(super) fn reduce_chunk(chunk: &[u8], fractional_bits: &mut usize) -> u128 {
    let mut u8ish = [1u16; 16];
    for (slot, &raw) in u8ish.iter_mut().zip(chunk) {
        let factor = 256u16 - u16::from(raw);
        let removable = factor.trailing_zeros().min(8) as usize;
        *fractional_bits = fractional_bits
            .checked_add(8 - removable)
            .expect("RankedPrefix precision overflowed usize");
        *slot = factor >> removable;
    }
    let mut u16s = [1u32; 8];
    for (output, input) in u16s.iter_mut().zip(u8ish.chunks_exact(2)) {
        *output = u32::from(input[0]) * u32::from(input[1]);
    }
    let mut u32s = [1u64; 4];
    for (output, input) in u32s.iter_mut().zip(u16s.chunks_exact(2)) {
        *output = u64::from(input[0]) * u64::from(input[1]);
    }
    let mut u64s = [1u128; 2];
    for (output, input) in u64s.iter_mut().zip(u32s.chunks_exact(2)) {
        *output = u128::from(input[0]) * u128::from(input[1]);
    }
    u64s[0] * u64s[1]
}
