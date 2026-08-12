use std::collections::HashMap;
use std::hash::Hash;

const SCALE: u64 = 1u64 << 32;

/// A fixed-point probability in `[0, 1)` with denominator `2^32`.
///
/// A raw value `r` denotes exactly `r / 2^32`. For a branch with two positive
/// children, constructors return a value in `1..=u32::MAX`, so both children
/// retain at least one representable unit of probability.
#[repr(transparent)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Frac32(u32);

impl Frac32 {
    pub const ZERO: Self = Self(0);
    pub const MAX: Self = Self(u32::MAX);

    #[inline]
    pub const fn from_raw(raw: u32) -> Self {
        Self(raw)
    }

    #[inline]
    pub const fn raw(self) -> u32 {
        self.0
    }

    #[inline]
    pub fn as_f64(self) -> f64 {
        self.0 as f64 / SCALE as f64
    }

    /// Quantize `part / whole` while keeping both represented branches alive.
    ///
    /// # Panics
    ///
    /// Panics unless `0 < part < whole`.
    #[inline]
    pub fn from_positive_ratio(part: u128, whole: u128) -> Self {
        assert!(part > 0 && part < whole);
        let raw = ((part << 32) / whole).clamp(1, u32::MAX as u128) as u32;
        Self(raw)
    }

