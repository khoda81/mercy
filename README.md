# mercy

Type-safe probabilistic serialization and compression for Rust.

Mercy treats Serde's data model as shared knowledge between encoder and decoder. A stateful prediction model is written once and used in both directions: serialization teacher-forces known decisions into an entropy coder, while deserialization obtains those same decisions from the coder.

The model owns its history and context:

```rust
pub trait Model<T: ?Sized> {
    type Prediction<'a>: IntoByteCategorical
    where
        Self: 'a;

    fn predict(&self) -> Self::Prediction<'_>;
    fn observe(&mut self, choice: u8);
}
```

Prediction representations are intentionally open. A model may return logits, fixed-point probabilities, products, mixtures, or an application-specific type. The shared wire boundary is deterministic lowering to Mercy's canonical byte distribution:

```rust
pub trait IntoByteCategorical {
    fn byte_categorical(&self) -> ByteCategorical;
}
```

`ByteCategorical` is the exact 256-way distribution consumed by the entropy coder. Its representation is private so the implementation can evolve without making a raw probability, CDF, or frequency layout part of the model ABI.

Serde primitives with fewer than 256 alternatives are handled by exact restriction of the byte distribution. Multi-byte primitives are represented as a stream of byte decisions; after every byte, the model is observed before the next prediction.

## Core invariant

Given the same Serde type, same initial model state, and same arithmetic-coded prefix, decoding deterministically reproduces the same Serde decisions, residual coded state, and updated model state. Encoding is the teacher-forced inverse under the same sequence of canonical byte distributions.

Mercy owns the Serde-level round trip. Custom `Serialize` and `Deserialize` implementations remain responsible for agreeing about what runtime value those Serde decisions mean.

## Current status

This branch is an architectural prototype. The entropy coder is still abstracted behind `ChoiceEncoder` and `ChoiceDecoder`; selecting and integrating a production range/arithmetic coder is the next systems step.

One known Serde asymmetry is enum metadata: serialization exposes the chosen variant index but not the total variant count, while deserialization receives the variant list. Enum discriminants are therefore encoded as a canonical `u32` for now; a future optional schema layer can expose a tighter shared domain.
