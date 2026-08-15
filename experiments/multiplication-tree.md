# Widening multiplication tree

## Idea

Reduce `factor = 256 - raw` values through pairwise widening after removing
powers of two:

```text
u8-ish factors -> u16 -> u32 -> u64 -> u128 -> BigUint / BigDyadic
```

The bounds align with the widening levels:

```text
255^2 < 2^16, 255^4 < 2^32, 255^8 < 2^64, 255^16 < 2^128
```

Each complete level can occupy 16 bytes: `16 x u8`, `8 x u16`, `4 x u32`,
`2 x u64`, or `1 x u128`.

## Why it might win

It offers a balanced dependency graph, predictable working-set size,
SIMD-friendly pairwise operations, and fewer bigint operands.

## Risks / complications

Widening and shuffling may cost more than the current eight-factor `u64`
chunks. Partial groups and transfer into `BigUint` need careful handling, and
architecture-specific SIMD must not leak into the public semantics.

## Benchmark target

`tail_probability` first; use `dyadic_multiply` to ensure any representation
changes do not shift cost into the next public operation.

## Status

Implemented as `prefix::implementations::widening_u128` and included in the
full candidate registry.

## Results

On the 2026-08-16 run, widening won four of seven patterned sizes, including
50k at about 4.30 ms and 200k at about 29.32 ms. It won only one of the other
19 workload/size cells and was materially slower on sparse model-shaped hazard
arrays. Online-balanced therefore remains the public default across the whole
matrix.
