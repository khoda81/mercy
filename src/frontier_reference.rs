use num_bigint::BigUint;

use crate::frontier::{Frontier512, FRONTIER_EVENTS};
use crate::{ByteSymbol, Transducer};

const SYMBOLS: usize = 257;

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
        let total = BigUint::from(total);
        let width = &self.hi - &self.lo;
        Self {
            lo: &self.lo * &total + &width * BigUint::from(part_lo),
            hi: &self.lo * &total + &width * BigUint::from(part_hi),
            den: &self.den * total,
        }
    }

    fn edge(&self, part: u128, total: u128) -> Fraction {
        let total = BigUint::from(total);
        let width = &self.hi - &self.lo;
        Fraction {
            num: &self.lo * &total + width * BigUint::from(part),
            den: &self.den * total,
        }
    }
}

#[derive(Clone, Debug)]
struct Fraction {
    num: BigUint,
    den: BigUint,
}

fn fraction_le(left: &Fraction, right: &Fraction) -> bool {
    &left.num * &right.den <= &right.num * &left.den
}

/// Exact arbitrary-precision implementation of the purified transducer API.
///
/// It owns two independently walkable prefixes: one in the 257-way symbol tree
/// and one in the radix-256 byte tree. `frontier()` exposes only the ordering of
/// their next child edges. The compression/decompression drivers below never
/// inspect these exact intervals; they walk exclusively through the public
/// three-method transducer interface plus the returned frontier.
pub struct ReferenceFrontierModel {
    weights: [u64; SYMBOLS],
    cumulative: [u128; SYMBOLS + 1],
    total: u128,
    symbol_interval: Interval,
    byte_interval: Interval,
}

impl ReferenceFrontierModel {
    pub fn new(weights: [u64; SYMBOLS]) -> Self {
        let mut cumulative = [0u128; SYMBOLS + 1];
        for (index, &weight) in weights.iter().enumerate() {
            cumulative[index + 1] = cumulative[index] + weight as u128;
        }
        let total = cumulative[SYMBOLS];
        assert!(total != 0, "probability model must have non-zero total mass");

        Self {
            weights,
            cumulative,
            total,
            symbol_interval: Interval::unit(),
            byte_interval: Interval::unit(),
        }
    }

    pub fn uniform() -> Self {
        Self::new([1; SYMBOLS])
    }

    pub fn weights(&self) -> &[u64; SYMBOLS] {
        &self.weights
    }
}

impl Transducer for ReferenceFrontierModel {
    type Symbol = ByteSymbol;

    fn push_symbol(&mut self, symbol: Self::Symbol) {
        let index = symbol_index(symbol);
        self.symbol_interval = self.symbol_interval.child(
            self.cumulative[index],
            self.cumulative[index + 1],
            self.total,
        );
    }

    fn push_byte(&mut self, byte: u8) {
        self.byte_interval = self
            .byte_interval
            .child(byte as u128, byte as u128 + 1, 256);
    }

    fn frontier(&self) -> Frontier512 {
        let symbol_edges: [Fraction; SYMBOLS + 1] = std::array::from_fn(|index| {
            self.symbol_interval
                .edge(self.cumulative[index], self.total)
        });
        let byte_edges: [Fraction; 257] = std::array::from_fn(|index| {
            self.byte_interval.edge(index as u128, 256)
        });

        let mut events = [false; FRONTIER_EVENTS];
        let mut symbol_i = 0usize;
        let mut byte_i = 0usize;

        for event in &mut events {
            let take_byte = if byte_i == byte_edges.len() {
                false
            } else if symbol_i == symbol_edges.len() {
                true
            } else {
                // Fixed cross-family tie convention: byte edge first.
                fraction_le(&byte_edges[byte_i], &symbol_edges[symbol_i])
            };

            if take_byte {
                byte_i += 1;
            } else {
                *event = true;
                symbol_i += 1;
            }
        }

        debug_assert_eq!(symbol_i, symbol_edges.len());
        debug_assert_eq!(byte_i, byte_edges.len());
        Frontier512::from_events(&events)
    }
}

/// Compress through *only* the purified transducer API.
///
/// The model never emits bytes. The driver advances the symbol prefix, examines
/// the frontier, and advances the byte prefix whenever a byte child is forced.
/// Finalization chooses a canonical radix path that ends strictly inside the
/// final EOS cylinder, which guarantees the decoder can eventually force EOS.
pub fn frontier_compress(weights: [u64; SYMBOLS], input: &[u8]) -> Vec<u8> {
    let mut model = ReferenceFrontierModel::new(weights);
    let mut output = Vec::new();

    for symbol in input
        .iter()
        .copied()
        .map(ByteSymbol::Byte)
        .chain([ByteSymbol::Eos])
    {
        assert!(
            model.weights()[symbol_index(symbol)] != 0,
            "cannot encode a zero-probability symbol"
        );
        model.push_symbol(symbol);
        push_forced_bytes(&mut model, &mut output);
    }

    finish_bytes(&mut model, &mut output);
    output
}

/// Inverse of [`frontier_compress`], again using only the three-method API and
/// frontier ordering. Exact-boundary ties are handled conservatively by waiting
/// for more byte refinement; finalization guarantees a strictly interior final
/// cylinder so every finite valid source stream eventually reaches EOS.
pub fn frontier_decompress(weights: [u64; SYMBOLS], input: &[u8]) -> Option<Vec<u8>> {
    let mut model = ReferenceFrontierModel::new(weights);
    let mut output = Vec::new();

    for &byte in input {
        model.push_byte(byte);
        loop {
            let Some(symbol_index) = model.frontier().decode().forced_symbol_strict() else {
                break;
            };
            let symbol = symbol_from_index(symbol_index);
            model.push_symbol(symbol);
            match symbol {
                ByteSymbol::Byte(byte) => output.push(byte),
                ByteSymbol::Eos => return Some(output),
            }
        }
    }

    None
}

