use std::fmt;

const PROBABILITY_BITS: u32 = 16;
const RANGE_TOP: u32 = 1 << 24;

/// A probability on the fixed grid `raw / 2^16`.
///
/// Zero is exact. One is intentionally not representable: a deterministic
/// final branch does not need to be entropy coded.
#[repr(transparent)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct FractionalU16(u16);

impl FractionalU16 {
    pub const ZERO: Self = Self(0);
    pub const HALF: Self = Self(1 << 15);

    #[inline]
    pub const fn from_raw(raw: u16) -> Self {
        Self(raw)
    }

    #[inline]
    pub const fn raw(self) -> u16 {
        self.0
    }

    #[inline]
    pub fn as_f64(self) -> f64 {
        self.0 as f64 / (1_u64 << PROBABILITY_BITS) as f64
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CoderError {
    UnexpectedEof,
    ImpossibleOutcome { probability: FractionalU16 },
}

impl fmt::Display for CoderError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnexpectedEof => f.write_str("unexpected end of arithmetic-coded input"),
            Self::ImpossibleOutcome { probability } => write!(
                f,
                "cannot encode the lower branch with probability {}/65536",
                probability.raw()
            ),
        }
    }
}

impl std::error::Error for CoderError {}

pub type Result<T> = std::result::Result<T, CoderError>;

/// Convert a 16-bit Bernoulli probability into an integer split of `range`.
#[inline(always)]
fn split(range: u32, probability: FractionalU16) -> u32 {
    ((range as u64 * probability.raw() as u64) >> PROBABILITY_BITS) as u32
}

/// FIFO byte-oriented range encoder whose only probabilistic primitive is a
/// Bernoulli decision on a 16-bit probability grid.
pub struct RangeEncoder {
    low: u64,
    range: u32,
    cache: u8,
    cache_size: u64,
    output: Vec<u8>,
}

impl Default for RangeEncoder {
    fn default() -> Self {
        Self::new()
    }
}

impl RangeEncoder {
    pub fn new() -> Self {
        Self {
            low: 0,
            range: u32::MAX,
            cache: 0,
            cache_size: 1,
            output: Vec::new(),
        }
    }

    /// Encode one Bernoulli outcome.
    ///
    /// `lower == true` selects `[0, p)`; `false` selects `[p, 1)`.
    #[inline]
    pub fn put(&mut self, probability: FractionalU16, lower: bool) -> Result<()> {
        let split = split(self.range, probability);

        if lower {
            if split == 0 {
                return Err(CoderError::ImpossibleOutcome { probability });
            }
            self.range = split;
        } else {
            self.low += split as u64;
            self.range -= split;
        }

        self.renormalize();
        Ok(())
    }

    pub fn finish(mut self) -> Vec<u8> {
        for _ in 0..5 {
            self.shift_low();
        }
        self.output
    }

    #[inline(always)]
    fn renormalize(&mut self) {
        while self.range < RANGE_TOP {
            self.range <<= 8;
            self.shift_low();
        }
    }

    #[inline(always)]
    fn shift_low(&mut self) {
        let low = self.low as u32;
        let carry = (self.low >> 32) as u8;

        if low < 0xff00_0000 || carry != 0 {
            let mut byte = self.cache;
            loop {
                self.output.push(byte.wrapping_add(carry));
                self.cache_size -= 1;
                if self.cache_size == 0 {
                    break;
                }
                byte = 0xff;
            }
            self.cache = (low >> 24) as u8;
        }

        self.cache_size += 1;
        self.low = (low << 8) as u64;
    }
}

/// FIFO byte-oriented range decoder for [`RangeEncoder`].
///
/// The arithmetic state is two 32-bit words. Conceptually the residual
/// fraction is `code / range`, but it is never explicitly divided.
pub struct RangeDecoder<'a> {
    code: u32,
    range: u32,
    input: &'a [u8],
    position: usize,
}