    /// Choose the left branch using one uniformly random `u32`.
    #[inline]
    pub fn choose_left(self, draw: u32) -> bool {
        draw < self.0
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Branch {
    Left,
    Right,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Decision {
    pub left_probability: Frac32,
    pub branch: Branch,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DecodeNode<S> {
    Branch(Frac32),
    Symbol(S),
}

/// Cursor over the binary decisions of a categorical distribution.
pub trait Decoder {
    type Symbol: Copy;

    fn node(&self) -> DecodeNode<Self::Symbol>;
    fn choose(&mut self, branch: Branch);
}

/// A finite categorical distribution exposed as binary conditional decisions.
///
/// This is deliberately more primitive than a CDF. Sampling and arithmetic
/// decoding walk [`Distribution::decoder`]. Encoding an existing symbol walks
/// [`Distribution::encode`]. Neither hot path needs an absolute leaf
/// probability or a normalized list of masses.
pub trait Distribution {
    type Symbol: Copy + Eq;
    type Decoder<'a>: Decoder<Symbol = Self::Symbol>
    where
        Self: 'a;
    type Encoder<'a>: Iterator<Item = Decision>
    where
        Self: 'a;

    fn len(&self) -> usize;
    fn decoder(&self) -> Self::Decoder<'_>;
    fn encode(&self, symbol: Self::Symbol) -> Option<Self::Encoder<'_>>;

    #[inline]
    fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Sample without imposing an RNG dependency on the crate.
    ///
    /// `next_u32` must return independent uniformly distributed `u32`s.
    ///
    /// # Panics
    ///
    /// Panics for an empty distribution.
    fn sample_with(&self, mut next_u32: impl FnMut() -> u32) -> Self::Symbol
    where
        Self: Sized,
    {
        assert!(!self.is_empty(), "cannot sample an empty distribution");
        let mut decoder = self.decoder();
        loop {
            match decoder.node() {
                DecodeNode::Symbol(symbol) => return symbol,
                DecodeNode::Branch(p_left) => {
                    let branch = if p_left.choose_left(next_u32()) {
                        Branch::Left
                    } else {
                        Branch::Right
                    };
                    decoder.choose(branch);
                }
            }
        }
    }

    /// Materialize one absolute leaf probability for diagnostics.
    ///
    /// Codecs should normally consume [`Distribution::encode`] directly and
    /// keep all arithmetic in local conditionals.
    fn probability(&self, symbol: Self::Symbol) -> Option<f64> {
        let mut probability = 1.0;
        for decision in self.encode(symbol)? {
            let left = decision.left_probability.as_f64();
            probability *= match decision.branch {
                Branch::Left => left,
                Branch::Right => 1.0 - left,
            };
        }
        Some(probability)
    }
}

impl<D: Distribution + ?Sized> Distribution for &D {
    type Symbol = D::Symbol;
    type Decoder<'a>
        = D::Decoder<'a>
    where
        Self: 'a;
    type Encoder<'a>
        = D::Encoder<'a>
    where
        Self: 'a;

    #[inline]
    fn len(&self) -> usize {
        (**self).len()
    }

    #[inline]
    fn decoder(&self) -> Self::Decoder<'_> {
        (**self).decoder()
    }

    #[inline]
    fn encode(&self, symbol: Self::Symbol) -> Option<Self::Encoder<'_>> {
        (**self).encode(symbol)
    }
}

/// Stateful autoregressive prediction machine.
///
/// `Prediction<'a>` may be owned or a zero-copy view into the model. `Symbol`
/// is the shared language between encoder and decoder; for large vocabularies
/// it should normally be a cheap ID such as `u32`.
pub trait Model {
    type Symbol: Copy + Eq;
    type Prediction<'a>: Distribution<Symbol = Self::Symbol>
    where
        Self: 'a;

    fn predict(&self) -> Self::Prediction<'_>;
    fn observe(&mut self, symbol: Self::Symbol);
}

mod sealed {
    pub trait Symbols {}
    pub trait Topology {}
}

/// Bidirectional leaf-index <-> symbol mapping.
///
/// This trait is sealed so [`BinaryDistribution`] can rely on the two
/// directions being inverses rather than adding another "trust me" invariant.
pub trait Symbols: sealed::Symbols {
    type Symbol: Copy + Eq;

    fn len(&self) -> usize;
    fn symbol(&self, index: usize) -> Option<Self::Symbol>;
    fn index_of(&self, symbol: Self::Symbol) -> Option<usize>;

    #[inline]
    fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DuplicateSymbol<S> {
    pub symbol: S,
}

/// Compact explicit symbol list. Decode is O(1); encode lookup is O(n).
#[derive(Clone, Debug)]
pub struct LinearSymbols<S> {
    symbols: Vec<S>,
}

impl<S: Copy + Eq> LinearSymbols<S> {
    pub fn new(symbols: Vec<S>) -> Result<Self, DuplicateSymbol<S>> {
        for i in 0..symbols.len() {
            if symbols[..i].contains(&symbols[i]) {
                return Err(DuplicateSymbol { symbol: symbols[i] });
            }
        }
        Ok(Self { symbols })
    }

    pub fn as_slice(&self) -> &[S] {
        &self.symbols
    }
}

impl<S: Copy + Eq> sealed::Symbols for LinearSymbols<S> {}

impl<S: Copy + Eq> Symbols for LinearSymbols<S> {
    type Symbol = S;

    #[inline]
    fn len(&self) -> usize {
        self.symbols.len()
    }

    #[inline]
    fn symbol(&self, index: usize) -> Option<S> {
        self.symbols.get(index).copied()
    }

    #[inline]
    fn index_of(&self, symbol: S) -> Option<usize> {
        self.symbols
            .iter()
            .position(|&candidate| candidate == symbol)
    }
}

/// Explicit symbol list plus a hash index. Decode is O(1); expected encode
/// lookup is O(1), at the cost of the hash table.
#[derive(Clone, Debug)]
pub struct IndexedSymbols<S> {
    symbols: Vec<S>,
    indices: HashMap<S, usize>,
}

impl<S: Copy + Eq + Hash> IndexedSymbols<S> {
    pub fn new(symbols: Vec<S>) -> Result<Self, DuplicateSymbol<S>> {
        let mut indices = HashMap::with_capacity(symbols.len());
        for (index, &symbol) in symbols.iter().enumerate() {
            if indices.insert(symbol, index).is_some() {
                return Err(DuplicateSymbol { symbol });
            }
        }
        Ok(Self { symbols, indices })
    }

    pub fn as_slice(&self) -> &[S] {
        &self.symbols
    }
}

impl<S: Copy + Eq + Hash> sealed::Symbols for IndexedSymbols<S> {}

impl<S: Copy + Eq + Hash> Symbols for IndexedSymbols<S> {
    type Symbol = S;

    #[inline]
    fn len(&self) -> usize {
        self.symbols.len()
    }

    #[inline]
    fn symbol(&self, index: usize) -> Option<S> {
        self.symbols.get(index).copied()
    }

    #[inline]
    fn index_of(&self, symbol: S) -> Option<usize> {
        self.indices.get(&symbol).copied()
    }
}

/// Zero-storage alphabet for dense token IDs `0..len`.
///
/// This is the intended fast path for byte/token models: encode and decode
/// symbol lookup are both O(1), with no vocabulary-sized symbol array or hash
/// table.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DenseU32Symbols {
    len: u32,
}

impl DenseU32Symbols {
    pub const fn new(len: u32) -> Self {
        Self { len }
    }
}

impl sealed::Symbols for DenseU32Symbols {}

impl Symbols for DenseU32Symbols {
    type Symbol = u32;

    #[inline]
    fn len(&self) -> usize {
        self.len as usize
    }

    #[inline]
    fn symbol(&self, index: usize) -> Option<u32> {
        (index < self.len as usize).then_some(index as u32)
    }

    #[inline]
    fn index_of(&self, symbol: u32) -> Option<usize> {
        (symbol < self.len).then_some(symbol as usize)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum BuildError {
    Empty,
    ZeroWeight {
        index: usize,
    },
    LengthMismatch {
        symbols: usize,
        probabilities: usize,
    },
}

/// Decoder over leaf indices rather than semantic symbols.
pub trait IndexDecoder {
    fn node(&self) -> DecodeNode<usize>;
    fn choose(&mut self, branch: Branch);
}

/// Binary probability topology over leaf indices `0..len`.
///
/// Kept separate from [`Symbols`] so probability layout and symbol lookup can
/// be benchmarked and changed independently.
pub trait Topology: sealed::Topology {
    type Decoder<'a>: IndexDecoder
    where
        Self: 'a;
    type Encoder<'a>: Iterator<Item = Decision>
    where
        Self: 'a;

    fn len(&self) -> usize;
    fn decoder(&self) -> Self::Decoder<'_>;
    fn encode_index(&self, index: usize) -> Option<Self::Encoder<'_>>;

    #[inline]
    fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

/// Degenerate binary tree: choose the current leaf or skip to the suffix.
///
/// If leaves are in descending probability order, this is the ranked linear
/// representation discussed in the design exploration. Common symbols can be
/// extremely shallow, while the worst-case path is linear in the vocabulary.
#[derive(Clone, Debug)]
pub struct Chain {
    left_probabilities: Vec<Frac32>,
}

impl Chain {
    pub fn from_weights(weights: &[u64]) -> Result<Self, BuildError> {
        validate_weights(weights)?;

        let mut suffix = vec![0u128; weights.len() + 1];
        for i in (0..weights.len()).rev() {
            suffix[i] = suffix[i + 1] + weights[i] as u128;
        }

        let mut left_probabilities = Vec::with_capacity(weights.len().saturating_sub(1));
        for i in 0..weights.len().saturating_sub(1) {
            left_probabilities.push(Frac32::from_positive_ratio(weights[i] as u128, suffix[i]));
        }

        Ok(Self { left_probabilities })
    }

    pub fn left_probabilities(&self) -> &[Frac32] {
        &self.left_probabilities
    }
}

impl sealed::Topology for Chain {}

pub struct ChainDecoder<'a> {
    chain: &'a Chain,
    rank: usize,
    selected: Option<usize>,
}

impl IndexDecoder for ChainDecoder<'_> {
    #[inline]
    fn node(&self) -> DecodeNode<usize> {
        if let Some(index) = self.selected {
            DecodeNode::Symbol(index)
        } else if self.rank == self.chain.left_probabilities.len() {
            DecodeNode::Symbol(self.rank)
        } else {
            DecodeNode::Branch(self.chain.left_probabilities[self.rank])
        }
    }

    #[inline]
    fn choose(&mut self, branch: Branch) {
        debug_assert!(self.selected.is_none());
        debug_assert!(self.rank < self.chain.left_probabilities.len());
        match branch {
            Branch::Left => self.selected = Some(self.rank),
            Branch::Right => self.rank += 1,
        }
    }
}

pub struct ChainEncoder<'a> {
    chain: &'a Chain,
    target: usize,
    rank: usize,
}

impl Iterator for ChainEncoder<'_> {
    type Item = Decision;

    #[inline]
    fn next(&mut self) -> Option<Self::Item> {
        if self.rank >= self.chain.left_probabilities.len() || self.rank > self.target {
            return None;
        }

        let rank = self.rank;
        self.rank += 1;
        Some(Decision {
            left_probability: self.chain.left_probabilities[rank],
            branch: if rank == self.target {
                Branch::Left
            } else {
                Branch::Right
            },
        })
    }
}

impl Topology for Chain {
    type Decoder<'a> = ChainDecoder<'a>;
    type Encoder<'a> = ChainEncoder<'a>;