fn push_forced_bytes<T>(model: &mut T, output: &mut Vec<u8>)
where
    T: Transducer<Symbol = ByteSymbol>,
{
    loop {
        let Some(byte) = model.frontier().decode().forced_byte_strict() else {
            break;
        };
        model.push_byte(byte);
        output.push(byte);
    }
}

fn finish_bytes<T>(model: &mut T, output: &mut Vec<u8>)
where
    T: Transducer<Symbol = ByteSymbol>,
{
    // Once true, the current byte left endpoint is known (from the transition
    // we just chose) to lie strictly inside the final symbol interval. Refining
    // byte child zero preserves that left endpoint while shrinking the right
    // endpoint until the whole byte cylinder lies strictly inside the symbol
    // cylinder.
    let mut left_inside = false;

    loop {
        let frontier = model.frontier().decode();

        if left_inside {
            if frontier.byte_right_strictly_before_symbol_right() {
                return;
            }
            model.push_byte(0);
            output.push(0);
            continue;
        }

        if let Some(byte) = frontier.first_byte_with_left_inside_symbol() {
            model.push_byte(byte);
            output.push(byte);
            left_inside = true;
            continue;
        }

        // The final symbol cylinder is narrower than the current byte grid cell
        // around its left endpoint. Follow that overlapping cell and ask for a
        // finer frontier. A sufficiently fine radix grid must eventually place
        // an internal byte edge strictly inside every non-empty symbol interval.
        let byte = frontier
            .byte_containing_symbol_left()
            .expect("forced-byte walking keeps the final symbol cylinder inside the byte cylinder");
        model.push_byte(byte);
        output.push(byte);
    }
}

fn symbol_index(symbol: ByteSymbol) -> usize {
    match symbol {
        ByteSymbol::Byte(byte) => byte as usize,
        ByteSymbol::Eos => 256,
    }
}

fn symbol_from_index(index: usize) -> ByteSymbol {
    if index == 256 {
        ByteSymbol::Eos
    } else {
        ByteSymbol::Byte(index as u8)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn skewed_weights() -> [u64; SYMBOLS] {
        let mut weights = [1u64; SYMBOLS];
        weights[b' ' as usize] = 40;
        weights[b'e' as usize] = 30;
        weights[b't' as usize] = 20;
        weights[b'a' as usize] = 16;
        weights[256] = 3;
        weights
    }

    fn ideal_bits(weights: &[u64; SYMBOLS], input: &[u8]) -> f64 {
        let total: f64 = weights.iter().map(|&weight| weight as f64).sum();
        input
            .iter()
            .map(|&byte| -(weights[byte as usize] as f64 / total).log2())
            .sum::<f64>()
            - (weights[256] as f64 / total).log2()
    }

    fn assert_roundtrip(weights: [u64; SYMBOLS], input: &[u8]) {
        let encoded = frontier_compress(weights, input);
        let decoded = frontier_decompress(weights, &encoded)
            .expect("canonical finalization must force EOS");
        assert_eq!(decoded, input);

        let ideal = ideal_bits(&weights, input);
        let physical = (encoded.len() * 8) as f64;
        assert!(physical + 1e-9 >= ideal);
        assert!(
            physical - ideal < 9.0,
            "radix-256 interval code exceeded the <9-bit termination bound: physical={physical}, ideal={ideal}"
        );
    }

    #[test]
    fn purified_api_roundtrips_real_probability_model() {
        let weights = skewed_weights();
        for input in [
            b"".as_slice(),
            b"a".as_slice(),
            b"hello mercy".as_slice(),
            b"aaaaaaaaaaaaaaaa".as_slice(),
            b"the byte stream and symbol stream constrain each other".as_slice(),
        ] {
            assert_roundtrip(weights, input);
        }
    }

    #[test]
    fn exact_boundary_heavy_uniform_model_roundtrips() {
        let weights = [1u64; SYMBOLS];
        assert_roundtrip(weights, &[0; 32]);
        assert_roundtrip(weights, &[255; 32]);
        assert_roundtrip(weights, b"mercy");
    }

    #[test]
    fn randomized_positive_models_roundtrip_and_stay_within_nine_bits() {
        let mut rng = 0x9e37_79b9_7f4a_7c15u64;
        for _case in 0..64 {
            let mut weights = [0u64; SYMBOLS];
            for weight in &mut weights {
                rng ^= rng << 13;
                rng ^= rng >> 7;
                rng ^= rng << 17;
                *weight = 1 + (rng & 31);
            }
            weights[256] = 1 + (weights[256] & 7);

            rng ^= rng << 13;
            rng ^= rng >> 7;
            rng ^= rng << 17;
            let len = (rng as usize) & 15;
            let mut input = Vec::with_capacity(len);
            for _ in 0..len {
                rng ^= rng << 13;
                rng ^= rng >> 7;
                rng ^= rng << 17;
                input.push(rng as u8);
            }

            assert_roundtrip(weights, &input);
        }
    }

    #[test]
    fn byte_quantization_prevents_a_universal_plus_one_bit_bound() {
        // Any whole-byte stream has length in multiples of eight bits. The
        // entropy bound tested above is therefore the appropriate physical-file
        // claim for this radix-256 API. A literal +1-bit guarantee requires a
        // bit-level/partial-byte finalization surface.
        let weights = skewed_weights();
        let encoded = frontier_compress(weights, b"");
        let ideal = ideal_bits(&weights, b"");
        assert!((encoded.len() * 8) as f64 - ideal > 1.0);
    }
}
