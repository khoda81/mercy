use std::{
    cmp::Ordering,
    fmt,
    mem::size_of,
    ops::Mul,
};

use bitvec::prelude::{BitBox, BitSlice, BitVec, Msb0};
use num_bigint::BigUint;

/// An exact arbitrary-precision dyadic fraction in `[0, 1]`.
///
/// A value is represented as a fixed-width bit string. The first bit is the
/// integer bit and every remaining bit is fractional:
///
/// ```text
/// bits = b0 b1 ... bk
/// value = integer(bits) / 2^k
/// ```
///
/// The bit-string length therefore *is* the denominator metadata. No separate
/// exponent is stored. For example:
///
/// ```text
/// 1       -> [1]
/// 1/2     -> [0, 1]
/// 3/4     -> [0, 1, 1]
/// 1/256   -> [0, 0, 0, 0, 0, 0, 0, 0, 1]
/// ```
///
/// Values are canonicalized by removing powers of two shared by the numerator
/// and denominator. Zero and one therefore have one-bit representations.
///
/// # Representation size
///
/// The only field is a [`BitBox`], an owning bit-slice pointer. The compile-time
/// assertion below deliberately locks the handle size to two machine words on
/// supported targets; this is part of the experiment because these values are
/// expected to be passed around frequently while the actual bits live on the
/// heap.
#[must_use]
#[derive(Clone, PartialEq, Eq, Hash)]
pub struct BigDyadic {
    bits: BitBox<u8, Msb0>,
}

const _: [(); 2 * size_of::<usize>()] = [(); size_of::<BigDyadic>()];

impl BigDyadic {
    /// Exact zero.
    #[inline]
    pub fn zero() -> Self {
        Self::from_scaled(BigUint::from(0u8), 0)
    }

    /// Exact one.
    #[inline]
    pub fn one() -> Self {
        Self::from_scaled(BigUint::from(1u8), 0)
    }

    /// Construct `numerator / 2^fractional_bits` exactly.
    ///
    /// The value must be in `[0, 1]`. Any common power of two is cancelled so
    /// numerically equal values have the same bit representation.
    pub(crate) fn from_scaled(mut numerator: BigUint, mut fractional_bits: usize) -> Self {
        let denominator = BigUint::from(1u8) << fractional_bits;
        assert!(numerator <= denominator, "BigDyadic must lie in [0, 1]");

        if numerator.bits() == 0 {
            fractional_bits = 0;
        } else if fractional_bits != 0 {
            let removable = numerator
                .trailing_zeros()
                .expect("nonzero BigUint has trailing-zero count")
                .min(fractional_bits as u64) as usize;
            if removable != 0 {
                numerator >>= removable;
                fractional_bits -= removable;
            }
        }

        let width = fractional_bits
            .checked_add(1)
            .expect("BigDyadic bit width overflowed usize");
        let numerator_bits = numerator.bits() as usize;
        debug_assert!(numerator_bits <= width);

        let mut bits = BitVec::<u8, Msb0>::repeat(false, width);
        for bit in 0..numerator_bits {
            if numerator.bit(bit as u64) {
                bits.set(width - 1 - bit, true);
            }
        }

        Self {
            bits: bits.into_boxed_bitslice(),
        }
    }

    /// Number of fractional bits in the exact representation.
    #[inline]
    pub fn fractional_bits(&self) -> usize {
        self.bits.len() - 1
    }

    /// The canonical fixed-width bit representation.
    #[inline]
    pub fn as_bits(&self) -> &BitSlice<u8, Msb0> {
        &self.bits
    }

    /// Return the integer numerator of `self / 2^fractional_bits()`.
    pub fn numerator(&self) -> BigUint {
        let mut numerator = BigUint::from(0u8);
        for (bit, set) in self.bits.iter().by_vals().rev().enumerate() {
            if set {
                numerator.set_bit(bit as u64, true);
            }
        }
        numerator
    }

    /// Exact multiplication.
    ///
    /// Dyadic multiplication is just integer multiplication plus addition of
    /// denominator exponents; [`Self::from_scaled`] then removes any common
    /// powers of two.
    pub fn multiplied(&self, rhs: &Self) -> Self {
        let numerator = self.numerator() * rhs.numerator();
        let fractional_bits = self
            .fractional_bits()
            .checked_add(rhs.fractional_bits())
            .expect("BigDyadic precision overflowed usize");
        Self::from_scaled(numerator, fractional_bits)
    }

    /// Exact complement `1 - self`.
    pub fn complement(&self) -> Self {
        let fractional_bits = self.fractional_bits();
        let one = BigUint::from(1u8) << fractional_bits;
        Self::from_scaled(one - self.numerator(), fractional_bits)
    }

    /// Return `floor(value * self)` exactly.
    ///
    /// This is a useful bridge from arbitrary-precision model boundaries to a
    /// fixed-width arithmetic-coder range.
    pub fn scale_floor_u64(&self, value: u64) -> u64 {
        let product = self.numerator() * BigUint::from(value);
        let scaled = product >> self.fractional_bits();
        let digits = scaled.to_u64_digits();
        match digits.as_slice() {
            [] => 0,
            [scaled] => *scaled,
            _ => unreachable!("floor(value * probability) cannot exceed value"),
        }
    }
}

impl Default for BigDyadic {
    fn default() -> Self {
        Self::zero()
    }
}

impl fmt::Debug for BigDyadic {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("BigDyadic")
            .field("numerator", &self.numerator())
            .field("fractional_bits", &self.fractional_bits())
            .finish()
    }
}

impl Ord for BigDyadic {
    fn cmp(&self, other: &Self) -> Ordering {
        let bits = self.fractional_bits().max(other.fractional_bits());
        let lhs = self.numerator() << (bits - self.fractional_bits());
        let rhs = other.numerator() << (bits - other.fractional_bits());
        lhs.cmp(&rhs)
    }
}

impl PartialOrd for BigDyadic {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Mul<&BigDyadic> for &BigDyadic {
    type Output = BigDyadic;

    fn mul(self, rhs: &'b BigDyadic) -> Self::Output {
        self.multiplied(rhs)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn representation_is_two_words() {
        assert_eq!(size_of::<BigDyadic>(), 2 * size_of::<usize>());
    }

    #[test]
    fn canonicalizes_powers_of_two() {
        let half = BigDyadic::from_scaled(BigUint::from(128u16), 8);
        assert_eq!(half.fractional_bits(), 1);
        assert_eq!(half.numerator(), BigUint::from(1u8));
    }

    #[test]
    fn multiplication_is_exact() {
        let half = BigDyadic::from_scaled(BigUint::from(1u8), 1);
        let quarter = &half * &half;
        assert_eq!(quarter.numerator(), BigUint::from(1u8));
        assert_eq!(quarter.fractional_bits(), 2);
    }

    #[test]
    fn complement_is_exact() {
        let quarter = BigDyadic::from_scaled(BigUint::from(1u8), 2);
        let three_quarters = quarter.complement();
        assert_eq!(three_quarters.numerator(), BigUint::from(3u8));
        assert_eq!(three_quarters.fractional_bits(), 2);
    }
}
