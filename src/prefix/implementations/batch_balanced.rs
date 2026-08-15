//! Round-based pairwise multiplication tree.

use num_bigint::BigUint;

use crate::{BigDyadic, RankedPrefix};

pub fn compute(prefix: &RankedPrefix) -> BigDyadic {
    if prefix.is_empty() {
        return BigDyadic::one();
    }
    let mut fractional_bits = 0usize;
    let mut products = Vec::with_capacity(prefix.len().div_ceil(8));
    for chunk in prefix.as_slice().chunks(8) {
        let mut product = 1u64;
        for &raw in chunk {
            let factor = 256u16 - raw as u16;
            let removable = factor.trailing_zeros().min(8) as usize;
            fractional_bits = fractional_bits
                .checked_add(8 - removable)
                .expect("RankedPrefix precision overflowed usize");
            product *= (factor >> removable) as u64;
        }
        if product != 1 {
            products.push(BigUint::from(product));
        }
    }
    while products.len() > 1 {
        let mut next = Vec::with_capacity(products.len().div_ceil(2));
        let mut current = products.into_iter();
        while let Some(left) = current.next() {
            next.push(current.next().map_or(left.clone(), |right| left * right));
        }
        products = next;
    }
    BigDyadic::from_scaled(
        products.pop().unwrap_or_else(|| BigUint::from(1u8)),
        fractional_bits,
    )
}
