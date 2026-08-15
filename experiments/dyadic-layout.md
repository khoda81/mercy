# BigDyadic layout and endianness

## Idea

Compare layouts while preserving the conceptual invariant
`bits.len() = fractional_bits + 1`:

```text
A. BitBox<u8, Msb0> (current)
B. BitBox<u8, Lsb0>
C. byte-aligned big-endian numerator
D. byte-aligned little-endian numerator
E. native limb order chosen to minimize BigUint conversion
```

Byte order of the integer and bit order within a byte are separate axes and
must be reported separately.

## Why it might win

The current implementation reconstructs `BigUint` values bit by bit. A layout
closer to native limbs may reduce conversion, copying, and canonicalization
cost in multiplication and scaling.

## Risks / complications

Arithmetic speed may trade against compactness, canonical representation, or
cheap comparison. Adopting result limbs without copying may couple the public
type too closely to one bigint implementation.

## Benchmark target

`dyadic_multiply`, followed by a future `BigDyadic::scale_floor_u64` benchmark.
Also track construction cost from `RankedPrefix::tail_probability` so a faster
multiplication layout does not merely move work upstream.

## Status

Idea

## Results

TBD
