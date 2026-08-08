# Design notes

## Core interpretation

A model relates two infinite streams:

- an ordered source-symbol stream;
- a uniformly distributed byte stream.

The state represents the continuations still compatible with constraints seen so
far. Constraining either stream may force a prefix on the other.

This avoids requiring the model ABI to expose floating-point probabilities,
integer arithmetic-coder ranges, or a dedicated encoder/decoder model API.

## Constraint semantics

The two stream operations are dual:

```text
push_symbol(symbol) -> longest newly-forced byte prefix
push_byte(byte)      -> longest newly-forced symbol prefix
```

Neither direction has a fixed rate. A common symbol may force no bytes yet; a
single byte may force several symbols. This variable-rate behavior is the point:
it lets information cross symbol/byte boundaries instead of rounding every
symbol to an integral number of transport digits.

The exact reference implementation makes this literal with two rational
intervals. Let `S` be the code points compatible with known source symbols and
`B` the code points compatible with known stream bytes.

- `push_symbol` narrows `S`, then emits radix-256 children while `S` lies wholly
  inside one child of `B`.
- `push_byte` narrows `B`, then emits source symbols while `B` lies wholly inside
  one symbol child of `S`.

In ordinary encoding the useful invariant is `S subset B`; in ordinary decoding
it is `B subset S`. The same state machinery supports both constraint directions.

EOS leaves a finite nonempty final symbol interval. Finalization chooses its
midpoint and emits radix-256 digits until the selected byte cylinder is wholly
inside that interval. The fast implementation does not need to use midpoints or
big rationals; this is just the reference semantics.

## Why the merged boundary bitvector works

At a single radix-256 view, there are two sorted boundary sequences:

1. 256 boundaries among 257 ordered source symbols;
2. 256 right edges of radix-256 buckets, including the endpoint 1.

A merge of two sorted sequences needs only one bit per event to say which list
the next item came from. Rank determines the event identity.

Thus the model's coarse partition is a 512-bit fixed-Hamming-weight bitvector.

This is also the unary/stars-and-bars encoding of the number of symbol boundaries
encountered from one byte cut to the next.

## Operations

For bit position `p`:

- `rank1(p)` = number of symbol boundaries before p;
- `rank0(p)` = number of byte cuts before p.

For event ordinal `k`:

- `select1(k)` = merged position of symbol boundary k;
- `select0(k)` = merged position of byte cut k.

These operations are standard succinct-bitvector primitives. The current code is
intentionally literal and therefore a useful compiler target to inspect. The hot
path will need dedicated benchmarking: broadword select, small lookup tables,
BMI2 (`pdep`/`pext` where useful), SIMD summaries, or a tiny auxiliary index may
all beat the current per-set-bit loop depending on the target CPU.

The representation itself should stay fixed while implementations are free to
optimize how they query it.

## Exact ties

The representation does not explicitly distinguish equality from an infinitesimal
ordering difference. The POC resolves exact ties as `B < S`.

This may make one coarse bucket conservatively appear to contain a boundary that
is actually on an edge. Refinement removes the ambiguity. If benchmarks ever show
pathological repeated refinement, equality metadata can be reconsidered, but it
should not be paid for preemptively.

## Prior art boundary

Known pieces:

- arithmetic/range coding uses cumulative boundaries;
- rank/select bitvectors are standard succinct structures;
- unary stars-and-bars representations encode dense monotone integer sequences;
- finite-state arithmetic coders exist;
- composable entropy-coding libraries separate model and coder abstractions.

What this POC is exploring is the *combination*:

- fixed 64-byte merge bitvector as the model's public radix-256 boundary view;
- recursive refinement instead of exposing numeric ranges;
- symmetric stream-constraint operations;
- algebra/composition directly over this interface.

No novelty claim should be made without a proper literature/patent search.
