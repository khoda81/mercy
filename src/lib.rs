//! Minimal primitives for experimenting with ranked arithmetic coders.
//!
//! The crate deliberately exposes only three mathematical ideas:
//!
//! - [`RankedPrefix`], an ordered sequence of conditional event probabilities;
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
//! The representation is intentionally narrow for now: probabilities are
//! always `u8 / 256`. Precision can be increased structurally by refining an
//! accepted interval and rescaling the coder again, rather than by widening
//! every stored probability.

mod coder;
mod dyadic;
pub mod prefix;

pub use coder::Coder;
pub use dyadic::BigDyadic;
pub use prefix::RankedPrefix;
