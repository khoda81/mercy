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

The reproducible fixture pipeline is implemented under `prefix::fixtures` for
4k, 50k, 100k, and 200k flat, peaked, and long-tail weight distributions. It
converts ranked integer weights into quantized conditional hazards with a
documented deterministic rule.

## Results

These model-shaped proxies exposed a regime absent from the mixed-byte input:
many hazards quantize to zero, so online/batch candidates process 200k entries
in roughly 0.20 ms rather than tens of milliseconds. Online won seven of twelve
model-shaped cells and batch won five; widening won none.

No actual model output was available in the repository, and the older remote
model experiment benchmarks topology APIs rather than storing probability
captures. The committed fixtures are therefore labeled model-shaped synthetic
proxies, not real-model evidence. Capturing licensed, provenance-recorded model
outputs remains a data-acquisition task rather than an unimplemented benchmark
path.