    #[inline]
    fn len(&self) -> usize {
        self.left_probabilities.len() + 1
    }

    #[inline]
    fn decoder(&self) -> Self::Decoder<'_> {
        ChainDecoder {
            chain: self,
            rank: 0,
            selected: None,
        }
    }

    #[inline]
    fn encode_index(&self, index: usize) -> Option<Self::Encoder<'_>> {
        (index < self.len()).then_some(ChainEncoder {
            chain: self,
            target: index,
            rank: 0,
        })
    }
}

/// Flat implicit balanced binary tree.
///
/// Only the `n - 1` branch probabilities are stored. Child locations and leaf
/// ranges are derived arithmetically from a preorder layout. Construction
/// visits every leaf and internal node once; encode/decode/sample depth is
/// `floor(log2 n)` or `ceil(log2 n)`.
#[derive(Clone, Debug)]
pub struct BalancedTree {
    len: usize,
    left_probabilities: Vec<Frac32>,
}

impl BalancedTree {
    pub fn from_weights(weights: &[u64]) -> Result<Self, BuildError> {
        validate_weights(weights)?;
        let mut left_probabilities = Vec::with_capacity(weights.len().saturating_sub(1));
        build_balanced(weights, &mut left_probabilities);
        Ok(Self {
            len: weights.len(),
            left_probabilities,
        })
    }

