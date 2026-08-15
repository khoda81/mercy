use std::ops::{
    Deref, DerefMut, Index, IndexMut, Range, RangeFrom, RangeFull, RangeInclusive, RangeTo,
    RangeToInclusive,
};

use crate::BigDyadic;

pub mod fixtures;
pub mod implementations;

/// An ordered prefix of conditional event probabilities.
///
/// `RankedPrefix` is an unsized, slice-like view over probability bytes. The
/// wrapper is deliberately a dynamically sized type (DST): the sequence length
/// belongs in the metadata of `&RankedPrefix`, `&mut RankedPrefix`, or
/// `Box<RankedPrefix>`, rather than in a second slice pointer stored inside a
/// small, sized view object. Build and resize models as [`Vec<u8>`], then borrow
/// them with [`Self::from_slice`] or freeze them with
/// [`Self::from_boxed_slice`].
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
///
/// # Representation and conversion safety
///
/// The type is `#[repr(transparent)]` over `[u8]`. It adds no storage,
/// validity requirement, or invariant to the wrapped bytes. Consequently a
/// slice pointer and its length metadata can be reinterpreted as a
/// `RankedPrefix` pointer without moving or reallocating the bytes. The small
/// amount of unsafe pointer conversion required for this is encapsulated by the
/// safe slice and box conversion methods.
#[must_use]
#[repr(transparent)]
#[derive(Debug, PartialEq, Eq, Hash)]
pub struct RankedPrefix([u8]);

impl RankedPrefix {
    /// Borrow raw `u8 / 256` probabilities as a ranked prefix without copying.
    #[inline]
    pub fn from_slice(slice: &[u8]) -> &Self {
        // SAFETY: `RankedPrefix` is transparent over `[u8]`, adds no invariant,
        // and therefore has identical data-pointer and length metadata.
        unsafe { &*(slice as *const [u8] as *const Self) }
    }

    /// Mutably borrow raw `u8 / 256` probabilities without copying.
    #[inline]
    pub fn from_slice_mut(slice: &mut [u8]) -> &mut Self {
        // SAFETY: As above, and every possible `u8` remains valid after
        // mutation because the wrapper imposes no byte-level invariant.
        unsafe { &mut *(slice as *mut [u8] as *mut Self) }
    }

    /// Convert owned probability bytes into an owned ranked prefix, zero-copy.
    ///
    /// `RankedPrefix` is intentionally not resizable. Construct or resize a
    /// [`Vec<u8>`], convert it to `Box<[u8]>`, then freeze it with this method.
    #[inline]
    pub fn from_boxed_slice(slice: Box<[u8]>) -> Box<Self> {
        let raw = Box::into_raw(slice);
        // SAFETY: The transparent wrapper preserves both the allocation layout
        // and the slice pointer's length metadata, so the same allocation can
        // be owned and eventually dropped as `Box<RankedPrefix>`.
        unsafe { Box::from_raw(raw as *mut Self) }
    }

    /// Recover the underlying boxed bytes without copying or reallocating.
    #[inline]
    pub fn into_boxed_slice(this: Box<Self>) -> Box<[u8]> {
        let raw = Box::into_raw(this);
        // SAFETY: This exactly reverses `from_boxed_slice`; the transparent
        // representation has the same allocation layout and length metadata.
        unsafe { Box::from_raw(raw as *mut [u8]) }
    }

    /// Borrow the raw conditional probabilities.
    #[inline]
    pub const fn as_slice(&self) -> &[u8] {
        &self.0
    }

    /// Mutably borrow the raw conditional probabilities.
    #[inline]
    pub fn as_slice_mut(&mut self) -> &mut [u8] {
        &mut self.0
    }

    /// Number of explicit events in this prefix.
    ///
    /// The prefix also has one implicit tail event at rank `len()`.
    #[inline]
    pub const fn len(&self) -> usize {
        self.0.len()
    }

    /// Whether this prefix has no explicit events.
    ///
    /// An empty prefix still has its implicit tail event, with probability one.
    #[inline]
    pub const fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    /// Split the first explicit event from the remaining prefix.
    #[inline]
    pub fn split_first(&self) -> Option<(u8, &Self)> {
        let (&first, rest) = self.0.split_first()?;
        Some((first, Self::from_slice(rest)))
    }

    /// Split this prefix at an anchored event boundary.
    #[inline]
    pub fn split_at(&self, mid: usize) -> (&Self, &Self) {
        let (left, right) = self.0.split_at(mid);
        (Self::from_slice(left), Self::from_slice(right))
    }

    /// Return the exact probability of the implicit tail event.
    ///
    /// For raw bytes `x_i`, this is
    ///
    /// ```text
    /// product_i ((256 - x_i) / 256).
    /// ```
    ///
    /// The public operation delegates to [`implementations::compute`], the
    /// fastest exact candidate selected by the durable benchmark suite. The
    /// other crate-owned implementations remain available through
    /// [`implementations`] for direct, apples-to-apples benchmarking; benchmark
    /// files contain no arithmetic implementation.
    pub fn tail_probability(&self) -> BigDyadic {
        implementations::compute(self)
    }

    /// Consume owned probability storage while computing the exact tail.
    ///
    /// The selected implementation may reuse the input allocation as scratch;
    /// callers that only have a borrow should use [`Self::tail_probability`].
    pub fn into_tail_probability(this: Box<Self>) -> BigDyadic {
        implementations::owned::compute(this)
    }
}

