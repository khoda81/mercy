//! Online binary-counter multiplication tree.

use num_bigint::BigUint;

use crate::{BigDyadic, RankedPrefix};

pub fn compute(prefix: &RankedPrefix) -> BigDyadic {
    if prefix.is_empty() {
        return BigDyadic::one();
    }

    let mut fractional_bits = 0usize;
    let mut levels: Vec<Option<BigUint>> = Vec::new();

    for chunk in prefix.as_slice().chunks(8) {
        let mut product = 1u64;
        for &raw in chunk {
            let factor = 256u16 - raw as u16;
            let removable = factor.trailing_zeros().min(8) as usize;
            let odd = (factor >> removable) as u64;
            fractional_bits = fractional_bits
                .checked_add(8 - removable)
                .expect("RankedPrefix precision overflowed usize");
            product *= odd;
        }
        if product != 1 {
            push_balanced(&mut levels, BigUint::from(product));
        }
    }

    let numerator = levels
        .into_iter()
        .flatten()
        .fold(BigUint::from(1u8), |acc, value| acc * value);
    BigDyadic::from_scaled(numerator, fractional_bits)
}

pub(super) fn push_balanced(levels: &mut Vec<Option<BigUint>>, mut value: BigUint) {
    let mut level = 0;
    loop {
        if level == levels.len() {
            levels.push(Some(value));
            return;
        }
        match levels[level].take() {
            None => {
                levels[level] = Some(value);
                return;
            }
            Some(other) => {
                value *= other;
                level += 1;
            }
        }
    }
}