    pub fn left_probabilities(&self) -> &[Frac32] {
        &self.left_probabilities
    }
}

#[inline]
fn split_len(len: usize) -> usize {
    (len + 1) / 2
}

fn build_balanced(weights: &[u64], probabilities: &mut Vec<Frac32>) -> u128 {
    if weights.len() == 1 {
        return weights[0] as u128;
    }

    let branch_index = probabilities.len();
    probabilities.push(Frac32::ZERO);

    let left_len = split_len(weights.len());
    let left = build_balanced(&weights[..left_len], probabilities);
    let right = build_balanced(&weights[left_len..], probabilities);
    probabilities[branch_index] = Frac32::from_positive_ratio(left, left + right);
    left + right
}

impl sealed::Topology for BalancedTree {}

pub struct BalancedDecoder<'a> {
    tree: &'a BalancedTree,
    start: usize,
    len: usize,
    node_index: usize,
}

impl IndexDecoder for BalancedDecoder<'_> {
    #[inline]
    fn node(&self) -> DecodeNode<usize> {
        if self.len == 1 {
            DecodeNode::Symbol(self.start)
        } else {
            DecodeNode::Branch(self.tree.left_probabilities[self.node_index])
        }
    }

    #[inline]
    fn choose(&mut self, branch: Branch) {
        debug_assert!(self.len > 1);
        let left_len = split_len(self.len);
        match branch {
            Branch::Left => {
                self.len = left_len;
                self.node_index += 1;
            }
            Branch::Right => {
                self.start += left_len;
                self.len -= left_len;
                // The left subtree has exactly `left_len - 1` internal nodes,
                // so its preorder span including this branch is `left_len`.
                self.node_index += left_len;
            }
        }
    }
}

pub struct BalancedEncoder<'a> {
    tree: &'a BalancedTree,
    target: usize,
    start: usize,
    len: usize,
    node_index: usize,
}

impl Iterator for BalancedEncoder<'_> {
    type Item = Decision;

    #[inline]
    fn next(&mut self) -> Option<Self::Item> {
        if self.len == 1 {
            return None;
        }

        let left_len = split_len(self.len);
        let split = self.start + left_len;
        let left_probability = self.tree.left_probabilities[self.node_index];

        if self.target < split {
            self.len = left_len;
            self.node_index += 1;
            Some(Decision {
                left_probability,
                branch: Branch::Left,
            })
        } else {
            self.start = split;
            self.len -= left_len;
            self.node_index += left_len;
            Some(Decision {
                left_probability,
                branch: Branch::Right,
            })
        }
    }
}

impl Topology for BalancedTree {
    type Decoder<'a> = BalancedDecoder<'a>;
    type Encoder<'a> = BalancedEncoder<'a>;

