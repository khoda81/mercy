use num_bigint::BigUint;

use crate::partition::SYMBOLS;
use crate::{BoundaryMerge, ConstraintModel};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ByteSymbol {
    Byte(u8),
    Eos,
}

impl ByteSymbol {
    #[inline]
    const fn index(self) -> usize {
        match self {
            Self::Byte(byte) => byte as usize,
            Self::Eos => 256,
        }
    }

    #[inline]
    const fn from_index(index: usize) -> Self {
        if index == 256 {
            Self::Eos
        } else {
            Self::Byte(index as u8)
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ReferenceError {
    ZeroTotalWeight,
    ZeroWeightSymbol(ByteSymbol),
    InconsistentConstraint,
    EosAlreadyConstrained,
    EosNotConstrained,
    MissingEos,
}

#[derive(Clone, Debug)]
struct Interval {
    lo: BigUint,
    hi: BigUint,
    den: BigUint,
}

impl Interval {
    fn unit() -> Self {
        Self {
            lo: BigUint::from(0u8),
            hi: BigUint::from(1u8),
            den: BigUint::from(1u8),
        }
    }

    fn child(&self, part_lo: u128, part_hi: u128, total: u128) -> Self {
        debug_assert!(part_lo <= part_hi);
        debug_assert!(part_hi <= total);
        debug_assert!(total != 0);

        let total = BigUint::from(total);
        let part_lo = BigUint::from(part_lo);
        let part_hi = BigUint::from(part_hi);
        let width = &self.hi - &self.lo;

        Self {
            lo: &self.lo * &total + &width * part_lo,
            hi: &self.lo * &total + &width * part_hi,
            den: &self.den * total,
        }
    }

    #[inline]
    fn byte_child(&self, byte: u8) -> Self {
        self.child(byte as u128, byte as u128 + 1, 256)
    }

    fn contains(&self, other: &Self) -> bool {
        fraction_le(&self.lo, &self.den, &other.lo, &other.den)
            && fraction_le(&other.hi, &other.den, &self.hi, &self.den)
    }

    fn intersects(&self, other: &Self) -> bool {
        fraction_lt(&self.lo, &self.den, &other.hi, &other.den)
            && fraction_lt(&other.lo, &other.den, &self.hi, &self.den)
    }

    fn is_empty(&self) -> bool {
        self.lo == self.hi
    }

    fn midpoint(&self) -> Fraction {
        Fraction {
            num: &self.lo + &self.hi,
            den: &self.den * BigUint::from(2u8),
        }
    }

    fn contains_point(&self, point: &Fraction) -> bool {
        fraction_le(&self.lo, &self.den, &point.num, &point.den)
            && fraction_lt(&point.num, &point.den, &self.hi, &self.den)
    }
}

#[derive(Clone, Debug)]
struct Fraction {
    num: BigUint,
    den: BigUint,
}

fn fraction_le(a_num: &BigUint, a_den: &BigUint, b_num: &BigUint, b_den: &BigUint) -> bool {
    a_num * b_den <= b_num * a_den
}

fn fraction_lt(a_num: &BigUint, a_den: &BigUint, b_num: &BigUint, b_den: &BigUint) -> bool {
    a_num * b_den < b_num * a_den
}

/// Deliberately slow, exact correctness oracle for the stream-constraint idea.
///
/// This implementation keeps exact arbitrary-precision rational intervals for
/// the symbol stream and byte stream. It is not intended to be fast. Its job is
/// to make the semantics painfully literal before we replace the internals with
/// fixed-width arithmetic/range machinery.
pub struct ReferenceModel {
    weights: [u64; SYMBOLS],
    cumulative: [u128; SYMBOLS + 1],
    total: u128,
    symbol_interval: Interval,
    byte_interval: Interval,
    byte_output: Vec<u8>,
    symbol_output: Vec<ByteSymbol>,
    eos_constrained: bool,
    eos_decoded: bool,
}

impl ReferenceModel {
    pub fn new(weights: [u64; SYMBOLS]) -> Result<Self, ReferenceError> {
        let mut cumulative = [0u128; SYMBOLS + 1];
        for (index, &weight) in weights.iter().enumerate() {
            cumulative[index + 1] = cumulative[index] + weight as u128;
        }
        let total = cumulative[SYMBOLS];
        if total == 0 {
            return Err(ReferenceError::ZeroTotalWeight);
        }

        Ok(Self {
            weights,
            cumulative,
            total,
            symbol_interval: Interval::unit(),
            byte_interval: Interval::unit(),
            byte_output: Vec::new(),
            symbol_output: Vec::new(),
            eos_constrained: false,
            eos_decoded: false,
        })
    }

    pub fn uniform() -> Self {
        Self::new([1u64; SYMBOLS]).expect("uniform model has non-zero mass")
    }

    fn symbol_child(&self, index: usize) -> Interval {
        self.symbol_interval.child(
            self.cumulative[index],
            self.cumulative[index + 1],
            self.total,
        )
    }

    /// Constrain the symbol side and return the byte prefix that became forced.
    pub fn try_push_symbol(&mut self, symbol: ByteSymbol) -> Result<&[u8], ReferenceError> {
        self.byte_output.clear();

        if self.eos_constrained {
            return Err(ReferenceError::EosAlreadyConstrained);
        }

        let index = symbol.index();
        if self.weights[index] == 0 {
            return Err(ReferenceError::ZeroWeightSymbol(symbol));
        }

        let next = self.symbol_child(index);
        if next.is_empty() || !next.intersects(&self.byte_interval) {
            return Err(ReferenceError::InconsistentConstraint);
        }
        self.symbol_interval = next;

        loop {
            let mut forced = None;
            for byte in u8::MIN..=u8::MAX {
                let child = self.byte_interval.byte_child(byte);
                if child.contains(&self.symbol_interval) {
                    forced = Some((byte, child));
                    break;
                }
            }

            let Some((byte, child)) = forced else {
                break;
            };
            self.byte_output.push(byte);
            self.byte_interval = child;
        }

        if symbol == ByteSymbol::Eos {
            self.eos_constrained = true;
        }

        Ok(&self.byte_output)
    }

    /// Constrain the byte side and return the symbol prefix that became forced.
    pub fn try_push_byte(&mut self, byte: u8) -> Result<&[ByteSymbol], ReferenceError> {
        self.symbol_output.clear();

        if self.eos_decoded {
            return Ok(&self.symbol_output);
        }

        let next = self.byte_interval.byte_child(byte);
        if !next.intersects(&self.symbol_interval) {
            return Err(ReferenceError::InconsistentConstraint);
        }
        self.byte_interval = next;

        loop {
            let mut forced = None;
            for index in 0..SYMBOLS {
                if self.weights[index] == 0 {
                    continue;
                }
                let child = self.symbol_child(index);
                if child.contains(&self.byte_interval) {
                    forced = Some((ByteSymbol::from_index(index), child));
                    break;
                }
            }

            let Some((symbol, child)) = forced else {
                break;
            };
            self.symbol_output.push(symbol);
            self.symbol_interval = child;

            if symbol == ByteSymbol::Eos {
                self.eos_decoded = true;
                break;
            }
        }

        Ok(&self.symbol_output)
    }

    /// Choose a canonical compatible byte continuation after EOS.
    ///
    /// Before finalization the symbol interval is contained in the byte interval.
    /// We follow the midpoint of the final symbol interval through radix-256
    /// children until the selected byte cylinder lies completely inside it.
    pub fn try_finish(&mut self) -> Result<&[u8], ReferenceError> {
        self.byte_output.clear();
        if !self.eos_constrained {
            return Err(ReferenceError::EosNotConstrained);
        }

        let point = self.symbol_interval.midpoint();
        if !self.byte_interval.contains_point(&point) {
            return Err(ReferenceError::InconsistentConstraint);
        }

        while !self.symbol_interval.contains(&self.byte_interval) {
            let mut chosen = None;
            for byte in u8::MIN..=u8::MAX {
                let child = self.byte_interval.byte_child(byte);
                if child.contains_point(&point) {
                    chosen = Some((byte, child));
                    break;
                }
            }

            let (byte, child) = chosen.expect("a point inside an interval belongs to one byte child");
            self.byte_output.push(byte);
            self.byte_interval = child;
        }

        Ok(&self.byte_output)
    }

    pub fn partition_view(&self) -> BoundaryMerge {
        BoundaryMerge::from_weights(&self.weights).expect("ReferenceModel rejects zero total mass")
    }
}

impl ConstraintModel for ReferenceModel {
    type Symbol = ByteSymbol;

    fn push_symbol(&mut self, symbol: Self::Symbol) -> &[u8] {
        self.try_push_symbol(symbol)
            .expect("inconsistent symbol constraint")
    }

    fn push_byte(&mut self, byte: u8) -> &[Self::Symbol] {
        self.try_push_byte(byte).expect("inconsistent byte constraint")
    }

    fn partition(&self) -> BoundaryMerge {
        self.partition_view()
    }
}

pub fn compress(weights: [u64; SYMBOLS], input: &[u8]) -> Result<Vec<u8>, ReferenceError> {
    let mut model = ReferenceModel::new(weights)?;
    let mut output = Vec::new();

    for &byte in input {
        output.extend_from_slice(model.try_push_symbol(ByteSymbol::Byte(byte))?);
    }
    output.extend_from_slice(model.try_push_symbol(ByteSymbol::Eos)?);
    output.extend_from_slice(model.try_finish()?);
    Ok(output)
}

pub fn decompress(weights: [u64; SYMBOLS], input: &[u8]) -> Result<Vec<u8>, ReferenceError> {
    let mut model = ReferenceModel::new(weights)?;
    let mut output = Vec::new();

    for &byte in input {
        let symbols = model.try_push_byte(byte)?.to_vec();
        for symbol in symbols {
            match symbol {
                ByteSymbol::Byte(byte) => output.push(byte),
                ByteSymbol::Eos => return Ok(output),
            }
        }
    }

    Err(ReferenceError::MissingEos)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn uniform_roundtrip() {
        let weights = [1u64; SYMBOLS];
        let input = b"hello mercy beaucoup";
        let encoded = compress(weights, input).unwrap();
        let decoded = decompress(weights, &encoded).unwrap();
        assert_eq!(decoded, input);
    }

    #[test]
    fn skewed_roundtrip() {
        let mut weights = [1u64; SYMBOLS];
        weights[b' ' as usize] = 40;
        weights[b'e' as usize] = 30;
        weights[b't' as usize] = 20;
        weights[b'a' as usize] = 16;
        weights[256] = 3;

        let input = b"the byte stream and symbol stream constrain each other";
        let encoded = compress(weights, input).unwrap();
        let decoded = decompress(weights, &encoded).unwrap();
        assert_eq!(decoded, input);
    }

    #[test]
    fn empty_stream_roundtrip() {
        let weights = [1u64; SYMBOLS];
        let encoded = compress(weights, b"").unwrap();
        let decoded = decompress(weights, &encoded).unwrap();
        assert!(decoded.is_empty());
    }

    #[test]
    fn pushes_have_variable_rate() {
        let mut weights = [1u64; SYMBOLS];
        weights[b'a' as usize] = 10_000;
        weights[256] = 10;

        let mut encoder = ReferenceModel::new(weights).unwrap();
        assert!(encoder.try_push_symbol(ByteSymbol::Byte(b'a')).unwrap().is_empty());

        let encoded = compress(weights, b"aaaaaaaaaaaaaaaaaaaaaaaa").unwrap();
        let mut decoder = ReferenceModel::new(weights).unwrap();
        let mut emitted_multiple = false;
        for byte in encoded {
            if decoder.try_push_byte(byte).unwrap().len() > 1 {
                emitted_multiple = true;
                break;
            }
        }
        assert!(emitted_multiple);
    }
}
