# In-place tail product

## Idea

Let a consuming operation such as
`into_tail_probability(self: Box<RankedPrefix>)` reuse owned probability bytes
as scratch storage while progressively reducing:

```text
[u8; N] -> [u16; ceil(N/2)] -> [u32] -> [u64] -> [u128]
```

## Why it might win

The input allocation already exists, so destructive reduction could avoid
temporary arrays, allocations, and copies.

## Risks / complications

`Box<[u8]>` guarantees only `u8` alignment. It must not be reinterpreted as a
normally aligned wider slice. Candidate approaches include unaligned
loads/stores, byte-level writes, stronger initial alignment, or a dedicated
owned container. A consuming API also has real public-surface cost.

## Benchmark target

Compare owned-input tail computation with `tail_probability` across the same
patterned sizes, including allocation accounting outside and inside the timed
region as separate experiments.

## Status

Idea

## Results

TBD
