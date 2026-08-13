//! `mercy`: type-safe probabilistic serialization and compression.
//!
//! Serde supplies the shared structural program. A stateful model predicts a
//! representation that deterministically lowers to the next exact byte
//! categorical, and the same model drives both entropy encoding and decoding.

pub mod coder;
pub mod serde_model;

pub use coder::{FractionalU16, RangeDecoder, RangeEncoder};
pub use serde_model::{
    decode, encode, ByteCategorical, ChoiceDecoder, ChoiceEncoder, IntoByteCategorical, Model,
};
