use core::mem::{align_of, size_of};

include!(concat!(env!("OUT_DIR"), "/jeffreys_tables.rs"));

/// A one-byte quantization of a Bernoulli distribution, uniform in the
/// Fisher-Rao / Jeffreys coordinate.
///
/// For Bernoulli probability `p`, define
///
/// ```text
/// u(p) = (2 / pi) * asin(sqrt(p))
/// ```
///
/// Byte `k` denotes the bucket
///
/// ```text
/// k / 256 <= u(p) < (k + 1) / 256
/// ```
///
/// with the final bucket also containing `p = 1`.
///
/// The byte is a bucket index, not a fixed-point probability. All 256 byte
/// values are valid. Buckets `0` and `255` touch the exact endpoints, but their
/// scalar representatives remain strictly inside `(0, 1)`.
#[must_use]
#[repr(transparent)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct JeffreysU8(u8);

const _: [(); 1] = [(); size_of::<JeffreysU8>()];
const _: [(); 1] = [(); align_of::<JeffreysU8>()];

impl JeffreysU8 {
    /// Construct directly from a protocol byte.
    #[inline]
    pub const fn from_raw(raw: u8) -> Self {
        Self(raw)
    }

    /// Return the protocol byte.
    #[inline]
    pub const fn into_raw(self) -> u8 {
        self.0
    }

    /// Quantize a probability by Jeffreys bucket.
    ///
    /// Returns `None` for NaN or values outside `[0, 1]`.
    #[inline]
    pub fn quantize_probability(probability: f32) -> Option<Self> {
        if !(0.0..=1.0).contains(&probability) {
            return None;
        }

        let index = if probability < 0.5 {
            PROBABILITY_BOUNDARIES_LOWER.partition_point(|&boundary| probability >= boundary)
        } else if probability == 0.5 {
            128
        } else {
            // Complement symmetry reverses interval orientation, hence `<`
            // rather than `<=` for the reflected boundary search.
            let mirrored = 1.0 - probability;
            255 - PROBABILITY_BOUNDARIES_LOWER.partition_point(|&boundary| boundary < mirrored)
        };

        Some(Self(index as u8))
    }

    /// Quantize a Bernoulli logit by Jeffreys bucket.
    ///
    /// Infinite logits are valid and map to the first/last bucket. NaN is
    /// rejected.
    #[inline]
    pub fn quantize_logit(logit: f32) -> Option<Self> {
        if logit.is_nan() {
            return None;
        }

        let index = if logit < 0.0 {
            LOGIT_BOUNDARIES_LOWER.partition_point(|&boundary| logit >= boundary)
        } else if logit == 0.0 {
            128
        } else {
            let mirrored = -logit;
            255 - LOGIT_BOUNDARIES_LOWER.partition_point(|&boundary| boundary < mirrored)
        };

        Some(Self(index as u8))
    }

    /// Probability interval represented by this bucket.
    ///
    /// The interval is `[lower, upper)`, except that the final bucket includes
    /// `p = 1`.
    #[inline]
    pub fn probability_bounds(self) -> (f32, f32) {
        let index = self.0 as usize;
        (probability_boundary(index), probability_boundary(index + 1))
    }

    /// Minimax scalar probability for this bucket.
    ///
    /// This minimizes the maximum `D_KL(Ber(p) || Ber(q))` over the bucket.
    #[inline]
    pub fn representative_probability(self) -> f32 {
        let index = self.0 as usize;
        if index < 128 {
            REPRESENTATIVE_PROBABILITIES_LOWER[index]
        } else {
            1.0 - REPRESENTATIVE_PROBABILITIES_LOWER[255 - index]
        }
    }

    /// View protocol bytes as Jeffreys buckets without copying.
    ///
    /// This is sound because the type is transparent over `u8` and every byte
    /// is a valid value.
    #[inline]
    pub fn slice_from_bytes(bytes: &[u8]) -> &[Self] {
        // SAFETY: same size/alignment as `u8`; no invalid bit patterns.
        unsafe { core::slice::from_raw_parts(bytes.as_ptr().cast(), bytes.len()) }
    }

    /// View Jeffreys buckets as protocol bytes without copying.
    #[inline]
    pub fn slice_as_bytes(values: &[Self]) -> &[u8] {
        // SAFETY: `#[repr(transparent)]` guarantees the `u8` layout.
        unsafe { core::slice::from_raw_parts(values.as_ptr().cast(), values.len()) }
    }
}

