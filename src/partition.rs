use core::fmt;

/// Number of ordered source symbols: all byte values plus EOS.
pub const SYMBOLS: usize = 257;
/// Number of boundaries between 257 ordered source symbols.
pub const SYMBOL_BOUNDARIES: usize = 256;
/// Number of radix-256 bucket right edges, including the terminal edge at 1.
pub const BYTE_CUTS: usize = 256;
/// Total events in the merged boundary stream.
pub const EVENTS: usize = SYMBOL_BOUNDARIES + BYTE_CUTS;

/// Exactly one cache-line-sized (on common CPUs) coarse view of a distribution.
///
/// Event encoding, from low probability coordinate to high:
/// - `1`: next symbol boundary
/// - `0`: next radix-256 bucket right edge
///
/// Each family is already internally ordered, so event identities are implicit.
/// The kth 1-bit is symbol boundary k; the kth 0-bit is byte-cut k.
///
/// Ties use `byte-cut before symbol-boundary`. This matches the bit ordering
/// `0 < 1`, requires no side metadata, and gives deterministic half-open-style
/// semantics. A coincident boundary can cause at most conservative ambiguity at
/// this coarse level; refinement can resolve it.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Default, Debug)]
#[repr(transparent)]
pub struct BoundaryMerge {
    words: [u64; 8],
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BuildError {
    ZeroTotalWeight,
}

impl fmt::Display for BuildError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::ZeroTotalWeight => f.write_str("all 257 symbol weights are zero"),
        }
    }
}

impl BoundaryMerge {
    pub const BYTES: usize = 64;
    pub const BITS: usize = EVENTS;

    /// Build the 512-bit merged ordering directly from 257 nonnegative integer
    /// weights. No floating point and no division are required.
    ///
    /// Symbol boundary i lies at:
    ///     sum(weights[..=i]) / sum(weights)
    /// for i in 0..256.
    ///
    /// Byte cut j lies at:
    ///     (j + 1) / 256
    /// for j in 0..256, so the final cut is exactly 1.
    pub fn from_weights(weights: &[u64; SYMBOLS]) -> Result<Self, BuildError> {
        let total: u128 = weights.iter().map(|&w| w as u128).sum();
        if total == 0 {
            return Err(BuildError::ZeroTotalWeight);
        }

        let mut out = Self::default();
        let mut symbol_i = 0usize;
        let mut byte_i = 0usize;
        let mut cumulative = weights[0] as u128;

        for event_pos in 0..EVENTS {
            let take_byte = if byte_i == BYTE_CUTS {
                false
            } else if symbol_i == SYMBOL_BOUNDARIES {
                true
            } else {
                // Compare (byte_i + 1) / 256 <= cumulative / total.
                // Equality deliberately chooses the byte cut first (0 before 1).
                let byte_scaled = (byte_i as u128 + 1) * total;
                let symbol_scaled = cumulative * 256u128;
                byte_scaled <= symbol_scaled
            };

            if take_byte {
                byte_i += 1;
            } else {
                out.set_symbol_event(event_pos);
                symbol_i += 1;
                if symbol_i < SYMBOL_BOUNDARIES {
                    cumulative += weights[symbol_i] as u128;
                }
            }
        }

        debug_assert_eq!(symbol_i, SYMBOL_BOUNDARIES);
        debug_assert_eq!(byte_i, BYTE_CUTS);
        debug_assert_eq!(out.count_symbol_events(), SYMBOL_BOUNDARIES as u32);
        Ok(out)
    }

    /// Raw 64-byte-friendly machine representation as eight native words.
    #[inline]
    pub const fn words(&self) -> &[u64; 8] {
        &self.words
    }

    /// True if event `pos` is a symbol boundary; false if it is a byte cut.
    #[inline]
    pub fn is_symbol_event(&self, pos: usize) -> bool {
        assert!(pos < EVENTS);
        ((self.words[pos / 64] >> (pos % 64)) & 1) != 0
    }

    /// Number of symbol-boundary events in `0..end`.
    #[inline]
    pub fn rank_symbols(&self, end: usize) -> u32 {
        assert!(end <= EVENTS);
        let full_words = end / 64;
        let rem = end % 64;

        let mut count = 0u32;
        let mut i = 0usize;
        while i < full_words {
            count += self.words[i].count_ones();
            i += 1;
        }
        if rem != 0 {
            let mask = (1u64 << rem) - 1;
            count += (self.words[full_words] & mask).count_ones();
        }
        count
    }

