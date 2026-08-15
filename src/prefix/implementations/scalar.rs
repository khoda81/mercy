//! Sequential exact correctness reference.

use num_bigint::BigUint;

use crate::{BigDyadic, RankedPrefix};

pub fn compute(prefix: &RankedPrefix) -> BigDyadic {
    let mut numerator = BigUint::from(1u8);
    let mut fractional_bits = 0usize;
    for &raw in prefix.as_slice() {
        let factor = 256u16 - raw as u16;
        let removable = factor.trailing_zeros().min(8) as usize;
        numerator *= BigUint::from(factor >> removable);
        fractional_bits = fractional_bits
            .checked_add(8 - removable)
            .expect("RankedPrefix precision overflowed usize");
    }
    BigDyadic::from_scaled(numerator, fractional_bits)
}