impl AsRef<[u8]> for RankedPrefix {
    #[inline]
    fn as_ref(&self) -> &[u8] {
        self.as_slice()
    }
}

impl AsMut<[u8]> for RankedPrefix {
    #[inline]
    fn as_mut(&mut self) -> &mut [u8] {
        self.as_slice_mut()
    }
}

impl Deref for RankedPrefix {
    type Target = [u8];

    #[inline]
    fn deref(&self) -> &Self::Target {
        self.as_slice()
    }
}

impl DerefMut for RankedPrefix {
    #[inline]
    fn deref_mut(&mut self) -> &mut Self::Target {
        self.as_slice_mut()
    }
}

impl Index<usize> for RankedPrefix {
    type Output = u8;

    #[inline]
    fn index(&self, index: usize) -> &Self::Output {
        &self.0[index]
    }
}

impl IndexMut<usize> for RankedPrefix {
    #[inline]
    fn index_mut(&mut self, index: usize) -> &mut Self::Output {
        &mut self.0[index]
    }
}

macro_rules! impl_range_index {
    ($($range:ty),+ $(,)?) => {
        $(
            impl Index<$range> for RankedPrefix {
                type Output = RankedPrefix;

                #[inline]
                fn index(&self, index: $range) -> &Self::Output {
                    Self::from_slice(&self.0[index])
                }
            }

            impl IndexMut<$range> for RankedPrefix {
                #[inline]
                fn index_mut(&mut self, index: $range) -> &mut Self::Output {
                    Self::from_slice_mut(&mut self.0[index])
                }
            }
        )+
    };
}

impl_range_index!(
    Range<usize>,
    RangeFrom<usize>,
    RangeTo<usize>,
    RangeToInclusive<usize>,
    RangeInclusive<usize>,
    RangeFull,
);

#[cfg(test)]
mod tests {
    use num_bigint::BigUint;

    use super::*;

    #[test]
    fn slice_conversions_are_zero_copy() {
        let probabilities = [1, 2, 3, 4];
        let prefix = RankedPrefix::from_slice(&probabilities);
        assert_eq!(prefix.as_slice().as_ptr(), probabilities.as_ptr());
        assert_eq!(prefix.as_slice(), probabilities);
    }

    #[test]
    fn mutable_slice_conversion_and_indexing_preserve_the_wrapper() {
        let mut probabilities = [1, 2, 3, 4];
        let prefix = RankedPrefix::from_slice_mut(&mut probabilities);
        prefix[1] = 9;
        prefix[2..][0] = 8;
        let middle: &RankedPrefix = &prefix[1..=2];
        assert_eq!(middle.as_slice(), &[9, 8]);
        assert_eq!(probabilities, [1, 9, 8, 4]);
    }

    #[test]
    fn boxed_conversion_round_trips_without_reallocation() {
        let bytes = vec![7, 11, 13].into_boxed_slice();
        let allocation = bytes.as_ptr();
        let prefix = RankedPrefix::from_boxed_slice(bytes);
        assert_eq!(prefix.as_slice().as_ptr(), allocation);
        let bytes = RankedPrefix::into_boxed_slice(prefix);
        assert_eq!(bytes.as_ptr(), allocation);
        assert_eq!(&*bytes, &[7, 11, 13]);
    }

    #[test]
    fn empty_prefix_has_certain_tail() {
        assert_eq!(
            RankedPrefix::from_slice(&[]).tail_probability(),
            BigDyadic::one()
        );
    }

    #[test]
    fn zero_probability_is_a_multiplicative_identity() {
        let a = RankedPrefix::from_slice(&[0]).tail_probability();
        let b = RankedPrefix::from_slice(&[0, 0, 0, 0]).tail_probability();
        assert_eq!(a, BigDyadic::one());
        assert_eq!(b, BigDyadic::one());
    }

    #[test]
    fn max_probability_leaves_one_over_256() {
        let tail = RankedPrefix::from_slice(&[255]).tail_probability();
        assert_eq!(tail.numerator(), BigUint::from(1u8));
        assert_eq!(tail.fractional_bits(), 8);
    }

    #[test]
    fn all_candidates_are_exactly_equivalent() {
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
            let prefix = RankedPrefix::from_slice(case);
            let expected = implementations::scalar::compute(prefix);
            for candidate in implementations::ALL_IMPLEMENTATIONS {
                assert_eq!(candidate.compute(prefix), expected, "{}", candidate.name);
            }
            assert_eq!(prefix.tail_probability(), expected);
        }
    }

    #[test]
    fn candidates_match_on_large_repeated_and_model_shaped_inputs() {
        let cases = [
            fixtures::ONES.generate(4_096),
            fixtures::PATTERNED.generate(4_096),
            fixtures::MODEL_FLAT.generate(4_096),
            fixtures::MODEL_PEAKED.generate(4_096),
            fixtures::MODEL_LONG_TAIL.generate(4_096),
        ];

        for case in cases {
            let prefix = RankedPrefix::from_slice(&case);
            let expected = implementations::online_balanced::compute(prefix);
            for candidate in implementations::PERFORMANCE_CANDIDATES {
                assert_eq!(candidate.compute(prefix), expected, "{}", candidate.name);
            }
        }
    }
}
