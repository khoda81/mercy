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

Implemented under `dyadic::implementations` with a common candidate value and
registry. The durable suite measures prepare-from-current-layout,
multiplication, and `scale_floor_u64` for all five layouts.

## Results

Native `BigUint` storage won every multiplication and scaling size. At 200k,
multiplication fell from about 40.58 ms for MSB bits to 18.04 ms (2.25x), and
scaling fell from about 6.09 ms to 15.13 us (about 402x). Big- and
little-endian bytes were close on multiplication and intermediate on scaling.

Preparing alternatives from an already-built MSB `BigDyadic` necessarily pays
the current bit-by-bit numerator conversion: native preparation cost about
6.07 ms at 200k, while cloning the incumbent took 0.224 ms. A native production
constructor would avoid that conversion, but changing `BigDyadic` would remove
its borrowed `as_bits()` representation and two-word handle invariant. Native
is exported as the experimental `DEFAULT`; production retains MSB bits until
that public API tradeoff is decided explicitly.