impl From<u8> for JeffreysU8 {
    #[inline]
    fn from(raw: u8) -> Self {
        Self::from_raw(raw)
    }
}

impl From<JeffreysU8> for u8 {
    #[inline]
    fn from(value: JeffreysU8) -> Self {
        value.into_raw()
    }
}

#[inline]
fn probability_boundary(index: usize) -> f32 {
    match index {
        0 => 0.0,
        1..=127 => PROBABILITY_BOUNDARIES_LOWER[index - 1],
        128 => 0.5,
        129..=255 => 1.0 - PROBABILITY_BOUNDARIES_LOWER[255 - index],
        256 => 1.0,
        _ => unreachable!("JeffreysU8 boundary index is in 0..=256"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn kl_bits(p: f64, q: f64) -> f64 {
        let nats = if p == 0.0 {
            -(1.0 - q).ln()
        } else if p == 1.0 {
            -q.ln()
        } else {
            p * (p / q).ln() + (1.0 - p) * ((1.0 - p) / (1.0 - q)).ln()
        };
        nats / core::f64::consts::LN_2
    }

    #[test]
    fn layout_is_exactly_one_byte() {
        assert_eq!(size_of::<JeffreysU8>(), 1);
        assert_eq!(align_of::<JeffreysU8>(), 1);
    }

    #[test]
    fn every_byte_round_trips() {
        for raw in 0u8..=u8::MAX {
            assert_eq!(JeffreysU8::from_raw(raw).into_raw(), raw);
        }
    }

    #[test]
    fn endpoint_probabilities_are_bucketed_not_represented() {
        assert_eq!(JeffreysU8::quantize_probability(0.0), Some(JeffreysU8(0)));
        assert_eq!(JeffreysU8::quantize_probability(1.0), Some(JeffreysU8(255)));
        assert!(JeffreysU8(0).representative_probability() > 0.0);
        assert!(JeffreysU8(255).representative_probability() < 1.0);
    }

    #[test]
    fn invalid_probabilities_are_rejected() {
        assert_eq!(JeffreysU8::quantize_probability(f32::NAN), None);
        assert_eq!(JeffreysU8::quantize_probability(-f32::EPSILON), None);
        assert_eq!(JeffreysU8::quantize_probability(1.0 + f32::EPSILON), None);
    }

    #[test]
    fn logit_endpoints_work() {
        assert_eq!(
            JeffreysU8::quantize_logit(f32::NEG_INFINITY),
            Some(JeffreysU8(0))
        );
        assert_eq!(
            JeffreysU8::quantize_logit(f32::INFINITY),
            Some(JeffreysU8(255))
        );
        assert_eq!(JeffreysU8::quantize_logit(f32::NAN), None);
    }

    #[test]
    fn representatives_requantize_to_their_bucket() {
        for raw in 0u8..=u8::MAX {
            let value = JeffreysU8(raw);
            assert_eq!(
                JeffreysU8::quantize_probability(value.representative_probability()),
                Some(value)
            );
        }
    }

    #[test]
    fn symmetry_is_exact() {
        for raw in 0u8..=u8::MAX {
            let q = JeffreysU8(raw).representative_probability();
            let mirror = JeffreysU8(255 - raw).representative_probability();
            assert_eq!(q + mirror, 1.0);
        }
    }

    #[test]
    fn worst_case_kl_is_below_three_e_minus_five_bits() {
        let mut worst = 0.0f64;
        for raw in 0u8..=u8::MAX {
            let value = JeffreysU8(raw);
            let (lower, upper) = value.probability_bounds();
            let q = value.representative_probability() as f64;
            worst = worst
                .max(kl_bits(lower as f64, q))
                .max(kl_bits(upper as f64, q));
        }
        assert!(worst < 3e-5, "worst-case KL penalty was {worst:e} bits");
    }

    #[test]
    fn byte_slice_views_are_zero_copy() {
        let bytes = [0u8, 1, 127, 128, 254, 255];
        let values = JeffreysU8::slice_from_bytes(&bytes);
        assert_eq!(values.as_ptr().cast::<u8>(), bytes.as_ptr());
        assert_eq!(JeffreysU8::slice_as_bytes(values), bytes);
    }
}
