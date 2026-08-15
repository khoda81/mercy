# Real-model probability fixtures

## Idea

Capture fixed ranked conditional-probability arrays from actual models after
quantizing hazards to the crate's `u8 / 256` representation.

Candidate categories include small vocabularies, roughly 50k-, 100k-, and
200k-token language-model vocabularies, plus peaked, flat, and long-tail
distributions.

## Why it might win

Stable real inputs reveal byte patterns, precision growth, and operand shapes
that a deterministic synthetic generator may miss. They also make performance
claims easier to relate to production workloads.

## Risks / complications

Fixtures can be large, model-specific, license-sensitive, or accidentally
contain identifying source data. Quantization and ranking procedures must be
recorded so results remain reproducible.

## Benchmark target

Run exact tail computation and dyadic multiplication over representative fixed
arrays. Add coder-operation targets only after a concrete coder exists.

## Status

Idea

## Results

TBD
