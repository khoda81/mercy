//! Repeated odd factors reduced with histogram powers.

use num_bigint::BigUint;

use super::histogram::{pow, product_tree, reduced_histogram};
use crate::{BigDyadic, RankedPrefix};

pub fn compute(prefix: &RankedPrefix) -> BigDyadic {
    let mut odd_counts = [0usize; 256];
    let fractional_bits = reduced_histogram(prefix, &mut odd_counts);
    let mut powers = Vec::new();
    for (odd, &count) in odd_counts.iter().enumerate().skip(3).step_by(2) {
        if count != 0 {
            powers.push(pow(BigUint::from(odd), count));
        }
    }
    BigDyadic::from_reduced_odd(product_tree(powers), fractional_bits)
}
