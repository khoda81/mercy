//! `mercy`: type-safe probabilistic serialization and compression.
//!
//! Serde supplies the shared structural program. A stateful model predicts the
//! next canonical byte distribution, and the same model drives both entropy
//! encoding and entropy decoding.

pub mod serde_model;

pub use serde_model::{
    decode, encode, ByteCategorical, ChoiceDecoder, ChoiceEncoder, IntoByteCategorical, Model,
};
