//! Minimal primitives for experimenting with ranked arithmetic coders.
//!
//! The crate deliberately exposes four mathematical ideas:
//!
//! - [`RankedPrefix`], an ordered sequence of conditional event probabilities;
//! - [`JeffreysU8`], a one-byte Bernoulli quantizer uniform in the
//!   Fisher-Rao / Jeffreys coordinate;
//! - [`BigDyadic`], an arbitrary-precision dyadic fraction used for exact
//!   probability boundaries; and
//! - [`Coder`], the path-relative interface implemented by arithmetic coders.
//!
//! A `RankedPrefix` is not a normalized categorical table. Each stored byte is
//! the probability that the event at that position occurs *given that all
//! previous events were denied*. Exhausting the prefix denotes one final,
//! implicit tail event. This makes every prefix boundary exact and turns the
//! tail probability into a product of `(1 - p)` factors.
//!
//! `RankedPrefix` still uses raw `u8 / 256` probabilities. `JeffreysU8` is kept
//! separate while its wire semantics and coder integration are explored.

mod coder;
mod dyadic;
mod jeffreys;
mod prefix;

pub use coder::Coder;
pub use dyadic::BigDyadic;
pub use jeffreys::JeffreysU8;
pub use prefix::RankedPrefix;