    #[inline]
    fn len(&self) -> usize {
        self.len
    }

    #[inline]
    fn decoder(&self) -> Self::Decoder<'_> {
        BalancedDecoder {
            tree: self,
            start: 0,
            len: self.len,
            node_index: 0,
        }
    }

    #[inline]
    fn encode_index(&self, index: usize) -> Option<Self::Encoder<'_>> {
        (index < self.len).then_some(BalancedEncoder {
            tree: self,
            target: index,
            start: 0,
            len: self.len,
            node_index: 0,
        })
    }
}

fn validate_weights(weights: &[u64]) -> Result<(), BuildError> {
    if weights.is_empty() {
        return Err(BuildError::Empty);
    }
    for (index, &weight) in weights.iter().enumerate() {
        if weight == 0 {
            return Err(BuildError::ZeroWeight { index });
        }
    }
    Ok(())
}

/// Compose an invertible symbol representation with a probability topology.
#[derive(Clone, Debug)]
pub struct BinaryDistribution<A, T> {
    symbols: A,
    topology: T,
}

impl<A: Symbols, T: Topology> BinaryDistribution<A, T> {
    pub fn new(symbols: A, topology: T) -> Result<Self, BuildError> {
        if symbols.len() != topology.len() {
            return Err(BuildError::LengthMismatch {
                symbols: symbols.len(),
                probabilities: topology.len(),
            });
        }
        Ok(Self { symbols, topology })
    }

    pub fn symbols(&self) -> &A {
        &self.symbols
    }

    pub fn topology(&self) -> &T {
        &self.topology
    }
}

pub struct SymbolDecoder<'a, A: Symbols, D> {
    symbols: &'a A,
    decoder: D,
}

impl<A, D> Decoder for SymbolDecoder<'_, A, D>
where
    A: Symbols,
    D: IndexDecoder,
{
    type Symbol = A::Symbol;

    #[inline]
    fn node(&self) -> DecodeNode<Self::Symbol> {
        match self.decoder.node() {
            DecodeNode::Branch(probability) => DecodeNode::Branch(probability),
            DecodeNode::Symbol(index) => DecodeNode::Symbol(
                self.symbols
                    .symbol(index)
                    .expect("topology emitted an out-of-range leaf"),
            ),
        }
    }

    #[inline]
    fn choose(&mut self, branch: Branch) {
        self.decoder.choose(branch);
    }
}

impl<A, T> Distribution for BinaryDistribution<A, T>
where
    A: Symbols,
    T: Topology,
{
    type Symbol = A::Symbol;
    type Decoder<'a>
        = SymbolDecoder<'a, A, T::Decoder<'a>>
    where
        Self: 'a;
    type Encoder<'a>
        = T::Encoder<'a>
    where
        Self: 'a;

    #[inline]
    fn len(&self) -> usize {
        self.topology.len()
    }

    #[inline]
    fn decoder(&self) -> Self::Decoder<'_> {
        SymbolDecoder {
            symbols: &self.symbols,
            decoder: self.topology.decoder(),
        }
    }

    #[inline]
    fn encode(&self, symbol: Self::Symbol) -> Option<Self::Encoder<'_>> {
        self.topology.encode_index(self.symbols.index_of(symbol)?)
    }
}

/// Tiny model useful in tests/examples: every state has the same prediction.
#[derive(Clone, Debug)]
pub struct IidModel<D> {
    distribution: D,
}

impl<D> IidModel<D> {
    pub fn new(distribution: D) -> Self {
        Self { distribution }
    }
}

impl<D: Distribution> Model for IidModel<D> {
    type Symbol = D::Symbol;
    type Prediction<'a>
        = &'a D
    where
        D: 'a;

    #[inline]
    fn predict(&self) -> Self::Prediction<'_> {
        &self.distribution
    }

    #[inline]
    fn observe(&mut self, _symbol: Self::Symbol) {}
}

#[cfg(test)]
mod tests {
    use super::*;

    fn decode_path<D: Distribution>(distribution: &D, symbol: D::Symbol) -> D::Symbol
    where
        D::Symbol: std::fmt::Debug,
    {
        let path: Vec<_> = distribution
            .encode(symbol)
            .expect("symbol must be encodable")
            .collect();
        let mut decoder = distribution.decoder();
        for decision in path {
            assert_eq!(
                decoder.node(),
                DecodeNode::Branch(decision.left_probability)
            );
            decoder.choose(decision.branch);
        }
        match decoder.node() {
            DecodeNode::Symbol(decoded) => decoded,
            DecodeNode::Branch(_) => panic!("path ended before a leaf"),
        }
    }

