use num_bigint::BigUint;
use std::sync::OnceLock;

/// Number of edges of the next 257-way symbol partition, including both
/// endpoints of the current symbol cylinder.
pub const SYMBOL_EDGES: usize = 258;
/// Number of edges of the next radix-256 byte partition, including both
/// endpoints of the current byte cylinder.
pub const BYTE_EDGES: usize = 257;
/// Total ordered edge events in the complete local frontier.
pub const FRONTIER_EVENTS: usize = SYMBOL_EDGES + BYTE_EDGES;

/// A complete symbol/byte refinement frontier packed into exactly 512 bits.
///
/// The decoded frontier contains 258 ordered symbol edges and 257 ordered byte
/// edges. A literal event bitvector would need 515 bits, but the family counts
/// are fixed, so there are only `C(515, 258)` possible interleavings. That is
/// about 510.17 bits of information. We store the combinatorial rank of the
/// interleaving in 64 little-endian bytes.
///
/// This is deliberately a semantic/reference representation. The existing raw
/// [`crate::BoundaryMerge`] remains the fast rank/select experiment. Once the
/// transducer semantics are nailed down, we can decide how much endpoint state
/// belongs in the hot representation versus the walking driver.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
#[repr(transparent)]
pub struct Frontier512 {
    bytes: [u8; 64],
}

impl Frontier512 {
    pub const BYTES: usize = 64;
    pub const BITS: usize = 512;

    pub const fn bytes(&self) -> &[u8; 64] {
        &self.bytes
    }

    /// Expand the enumerative representation into edge positions convenient
    /// for the exact/reference walking policy.
    pub fn decode(&self) -> DecodedFrontier {
        let mut rank = BigUint::from_bytes_le(&self.bytes);
        assert!(&rank < binomial(FRONTIER_EVENTS, SYMBOL_EDGES));

        let mut events = [false; FRONTIER_EVENTS];
        let mut upper = FRONTIER_EVENTS;

        // Combinadic unranking. If the set-bit positions are p_1 < ... < p_k,
        // the stored rank is sum_i C(p_i, i).
        for i in (1..=SYMBOL_EDGES).rev() {
            let mut lo = i - 1;
            let mut hi = upper;
            while lo + 1 < hi {
                let mid = (lo + hi) / 2;
                if binomial(mid, i) <= &rank {
                    lo = mid;
                } else {
                    hi = mid;
                }
            }

            events[lo] = true;
            rank -= binomial(lo, i);
            upper = lo;
        }
        debug_assert_eq!(rank, BigUint::from(0u8));

        DecodedFrontier::from_events(&events)
    }

    pub(crate) fn from_events(events: &[bool; FRONTIER_EVENTS]) -> Self {
        let mut rank = BigUint::from(0u8);
        let mut symbol_i = 0usize;

        for (pos, &is_symbol) in events.iter().enumerate() {
            if is_symbol {
                symbol_i += 1;
                rank += binomial(pos, symbol_i);
            }
        }

        assert_eq!(symbol_i, SYMBOL_EDGES);
        assert!(&rank < binomial(FRONTIER_EVENTS, SYMBOL_EDGES));
        assert!(rank.bits() <= Self::BITS as u64);

        let encoded = rank.to_bytes_le();
        assert!(encoded.len() <= Self::BYTES);
        let mut bytes = [0u8; Self::BYTES];
        bytes[..encoded.len()].copy_from_slice(&encoded);
        Self { bytes }
    }
}

/// Expanded edge positions for the exact walking policy.
///
/// Positions are indices in the 515-event sorted merge. Within each family,
/// edge identity is implicit in rank order.
#[derive(Clone, Debug)]
pub struct DecodedFrontier {
    symbol_pos: [u16; SYMBOL_EDGES],
    byte_pos: [u16; BYTE_EDGES],
}

impl DecodedFrontier {
    fn from_events(events: &[bool; FRONTIER_EVENTS]) -> Self {
        let mut symbol_pos = [0u16; SYMBOL_EDGES];
        let mut byte_pos = [0u16; BYTE_EDGES];
        let mut symbol_i = 0usize;
        let mut byte_i = 0usize;

        for (pos, &is_symbol) in events.iter().enumerate() {
            if is_symbol {
                symbol_pos[symbol_i] = pos as u16;
                symbol_i += 1;
            } else {
                byte_pos[byte_i] = pos as u16;
                byte_i += 1;
            }
        }

        debug_assert_eq!(symbol_i, SYMBOL_EDGES);
        debug_assert_eq!(byte_i, BYTE_EDGES);
        Self {
            symbol_pos,
            byte_pos,
        }
    }

