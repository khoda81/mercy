//! Shared exact primitives for histogram-based tail products.

use num_bigint::BigUint;

use crate::RankedPrefix;

pub(super) const ODD_PRIMES: [u16; 53] = [
    3, 5, 7, 11, 13, 17, 19, 23, 29, 31, 37, 41, 43, 47, 53, 59, 61, 67, 71, 73, 79, 83, 89, 97,
    101, 103, 107, 109, 113, 127, 131, 137, 139, 149, 151, 157, 163, 167, 173, 179, 181, 191, 193,
    197, 199, 211, 223, 227, 229, 233, 239, 241, 251,
];

const MAX_DISTINCT_FACTORS: usize = 3;
const EMPTY_FACTOR: PrimeFactor = PrimeFactor {
    prime_index: 0,
    exponent: 0,
};

#[derive(Clone, Copy)]
pub(super) struct PrimeFactor {
    pub(super) prime_index: u8,
    pub(super) exponent: u8,
}

pub(super) const ODD_FACTORIZATIONS: [[PrimeFactor; MAX_DISTINCT_FACTORS]; 256] = factorizations();

/// Populate a zeroed odd-factor histogram and return its fractional-bit count.
pub(super) fn reduced_histogram(prefix: &RankedPrefix, odd_counts: &mut [usize; 256]) -> usize {
    debug_assert!(odd_counts.iter().all(|&count| count == 0));
    let mut raw_counts = [0usize; 256];
    for &raw in prefix.as_slice() {
        raw_counts[usize::from(raw)] = raw_counts[usize::from(raw)]
            .checked_add(1)
            .expect("RankedPrefix length overflowed usize");
    }

    let mut fractional_bits = 0usize;
    for (raw, &count) in raw_counts.iter().enumerate() {
        if count == 0 {
            continue;
        }
        let factor = 256u16 - raw as u16;
        let removable = factor.trailing_zeros().min(8) as usize;
        let odd = usize::from(factor >> removable);
        odd_counts[odd] = odd_counts[odd]
            .checked_add(count)
            .expect("RankedPrefix length overflowed usize");
        fractional_bits = fractional_bits
            .checked_add(
                (8 - removable)
                    .checked_mul(count)
                    .expect("RankedPrefix precision overflowed usize"),
            )
            .expect("RankedPrefix precision overflowed usize");
    }
    fractional_bits
}

pub(super) fn pow(mut base: BigUint, mut exponent: usize) -> BigUint {
    let mut result = BigUint::from(1u8);
    while exponent != 0 {
        if exponent & 1 == 1 {
            result *= &base;
        }
        exponent >>= 1;
        if exponent != 0 {
            base = &base * &base;
        }
    }
    result
}

pub(super) fn product_tree(mut terms: Vec<BigUint>) -> BigUint {
    while terms.len() > 1 {
        let mut next = Vec::with_capacity(terms.len().div_ceil(2));
        let mut current = terms.into_iter();
        while let Some(left) = current.next() {
            next.push(match current.next() {
                Some(right) => left * right,
                None => left,
            });
        }
        terms = next;
    }
    terms.pop().unwrap_or_else(|| BigUint::from(1u8))
}

const fn factorizations() -> [[PrimeFactor; MAX_DISTINCT_FACTORS]; 256] {
    let mut table = [[EMPTY_FACTOR; MAX_DISTINCT_FACTORS]; 256];
    let mut odd = 1usize;
    while odd < table.len() {
        let mut remainder = odd as u16;
        let mut prime_index = 0usize;
        let mut factor_index = 0usize;
        while prime_index < ODD_PRIMES.len() && remainder != 1 {
            let prime = ODD_PRIMES[prime_index];
            let mut exponent = 0u8;
            while remainder.is_multiple_of(prime) {
                exponent += 1;
                remainder /= prime;
            }
            if exponent != 0 {
                assert!(factor_index < MAX_DISTINCT_FACTORS);
                table[odd][factor_index] = PrimeFactor {
                    prime_index: prime_index as u8,
                    exponent,
                };
                factor_index += 1;
            }
            prime_index += 1;
        }
        odd += 2;
    }
    table
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn factorization_table_reconstructs_every_odd_factor() {
        for odd in (1u16..=255).step_by(2) {
            let reconstructed = ODD_FACTORIZATIONS[usize::from(odd)]
                .into_iter()
                .take_while(|factor| factor.exponent != 0)
                .fold(1u16, |value, factor| {
                    value
                        * ODD_PRIMES[usize::from(factor.prime_index)]
                            .pow(u32::from(factor.exponent))
                });
            assert_eq!(reconstructed, odd);
        }
    }

    #[test]
    fn product_tree_handles_empty_even_and_odd_term_counts() {
        assert_eq!(product_tree(Vec::new()), BigUint::from(1u8));
        assert_eq!(
            product_tree([3u8, 5, 7, 11].into_iter().map(BigUint::from).collect()),
            BigUint::from(1_155u16)
        );
        assert_eq!(
            product_tree([3u8, 5, 7, 11, 13].into_iter().map(BigUint::from).collect()),
            BigUint::from(15_015u16)
        );
    }
}