    /// Number of byte-cut events in `0..end`.
    #[inline]
    pub fn rank_bytes(&self, end: usize) -> u32 {
        end as u32 - self.rank_symbols(end)
    }

    #[inline]
    pub fn count_symbol_events(&self) -> u32 {
        self.words.iter().map(|w| w.count_ones()).sum()
    }

    /// Position of the kth symbol boundary (k in 0..256).
    pub fn select_symbol(&self, mut k: usize) -> usize {
        assert!(k < SYMBOL_BOUNDARIES);
        for (word_i, &word) in self.words.iter().enumerate() {
            let n = word.count_ones() as usize;
            if k < n {
                return word_i * 64 + select_nth_set_bit(word, k);
            }
            k -= n;
        }
        unreachable!("BoundaryMerge invariant violated: missing symbol event")
    }

    /// Position of the kth byte cut (k in 0..256).
    pub fn select_byte(&self, mut k: usize) -> usize {
        assert!(k < BYTE_CUTS);
        for (word_i, &word) in self.words.iter().enumerate() {
            let inv = !word;
            let n = inv.count_ones() as usize;
            if k < n {
                return word_i * 64 + select_nth_set_bit(inv, k);
            }
            k -= n;
        }
        unreachable!("BoundaryMerge invariant violated: missing byte event")
    }

    /// Number of symbol-boundary events associated with radix bucket `bucket`.
    ///
    /// More precisely, this counts symbol events after the previous bucket's
    /// right-edge event and before this bucket's right-edge event. With the
    /// chosen tie rule, a symbol boundary exactly on a bucket edge is placed
    /// immediately after that edge, i.e. on the right-hand side.
    pub fn symbol_boundaries_in_bucket(&self, bucket: u8) -> u16 {
        let bucket = bucket as usize;
        let end = self.select_byte(bucket);
        let start = if bucket == 0 {
            0
        } else {
            self.select_byte(bucket - 1) + 1
        };
        (self.rank_symbols(end) - self.rank_symbols(start)) as u16
    }

    /// Human-readable event stream, useful because the representation is funny.
    /// `S` is a symbol boundary and `B` is a byte cut.
    pub fn event_string(&self) -> String {
        let mut s = String::with_capacity(EVENTS);
        for i in 0..EVENTS {
            s.push(if self.is_symbol_event(i) { 'S' } else { 'B' });
        }
        s
    }

    #[inline]
    fn set_symbol_event(&mut self, pos: usize) {
        self.words[pos / 64] |= 1u64 << (pos % 64);
    }
}

fn select_nth_set_bit(mut word: u64, mut n: usize) -> usize {
    loop {
        let bit = word.trailing_zeros() as usize;
        if n == 0 {
            return bit;
        }
        word &= word - 1;
        n -= 1;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn uniform_257_symbols_has_exact_event_counts() {
        let weights = [1u64; SYMBOLS];
        let p = BoundaryMerge::from_weights(&weights).unwrap();
        assert_eq!(p.count_symbol_events(), 256);
        assert_eq!(p.rank_bytes(512), 256);
        assert_eq!(core::mem::size_of::<BoundaryMerge>(), 64);
    }

    #[test]
    fn rank_select_roundtrip() {
        let mut weights = [1u64; SYMBOLS];
        weights[0] = 1000;
        weights[1] = 0;
        weights[256] = 17;
        let p = BoundaryMerge::from_weights(&weights).unwrap();

        for k in 0..256 {
            let ps = p.select_symbol(k);
            assert!(p.is_symbol_event(ps));
            assert_eq!(p.rank_symbols(ps), k as u32);

            let pb = p.select_byte(k);
            assert!(!p.is_symbol_event(pb));
            assert_eq!(p.rank_bytes(pb), k as u32);
        }
    }

    #[test]
    fn zero_total_rejected() {
        let weights = [0u64; SYMBOLS];
        assert_eq!(
            BoundaryMerge::from_weights(&weights),
            Err(BuildError::ZeroTotalWeight)
        );
    }

    #[test]
    fn a_bucket_can_contain_all_256_symbol_boundaries() {
        // This is why a naive `[u8; 256]` boundary-count table has one ugly
        // overflow case: the count can be 256, not merely 255.
        let mut weights = [0u64; SYMBOLS];
        weights[256] = 1;
        let p = BoundaryMerge::from_weights(&weights).unwrap();
        assert_eq!(p.symbol_boundaries_in_bucket(0), 256);
    }
}
