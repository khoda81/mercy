//! `mercy`: experimental abstractions for probabilistic, typed compression.
//!
//! [`serde_model`] is the current design direction: Serde supplies the shared
//! structural program while one stateful prediction model drives both entropy
//! encoding and entropy decoding.

mod frontier;
mod frontier_reference;
mod partition;
mod reference;
pub mod serde_model;

pub use frontier::{DecodedFrontier, Frontier512};
pub use frontier_reference::{frontier_compress, frontier_decompress, ReferenceFrontierModel};
pub use partition::{BoundaryMerge, BuildError};
pub use reference::{compress, decompress, ByteSymbol, ReferenceError, ReferenceModel};

/// Purified bidirectional prefix transducer.
///
/// The symbol prefix and byte prefix can be advanced independently. Neither
/// push method performs work on the opposite stream. `frontier()` is the only
/// observation surface: it describes how the *next* symbol-child edges and
/// radix-256 byte-child edges interleave under the current pair of prefixes.
pub trait Transducer {
    type Symbol: Copy;

    fn push_symbol(&mut self, symbol: Self::Symbol);
    fn push_byte(&mut self, byte: u8);
    fn frontier(&self) -> Frontier512;
}

/// Earlier convenience API where each push also walks and returns the longest
/// forced prefix on the opposite side.
pub trait ConstraintModel {
    type Symbol: Copy;

    fn push_symbol(&mut self, symbol: Self::Symbol) -> &[u8];
    fn push_byte(&mut self, byte: u8) -> &[Self::Symbol];
    fn partition(&self) -> BoundaryMerge;
}
