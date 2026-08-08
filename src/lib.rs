//! `mercy`: a tiny proof-of-concept for a boundary-first codec/model ABI.
//!
//! The core object is [`BoundaryMerge`]: exactly 512 bits containing the
//! sorted merge of:
//!
//! - 256 symbol boundaries (1-bits), for 257 ordered symbols
//!   (`0..=255` plus EOS), and
//! - 256 radix-256 bucket right edges (0-bits), including the terminal edge 1.
//!
//! Thus the representation is exactly 64 bytes and has exactly 256 one-bits
//! and 256 zero-bits.

mod partition;
mod reference;

pub use partition::{BoundaryMerge, BuildError};
pub use reference::{compress, decompress, ByteSymbol, ReferenceError, ReferenceModel};

/// A streaming model viewed as a bidirectional constraint transducer.
///
/// Both methods constrain one side of a relation between a symbol stream and a
/// byte stream. Their returned slices are the longest prefixes on the opposite
/// side that became forced by the new constraint.
pub trait ConstraintModel {
    type Symbol: Copy;

    /// Constrain the symbol stream by one known symbol; return bytes that have
    /// become inevitable.
    fn push_symbol(&mut self, symbol: Self::Symbol) -> &[u8];

    /// Constrain the byte stream by one known byte; return symbols that have
    /// become inevitable.
    fn push_byte(&mut self, byte: u8) -> &[Self::Symbol];

    /// Optional fixed-size introspection/composition view.
    fn partition(&self) -> BoundaryMerge;
}
