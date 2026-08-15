use num_bigint::BigUint;

use crate::BigDyadic;

/// An ordered prefix of conditional event probabilities.
///
/// Each byte `raw` represents the probability
///
/// ```text
/// p = raw / 256
/// ```
///
/// that the event at that position occurs, conditioned on every previous
/// event in the prefix having been denied.
///
/// A prefix of length `N` therefore describes `N` explicit ranked events plus
/// one implicit tail event:
///
/// ```text
/// event 0     p[0]
/// event 1     (1 - p[0]) p[1]
/// event 2     (1 - p[0]) (1 - p[1]) p[2]
/// ...
/// tail        product_i (1 - p[i])
/// ```
///
/// # Why probabilities are in `[0, 1)`
///
/// `raw = 0` is valid: an explicit event may be impossible.
///
/// Probability one is deliberately *not* representable. If an explicit event
/// had probability one, every event after it would become unreachable. Exact
/// certainty is represented structurally instead: truncate the prefix and use
/// its implicit tail event.
///
/// As a result, the tail probability of every finite prefix is strictly
/// positive by construction.
///
/// # Prefix boundaries
///
/// Let `S(i)` be the tail probability of the first `i` entries. Then
///
/// ```text
/// S(0) = 1
/// S(i) = product_{k < i} (1 - p[k])
/// ```
///
/// Every contiguous ranked interval is determined by two such boundaries:
///
/// ```text
/// P(i <= X < j) = S(i) - S(j)
/// ```
///
/// This is the only normalization arithmetic an arithmetic coder needs.
#[must_use]
#[repr(transparent)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct RankedPrefix<'a> {
    probabilities: &'a [u8],
}

impl<'a> RankedPrefix<'a> {
    /// Construct a ranked prefix from raw `u8 / 256` probabilities.
    #[inline]
    pub const fn new(probabilities: &'a [u8]) -> Self {
        Self { probabilities }
    }

    /// Borrow the raw conditional probabilities.
    #[inline]
    pub const fn as_slice(self) -> &'a [u8] {
        self.probabilities
    }

    /// Number of explicit events in this prefix.
    ///
    /// The prefix also has one implicit tail event at rank `len()`.
    #[inline]
    pub const fn len(self) -> usize {
        self.probabilities.len()
    }

    /// Whether this prefix has no explicit events.
    ///
    /// An empty prefix still has its implicit tail event, with probability one.
    #[inline]
    pub const fn is_empty(self) -> bool {
        self.probabilities.is_empty()
    }

    /// Split the first explicit event from the remaining prefix.
    #[inline]
    pub fn split_first(self) -> Option<(u8, Self)> {
        let (&first, rest) = self.probabilities.split_first()?;
        Some((first, Self::new(rest)))
    }

    /// Split this prefix at an anchored event boundary.
    #[inline]
    pub fn split_at(self, mid: usize) -> (Self, Self) {
        let (left, right) = self.probabilities.split_at(mid);
        (Self::new(left), Self::new(right))
    }

    /// Return the exact probability of the implicit tail event.
    ///
    /// For raw bytes `x_i`, this is
    ///
    /// ```text
    /// product_i ((256 - x_i) / 256).
    /// ```
    ///
    /// The implementation is exact. It first removes powers of two from each
    /// small factor, so zero-probability events (`x_i = 0`) disappear entirely
    /// as multiplicative identities. Up to eight remaining odd factors are
    /// accumulated in a `u64`; `255^8 < 2^64`, so a chunk cannot overflow.
    /// Chunk products are then combined through an online balanced tree of
    /// [`BigUint`] multiplications.
    ///
    /// This shape is deliberate: the small-factor reduction is friendly to
    /// SIMD/vectorization, while balanced large multiplications avoid feeding
    /// a huge bigint one tiny operand at a time.
    pub fn tail_probability(self) -> BigDyadic {
        if self.probabilities.is_empty() {
            return BigDyadic::one();
        }

        let mut fractional_bits = 0usize;
        let mut levels: Vec<Option<BigUint>> = Vec::new();

        for chunk in self.probabilities.chunks(8) {
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

    /// Exact scalar reference implementation of [`Self::tail_probability`].
    ///
    /// Kept crate-visible for tests. Production code should use the balanced
    /// implementation above.
    #[cfg(test)]
    pub(crate) fn tail_probability_scalar(self) -> BigDyadic {
        let mut numerator = BigUint::from(1u8);
        let mut fractional_bits = 0usize;

        for &raw in self.probabilities {
            let factor = 256u16 - raw as u16;
            let removable = factor.trailing_zeros().min(8) as usize;
            let odd = factor >> removable;
            numerator *= BigUint::from(odd);
            fractional_bits += 8 - removable;
        }

        BigDyadic::from_scaled(numerator, fractional_bits)
    }
}

/// Insert an equally-sized product into an online balanced multiplication tree.
///
/// `levels[n]` contains a product of `2^n` original chunks. Collisions multiply
/// equal-sized operands and carry upward, exactly like incrementing a binary
/// counter.
fn push_balanced(levels: &mut Vec<Option<BigUint>>, mut value: BigUint) {
    let mut level = 0usize;

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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_prefix_has_certain_tail() {
        assert_eq!(RankedPrefix::new(&[]).tail_probability(), BigDyadic::one());
    }

    #[test]
    fn zero_probability_is_a_multiplicative_identity() {
        let a = RankedPrefix::new(&[0]).tail_probability();
        let b = RankedPrefix::new(&[0, 0, 0, 0]).tail_probability();
        assert_eq!(a, BigDyadic::one());
        assert_eq!(b, BigDyadic::one());
    }

    #[test]
    fn max_probability_leaves_one_over_256() {
        let tail = RankedPrefix::new(&[255]).tail_probability();
        assert_eq!(tail.numerator(), BigUint::from(1u8));
        assert_eq!(tail.fractional_bits(), 8);
    }

    #[test]
    fn balanced_matches_scalar() {
        let cases: &[&[u8]] = &[
            &[],
            &[0],
            &[255],
            &[128, 128],
            &[1, 2, 3, 4, 5, 6, 7, 8],
            &[255; 32],
            &[0, 255, 64, 128, 192, 1, 254, 17, 99, 201, 33],
        ];

        for &case in cases {
            let prefix = RankedPrefix::new(case);
            assert_eq!(prefix.tail_probability(), prefix.tail_probability_scalar());
        }
    }
}
