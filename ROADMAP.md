# Roadmap

Mercy is being rebuilt around type-safe probabilistic Serde serialization.

Near-term work:

1. Finalize the `Model<T>` and prediction-lowering API.
2. Define the exact private semantics and efficient representation of `ByteCategorical`.
3. Integrate a production arithmetic/range coder behind `ChoiceEncoder` and `ChoiceDecoder`.
4. Add ergonomic prediction representations such as logits and exact fixed-point distributions.
5. Add exact model algebra such as mixtures and products over compatible predictions.
6. Investigate an optional schema layer for Serde metadata that is not exposed symmetrically, especially enum cardinality.
7. Build typed MDL composition where a decoded model can itself model the following value.
8. Add token-model adapters that expose exact byte-level predictions without forcing applications to manage token-prefix machinery.

Old boundary-merge/frontier/transducer experiments are intentionally not part of this branch.
