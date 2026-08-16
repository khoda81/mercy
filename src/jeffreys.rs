use core::mem::{align_of, size_of};

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
            PROBABILITY_BOUNDARIES_LOWER
                .partition_point(|&boundary| probability >= boundary)
        } else if probability == 0.5 {
            128
        } else {
            // Complement symmetry reverses interval orientation, hence `<`
            // rather than `<=` for the reflected boundary search.
            let mirrored = 1.0 - probability;
            255 - PROBABILITY_BOUNDARIES_LOWER
                .partition_point(|&boundary| boundary < mirrored)
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

/// `sin^2(pi * k / 512)` for `k = 1..=127`.
static PROBABILITY_BOUNDARIES_LOWER: [f32; 127] = [
    3.76490789e-05, 0.000150590655, 0.000338807702, 0.000602271874, 0.000940943544, 0.00135477167,
    0.0018436939, 0.00240763673, 0.00304651493, 0.00376023259, 0.00454868237, 0.00541174505,
    0.00634929072, 0.00736117875, 0.00844725594, 0.00960735977, 0.0108413147, 0.0121489353,
    0.0135300243, 0.014984373, 0.0165117644, 0.0181119666, 0.0197847411, 0.0215298329,
    0.0233469792, 0.02523591, 0.0271963365, 0.0292279683, 0.0313304923, 0.0335035995,
    0.0357469581, 0.038060233, 0.040443074, 0.0428951234, 0.0454160087, 0.0480053537,
    0.0506627671, 0.0533878505, 0.0561801903, 0.0590393692, 0.0619649515, 0.0649565011,
    0.0680135712, 0.0711356923, 0.0743224025, 0.0775732175, 0.0808876455, 0.0842651948,
    0.0877053514, 0.0912075937, 0.0947714001, 0.0983962342, 0.102081545, 0.105826788,
    0.109631389, 0.113494776, 0.117416367, 0.12139558, 0.125431806, 0.12952444,
    0.133672863, 0.137876466, 0.142134592, 0.146446615, 0.150811881, 0.155229732,
    0.1596995, 0.164220527, 0.168792114, 0.173413575, 0.178084224, 0.182803363,
    0.187570259, 0.192384198, 0.19724448, 0.202150345, 0.207101077, 0.212095901,
    0.217134088, 0.222214878, 0.227337509, 0.232501194, 0.237705156, 0.242948622,
    0.248230815, 0.253550917, 0.258908123, 0.264301628, 0.269730657, 0.275194347,
    0.280691892, 0.286222458, 0.29178521, 0.297379345, 0.303003967, 0.308658272,
    0.314341396, 0.320052475, 0.325790673, 0.331555068, 0.337344855, 0.343159139,
    0.348997027, 0.354857653, 0.360740155, 0.366643608, 0.372567177, 0.378509909,
    0.38447094, 0.390449375, 0.396444321, 0.402454853, 0.408480048, 0.414519042,
    0.42057094, 0.426634759, 0.432709634, 0.438794672, 0.44488889, 0.450991422,
    0.457101345, 0.463217705, 0.469339639, 0.475466162, 0.48159638, 0.4877294,
    0.493864238,
];

/// `2 * ln(tan(pi * k / 512))` for `k = 1..=127`.
static LOGIT_BOUNDARIES_LOWER: [f32; 127] = [
    -10.1871643, -8.8007946, -7.98973894, -7.41419888, -6.96768618, -6.60276651,
    -6.29413891, -6.02669907, -5.79070568, -5.57950735, -5.38835859, -5.21375704,
    -5.05304241, -4.90414667, -4.76543045, -4.63557196, -4.51349068, -4.39829063,
    -4.28922176, -4.18564939, -4.08703232, -3.99290442, -3.90286136, -3.81655073,
    -3.7336638, -3.65392756, -3.57710004, -3.50296569, -3.43133163, -3.36202455,
    -3.2948885, -3.22978187, -3.16657615, -3.10515475, -3.04541087, -2.98724699,
    -2.93057275, -2.87530637, -2.82137108, -2.76869678, -2.7172184, -2.66687512,
    -2.61761093, -2.56937337, -2.52211356, -2.47578573, -2.4303472, -2.38575792,
    -2.34198022, -2.29897857, -2.25671983, -2.21517253, -2.17430735, -2.13409591,
    -2.09451175, -2.05553031, -2.01712728, -1.97928035, -1.94196808, -1.90517008,
    -1.86886704, -1.83304048, -1.79767287, -1.76274717, -1.72824752, -1.69415855,
    -1.6604656, -1.62715459, -1.59421206, -1.56162512, -1.52938128, -1.49746871,
    -1.46587598, -1.43459201, -1.4036063, -1.37290847, -1.34248877, -1.31233788,
    -1.28244627, -1.25280547, -1.22340655, -1.19424164, -1.16530228, -1.13658106,
    -1.10807037, -1.07976282, -1.05165136, -1.02372921, -0.995989621, -0.968426049,
    -0.941032231, -0.913802028, -0.886729419, -0.859808564, -0.833033741, -0.806399465,
    -0.779900193, -0.753530622, -0.727285624, -0.701160073, -0.675149024, -0.649247527,
    -0.623450816, -0.59775424, -0.572153091, -0.546642959, -0.521219254, -0.495877713,
    -0.470613927, -0.445423663, -0.420302719, -0.395246983, -0.370252311, -0.345314682,
    -0.32043013, -0.295594633, -0.270804316, -0.24605529, -0.221343696, -0.196665719,
    -0.172017545, -0.147395402, -0.122795537, -0.098214224, -0.0736477152, -0.0490923151,
    -0.024544308,
];

/// Minimax-KL representatives for buckets `0..128`.
static REPRESENTATIVE_PROBABILITIES_LOWER: [f32; 128] = [
    1.3850392e-05, 8.79422369e-05, 0.0002384611, 0.000464287848, 0.000765352743, 0.00114160392,
    0.00159298279, 0.00211942056, 0.00272083795, 0.00339714391, 0.00414823648, 0.00497400248,
    0.00587431807, 0.0068490468, 0.00789804291, 0.00902114715, 0.010218191, 0.0114889946,
    0.0128333662, 0.0142511027, 0.0157419927, 0.0173058081, 0.018942317, 0.0206512716,
    0.022432413, 0.0242854748, 0.0262101777, 0.0282062311, 0.0302733351, 0.0324111804,
    0.0346194394, 0.036897786, 0.03924587, 0.0416633449, 0.0441498458, 0.0467049927,
    0.0493284054, 0.0520196855, 0.0547784306, 0.0576042272, 0.0604966432, 0.0634552538,
    0.066479601, 0.0695692301, 0.0727236867, 0.0759424865, 0.0792251527, 0.0825711787,
    0.0859800726, 0.0894513205, 0.0929843858, 0.0965787545, 0.100233875, 0.103949197,
    0.10772416, 0.111558206, 0.115450747, 0.119401194, 0.123408966, 0.127473444,
    0.131594032, 0.135770097, 0.140000999, 0.144286141, 0.148624837, 0.153016448,
    0.157460317, 0.161955774, 0.166502133, 0.171098724, 0.175744832, 0.180439785,
    0.185182855, 0.189973339, 0.19481051, 0.19969365, 0.204622, 0.209594846,
    0.214611411, 0.219670966, 0.224772736, 0.229915962, 0.235099852, 0.240323633,
    0.245586529, 0.250887722, 0.25622645, 0.261601895, 0.267013222, 0.272459626,
    0.277940333, 0.283454448, 0.289001197, 0.294579715, 0.300189167, 0.305828691,
    0.31149748, 0.317194641, 0.322919339, 0.32867071, 0.334447891, 0.340249985,
    0.346076161, 0.351925492, 0.357797116, 0.363690168, 0.369603753, 0.375536978,
    0.381488949, 0.387458742, 0.393445522, 0.399448305, 0.405466259, 0.411498457,
    0.417543948, 0.423601896, 0.429671317, 0.435751349, 0.441841036, 0.447939515,
    0.454045802, 0.460159034, 0.466278255, 0.472402543, 0.478531003, 0.484662682,
    0.490796685, 0.496932089,
];

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
        assert_eq!(JeffreysU8::quantize_logit(f32::NEG_INFINITY), Some(JeffreysU8(0)));
        assert_eq!(JeffreysU8::quantize_logit(f32::INFINITY), Some(JeffreysU8(255)));
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
