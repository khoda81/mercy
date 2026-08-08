# Design notes

## Core interpretation

A model relates two infinite streams:

- an ordered source-symbol stream;
- a uniformly distributed byte stream.

The state represents the continuations still compatible with constraints seen so
far. Constraining either stream may force a prefix on the other.

This avoids requiring the model ABI to expose floating-point probabilities,
integer arithmetic-coder ranges, or a dedicated encoder/decoder model API.

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

These operations are standard succinct-bitvector primitives. On only 512 bits,
a simple eight-word implementation is likely sufficient; SIMD/BMI2 lookup paths
can be benchmarked later rather than assumed.

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
