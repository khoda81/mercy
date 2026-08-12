//! `mercy`: a tiny proof-of-concept for a boundary-first codec/model ABI.
//!
//! The original hot-path experiment is [`BoundaryMerge`]: exactly 512 bits
//! containing the sorted merge of 256 symbol boundaries and 256 radix-256 byte
//! cuts. [`Frontier512`] is the semantic/reference companion for the purified
//! independently-walkable transducer API; it packs the complete 515-edge local
//! frontier (including current endpoints) into the same 64 bytes by enumerative
//! coding.

mod frontier;
mod frontier_reference;
pub mod model;
mod partition;
mod reference;

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
///
/// A normal entropy-coding driver simply walks the opposite side whenever the
/// frontier says one child has become forced, but that synchronization policy is
/// intentionally outside the transducer itself.
pub trait Transducer {
    type Symbol: Copy;

    fn push_symbol(&mut self, symbol: Self::Symbol);
    fn push_byte(&mut self, byte: u8);
    fn frontier(&self) -> Frontier512;
}

/// Earlier convenience API where each push also walks and returns the longest
/// forced prefix on the opposite side.
///
/// Kept while the new purified [`Transducer`] semantics are proven against the
/// existing exact reference coder. New composition work should target
/// `Transducer` rather than adding more behavior here.
pub trait ConstraintModel {
    type Symbol: Copy;

    fn push_symbol(&mut self, symbol: Self::Symbol) -> &[u8];
    fn push_byte(&mut self, byte: u8) -> &[Self::Symbol];
    fn partition(&self) -> BoundaryMerge;
}