    /// A byte child that *strictly* contains the whole current symbol cylinder.
    ///
    /// Cross-family equality is intentionally treated conservatively. The
    /// frontier stores only total order (byte edge before symbol edge on ties),
    /// not a separate equality bit. Delaying a decision on an exact tie is safe;
    /// further refinement resolves all finite streams once finalization chooses
    /// a byte cylinder strictly inside the final symbol cylinder.
    pub(crate) fn forced_byte_strict(&self) -> Option<u8> {
        let symbol_lo = self.symbol_pos[0];
        let symbol_hi = self.symbol_pos[SYMBOL_EDGES - 1];
        let byte_edges_before_lo = self
            .byte_pos
            .partition_point(|&position| position < symbol_lo);
        let byte = byte_edges_before_lo.checked_sub(1)?;
        if byte >= 256 {
            return None;
        }

        (symbol_hi < self.byte_pos[byte + 1]).then_some(byte as u8)
    }

    /// A symbol child that *strictly* contains the whole current byte cylinder.
    pub(crate) fn forced_symbol_strict(&self) -> Option<usize> {
        let byte_lo = self.byte_pos[0];
        let byte_hi = self.byte_pos[BYTE_EDGES - 1];
        let symbol_edges_before_lo = self
            .symbol_pos
            .partition_point(|&position| position < byte_lo);
        let symbol = symbol_edges_before_lo.checked_sub(1)?;
        if symbol >= 257 {
            return None;
        }

        (byte_hi < self.symbol_pos[symbol + 1]).then_some(symbol)
    }

    /// First byte child whose left edge lies strictly inside the current symbol
    /// cylinder. Used only by canonical finalization.
    pub(crate) fn first_byte_with_left_inside_symbol(&self) -> Option<u8> {
        let symbol_lo = self.symbol_pos[0];
        let symbol_hi = self.symbol_pos[SYMBOL_EDGES - 1];
        (0..256)
            .find(|&byte| {
                symbol_lo < self.byte_pos[byte] && self.byte_pos[byte] < symbol_hi
            })
            .map(|byte| byte as u8)
    }

    /// Byte child containing the current symbol left endpoint, under the fixed
    /// byte-before-symbol tie convention.
    pub(crate) fn byte_containing_symbol_left(&self) -> Option<u8> {
        let symbol_lo = self.symbol_pos[0];
        let byte_edges_before_lo = self
            .byte_pos
            .partition_point(|&position| position < symbol_lo);
        let byte = byte_edges_before_lo.checked_sub(1)?;
        (byte < 256).then_some(byte as u8)
    }

    pub(crate) fn byte_right_strictly_before_symbol_right(&self) -> bool {
        self.byte_pos[BYTE_EDGES - 1] < self.symbol_pos[SYMBOL_EDGES - 1]
    }

    #[cfg(test)]
    fn event_string(&self) -> String {
        let mut out = vec![b'?'; FRONTIER_EVENTS];
        for &position in &self.symbol_pos {
            out[position as usize] = b'S';
        }
        for &position in &self.byte_pos {
            out[position as usize] = b'B';
        }
        String::from_utf8(out).expect("frontier event alphabet is ASCII")
    }
}

fn binomial(n: usize, k: usize) -> &'static BigUint {
    static TABLE: OnceLock<Vec<BigUint>> = OnceLock::new();
    let table = TABLE.get_or_init(|| {
        let stride = SYMBOL_EDGES + 1;
        let mut table = vec![BigUint::from(0u8); (FRONTIER_EVENTS + 1) * stride];
        table[0] = BigUint::from(1u8);

        for n in 1..=FRONTIER_EVENTS {
            table[n * stride] = BigUint::from(1u8);
            let max_k = n.min(SYMBOL_EDGES);
            for k in 1..=max_k {
                let left = table[(n - 1) * stride + (k - 1)].clone();
                let right = table[(n - 1) * stride + k].clone();
                table[n * stride + k] = left + right;
            }
        }
        table
    });

    &table[n * (SYMBOL_EDGES + 1) + k]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn complete_frontier_still_fits_512_bits() {
        let possibilities = binomial(FRONTIER_EVENTS, SYMBOL_EDGES);
        assert_eq!(possibilities.bits(), 511);
        assert_eq!(core::mem::size_of::<Frontier512>(), 64);
    }

    #[test]
    fn combinadic_roundtrip() {
        let mut events = [false; FRONTIER_EVENTS];
        // A deliberately uneven but valid fixed-weight event pattern.
        let mut remaining_symbols = SYMBOL_EDGES;
        for (pos, event) in events.iter_mut().enumerate() {
            let remaining = FRONTIER_EVENTS - pos;
            if remaining_symbols != 0
                && (pos % 3 == 0 || remaining == remaining_symbols)
            {
                *event = true;
                remaining_symbols -= 1;
            }
        }
        assert_eq!(remaining_symbols, 0);

        let expected: String = events
            .iter()
            .map(|&is_symbol| if is_symbol { 'S' } else { 'B' })
            .collect();
        let packed = Frontier512::from_events(&events);
        assert_eq!(packed.decode().event_string(), expected);
    }
}