    #[test]
    fn frac32_uses_the_entire_u32_draw_space() {
        assert!(!Frac32::ZERO.choose_left(0));
        assert!(Frac32::MAX.choose_left(u32::MAX - 1));
        assert!(!Frac32::MAX.choose_left(u32::MAX));
    }

    #[test]
    fn explicit_symbol_tables_are_bidirectional() {
        let linear = LinearSymbols::new(vec![7u16, 2, 9]).unwrap();
        let indexed = IndexedSymbols::new(vec![7u16, 2, 9]).unwrap();
        for index in 0..3 {
            let symbol = linear.symbol(index).unwrap();
            assert_eq!(linear.index_of(symbol), Some(index));
            let symbol = indexed.symbol(index).unwrap();
            assert_eq!(indexed.index_of(symbol), Some(index));
        }
        assert!(LinearSymbols::new(vec![1, 2, 1]).is_err());
        assert!(IndexedSymbols::new(vec![1, 2, 1]).is_err());
    }

    #[test]
    fn dense_symbols_need_no_lookup_structure() {
        let symbols = DenseU32Symbols::new(100_000);
        assert_eq!(symbols.symbol(99_999), Some(99_999));
        assert_eq!(symbols.index_of(99_999), Some(99_999));
        assert_eq!(symbols.index_of(100_000), None);
    }

    #[test]
    fn chain_encode_decode_roundtrip() {
        let topology = Chain::from_weights(&[8, 4, 2, 1]).unwrap();
        let distribution = BinaryDistribution::new(DenseU32Symbols::new(4), topology).unwrap();

        for symbol in 0..4 {
            assert_eq!(decode_path(&distribution, symbol), symbol);
        }
        assert_eq!(distribution.encode(0).unwrap().count(), 1);
        assert_eq!(distribution.encode(1).unwrap().count(), 2);
        assert_eq!(distribution.encode(2).unwrap().count(), 3);
        assert_eq!(distribution.encode(3).unwrap().count(), 3);
    }

    #[test]
    fn balanced_tree_roundtrips_non_power_of_two_vocabularies() {
        let weights: Vec<_> = (1..=13).collect();
        let topology = BalancedTree::from_weights(&weights).unwrap();
        assert_eq!(topology.left_probabilities().len(), 12);
        let distribution = BinaryDistribution::new(DenseU32Symbols::new(13), topology).unwrap();

        for symbol in 0..13 {
            assert_eq!(decode_path(&distribution, symbol), symbol);
            let depth = distribution.encode(symbol).unwrap().count();
            assert!((3..=4).contains(&depth));
        }
    }

    fn probability_sum<T: Topology>(distribution: &BinaryDistribution<DenseU32Symbols, T>) -> f64 {
        (0..distribution.len() as u32)
            .map(|symbol| distribution.probability(symbol).unwrap())
            .sum()
    }

    #[test]
    fn represented_leaf_probabilities_sum_to_one() {
        for weights in [&[1, 1, 1, 1, 1][..], &[1000, 3, 2, 1][..]] {
            let chain = BinaryDistribution::new(
                DenseU32Symbols::new(weights.len() as u32),
                Chain::from_weights(weights).unwrap(),
            )
            .unwrap();
            let balanced = BinaryDistribution::new(
                DenseU32Symbols::new(weights.len() as u32),
                BalancedTree::from_weights(weights).unwrap(),
            )
            .unwrap();

            assert!((probability_sum(&chain) - 1.0).abs() < 1e-12);
            assert!((probability_sum(&balanced) - 1.0).abs() < 1e-12);
        }
    }

    #[test]
    fn iid_model_can_borrow_its_prediction() {
        let distribution = BinaryDistribution::new(
            DenseU32Symbols::new(3),
            BalancedTree::from_weights(&[5, 3, 2]).unwrap(),
        )
        .unwrap();
        let mut model = IidModel::new(distribution);
        {
            let prediction = model.predict();
            assert_eq!(prediction.len(), 3);
        }
        model.observe(1);
    }
}
