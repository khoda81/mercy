# mercy

A tiny Rust proof-of-concept for treating an entropy model as a relation between
an ordered **symbol stream** and a uniform radix-256 **byte stream**, without
making probabilities/CDF ranges the public model ABI.

The name comes from the fact that the 256 gods finally showed *mercy beaucoup*.

## The 64-byte object

For byte-oriented source data with EOS there are 257 ordered source symbols:

- `0..=255`
- `EOS`

Therefore there are exactly **256 symbol boundaries**.

At one radix-256 refinement level, use **256 byte-bucket right edges**, including
the terminal edge at probability coordinate `1`.

Merge those two already-sorted event streams:

- `S` = next symbol boundary
- `B` = next byte-bucket right edge

Only the event *kind* has to be stored because identities are implicit from
rank. This produces exactly:

```text
256 S events + 256 B events = 512 bits = 64 bytes
```

A bitvector is the obvious machine representation:

```text
1 = S
0 = B
```

It has fixed Hamming weight 256. If all fixed-weight 512-bit strings were
possible and equally likely, the enumerative information would be
`log2(C(512, 256)) ~= 507.17 bits`, so a raw 512-bit representation is already
very close to the combinatorial lower bound while being dramatically easier to
compute on.

This representation is mathematically a stars-and-bars / unary representation
of a weak composition, or equivalently a succinct bitvector representation of a
monotone sequence. The proposed use as a fixed model/codec partition ABI is the
interesting part, not the combinatorial encoding itself.

## Tie convention

Probability cells are conceptually half-open. For an exact coincidence between a
byte cut and a symbol boundary, this POC orders:

```text
B before S
```

Reasons:

1. It is deterministic.
2. It is literally the natural bit sort order (`0 < 1`).
3. Building the merge uses one integer `<=` comparison and no tie side-channel.
4. Exact coincidences may conservatively create a coarse ambiguity, but a
   refinement step can remove it. The public representation stays 64 bytes.

The convention can be benchmarked against `S before B` later; nothing fundamental
depends on it.

## No floats required

`BoundaryMerge::from_weights` accepts 257 `u64` weights and compares a symbol
boundary `cum / total` with a byte cut `(j+1)/256` using only cross-multiplied
`u128` integers:

```text
cum * 256  ?  (j + 1) * total
```

## Streaming model interface

The deeper model abstraction is a bidirectional constraint transducer:

```rust
trait ConstraintModel {
    type Symbol: Copy;

    fn push_symbol(&mut self, symbol: Self::Symbol) -> &[u8];
    fn push_byte(&mut self, byte: u8) -> &[Self::Symbol];
    fn partition(&self) -> BoundaryMerge;
}
```

`push_symbol` discards continuations whose symbol stream does not match and
returns whatever byte prefix has become forced.

`push_byte` does the symmetric operation and returns whatever symbol prefix has
become forced.

Compression repeatedly calls `push_symbol` and concatenates returned bytes.
Decompression repeatedly calls `push_byte` and concatenates returned symbols.

The 64-byte `partition()` view is optional introspection intended for model
algebra (PoE, mixtures, visualization, etc.).

## Exact reference codec

`ReferenceModel` is deliberately the least clever implementation possible. It
keeps two arbitrary-precision rational intervals:

- the set of code points compatible with the symbol constraints;
- the set compatible with the byte constraints.

Pushing a symbol narrows the first interval. Every radix-256 child that becomes
inevitable is emitted as a byte. Pushing a byte narrows the second interval and
symmetrically emits every source symbol that becomes inevitable.

After EOS, `try_finish()` follows the midpoint of the final symbol interval until
the chosen byte cylinder is entirely inside it. This gives a finite canonical
code without changing the streaming interface.

The helpers below exercise the complete path:

```rust
let weights = [1u64; 257];
let encoded = mercy::compress(weights, b"hello mercy")?;
let decoded = mercy::decompress(weights, &encoded)?;
assert_eq!(decoded, b"hello mercy");
```

This implementation is a **correctness oracle**, not a performance target.
Big integers, linear scans over 256/257 children, and the current literal
rank/select implementation are all intentionally obvious.

## Roadmap

1. keep the exact reference transducer as the behavioral oracle;
2. add randomized/property roundtrip tests and compare future implementations
   against the oracle;
3. implement recursive `partition()` refinement;
4. implement a fixed-width arithmetic/range transducer behind the same API;
5. optimize the 512-bit hot paths (`rank`/`select`, boundary construction,
   child lookup), benchmarking broadword tricks, lookup tables, BMI2 and SIMD
   rather than assuming which wins;
6. benchmark the raw 64-byte bitvector ABI against conventional CDF/range arrays;
7. implement generic product-of-experts composition at partition/refinement
   level.