impl<'a> RangeDecoder<'a> {
    pub fn new(input: &'a [u8]) -> Result<Self> {
        let mut decoder = Self {
            code: 0,
            range: u32::MAX,
            input,
            position: 0,
        };

        for _ in 0..5 {
            decoder.code = (decoder.code << 8) | decoder.read_byte()? as u32;
        }

        Ok(decoder)
    }

    /// Extract one Bernoulli and rescale into whichever bucket contains the
    /// hidden fraction.
    ///
    /// In ideal rational notation:
    ///
    /// ```text
    /// true:  x <- x / p
    /// false: x <- (x - p) / (1 - p)
    /// ```
    ///
    /// With `x = code / range`, both transformations are implemented by one
    /// multiply-derived split, a comparison, and integer updates. No division
    /// or reciprocal is required.
    #[inline]
    pub fn test(&mut self, probability: FractionalU16) -> Result<bool> {
        let split = split(self.range, probability);
        let lower = self.code < split;

        if lower {
            self.range = split;
        } else {
            self.code -= split;
            self.range -= split;
        }

        self.renormalize()?;
        Ok(lower)
    }

    #[inline]
    pub const fn bytes_consumed(&self) -> usize {
        self.position
    }

    #[inline(always)]
    fn renormalize(&mut self) -> Result<()> {
        while self.range < RANGE_TOP {
            self.range <<= 8;
            self.code = (self.code << 8) | self.read_byte()? as u32;
        }
        Ok(())
    }

    #[inline(always)]
    fn read_byte(&mut self) -> Result<u8> {
        let byte = *self
            .input
            .get(self.position)
            .ok_or(CoderError::UnexpectedEof)?;
        self.position += 1;
        Ok(byte)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn next_u64(state: &mut u64) -> u64 {
        *state ^= *state << 13;
        *state ^= *state >> 7;
        *state ^= *state << 17;
        *state
    }

    #[test]
    fn round_trips_long_varying_probability_stream() {
        let mut random = 0x0123_4567_89ab_cdef_u64;
        let mut decisions = Vec::new();
        let mut encoder = RangeEncoder::new();

        for _ in 0..100_000 {
            let raw = (next_u64(&mut random) as u16) | 1;
            let probability = FractionalU16::from_raw(raw);
            let lower = next_u64(&mut random) >> 63 != 0;
            decisions.push((probability, lower));
            encoder.put(probability, lower).unwrap();
        }

        let encoded = encoder.finish();
        let mut decoder = RangeDecoder::new(&encoded).unwrap();

        for (probability, expected) in decisions {
            assert_eq!(decoder.test(probability).unwrap(), expected);
        }
    }

    #[test]
    fn handles_extreme_representable_probabilities() {
        let decisions = [
            (FractionalU16::from_raw(1), true),
            (FractionalU16::from_raw(1), false),
            (FractionalU16::HALF, true),
            (FractionalU16::HALF, false),
            (FractionalU16::from_raw(u16::MAX), true),
            (FractionalU16::from_raw(u16::MAX), false),
        ];

        let mut encoder = RangeEncoder::new();
        for &(probability, lower) in &decisions {
            encoder.put(probability, lower).unwrap();
        }

        let encoded = encoder.finish();
        let mut decoder = RangeDecoder::new(&encoded).unwrap();
        for &(probability, expected) in &decisions {
            assert_eq!(decoder.test(probability).unwrap(), expected);
        }
    }

    #[test]
    fn zero_probability_is_a_deterministic_upper_branch() {
        let mut encoder = RangeEncoder::new();
        encoder.put(FractionalU16::ZERO, false).unwrap();
        encoder.put(FractionalU16::HALF, true).unwrap();

        let encoded = encoder.finish();
        let mut decoder = RangeDecoder::new(&encoded).unwrap();
        assert!(!decoder.test(FractionalU16::ZERO).unwrap());
        assert!(decoder.test(FractionalU16::HALF).unwrap());
    }

    #[test]
    fn rejects_impossible_lower_branch() {
        let mut encoder = RangeEncoder::new();
        assert_eq!(
            encoder.put(FractionalU16::ZERO, true),
            Err(CoderError::ImpossibleOutcome {
                probability: FractionalU16::ZERO,
            })
        );
    }
}
