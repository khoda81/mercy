//! Global prime-exponent reduction of repeated odd factors.

use num_bigint::BigUint;

use super::histogram::{pow, product_tree, reduced_histogram, ODD_FACTORIZATIONS, ODD_PRIMES};
use crate::{BigDyadic, RankedPrefix};

pub fn compute(prefix: &RankedPrefix) -> BigDyadic {
    let mut odd_counts = [0usize; 256];
    let fractional_bits = reduced_histogram(prefix, &mut odd_counts);
    let mut prime_exponents = [0usize; ODD_PRIMES.len()];

    for (odd, &count) in odd_counts.iter().enumerate().skip(3).step_by(2) {
        if count == 0 {
            continue;
        }
        for factor in ODD_FACTORIZATIONS[odd] {
            if factor.exponent == 0 {
                break;
            }
            let total = &mut prime_exponents[usize::from(factor.prime_index)];
            *total = (*total)
                .checked_add(
                    usize::from(factor.exponent)
                        .checked_mul(count)
                        .expect("prime exponent overflowed usize"),
                )
                .expect("prime exponent overflowed usize");
        }
    }

    let powers = ODD_PRIMES
        .into_iter()
        .zip(prime_exponents)
        .filter(|(_, exponent)| *exponent != 0)
        .map(|(prime, exponent)| pow(BigUint::from(prime), exponent))
        .collect();
    BigDyadic::from_reduced_odd(product_tree(powers), fractional_bits)
}
