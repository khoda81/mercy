# mercy (rewrite experiment)

This branch is a clean-room experiment in reducing arithmetic coding to the
smallest probability interface we currently need.

The crate has three public concepts:

- `RankedPrefix`: ordered conditional probabilities stored as `u8 / 256`;
- `BigDyadic`: exact arbitrary-precision dyadic boundaries; and
- `Coder`: a deliberately path-relative `locate` / `zoom` interface.

## Ranked prefixes

For bytes `x[i]`, define

```text
p[i] = x[i] / 256
S(0) = 1
S(i + 1) = S(i) * (1 - p[i])
```

The explicit event at rank `i` has probability `S(i) * p[i]`. A finite prefix
of length `N` has one implicit tail event with probability `S(N)`.

Stored probabilities are in `[0, 1)`, not `(0, 1]`:

- `0` is useful because an explicit event may be impossible;
- `1` is forbidden because it would make all following ranked events dead; and
- certainty is represented structurally by truncating the prefix and selecting
  its implicit tail event.

Every finite tail therefore remains strictly positive by construction.

`RankedPrefix` is a transparent dynamically sized wrapper over `[u8]`.
Borrowing and ownership therefore live in `&RankedPrefix`,
`&mut RankedPrefix`, and `Box<RankedPrefix>`; model construction and resizing
remain ordinary `Vec<u8>` operations. Slice and box conversions are zero-copy,
and range indexing preserves the nominal prefix type for coder calls such as
`coder.zoom(&dist[..i], &dist[i..j])`.

## Why no categorical type?

Every categorical boundary is already the tail event of a prefix. If

```text
S(i) = product_{k < i} (1 - p[k])
```

then any contiguous interval is

```text
P(i <= X < j) = S(i) - S(j).
```

Nothing after `j` can influence that interval. Consequently a coder zoom is
fully described by two prefixes:

```text
denied   = p[..i]
accepted = p[i..j]
```

with interval boundaries

```text
lower = 1 - tail(denied)
upper = 1 - tail(denied) * tail(accepted).
```

## Exact tail computation

For one stored byte `x`,

```text
1 - p = (256 - x) / 256.
```

The implementation removes powers of two from each factor, then multiplies up
to eight remaining odd factors in a `u64`. This is safe because
`255^8 < 2^64`. Chunk products are merged through an online balanced BigUint
multiplication tree.

Performance candidates live in separate modules under
`prefix::implementations`, not in the benchmark harness. The public
`prefix::implementations::compute` function and
`RankedPrefix::tail_probability` use the measured all-workload winner. All
candidates remain publicly addressable, and `PERFORMANCE_CANDIDATES` gives
Criterion one durable registry to enumerate. The scalar implementation is
retained as an exactness reference without forcing its poor large-input scaling
into every timed run.

This structure is intentionally friendly to future SIMD work: the public
semantics are exact and scalar, while the hot small-factor reduction can be
replaced independently.

## BigDyadic

All exact boundaries are dyadic. `BigDyadic` stores the numerator as a fixed
bit string whose length encodes the denominator exponent:

```text
bits = b0 b1 ... bk
value = integer(bits) / 2^k
```

There is no separately stored precision field. The type is intentionally a
single owning bit-slice handle and asserts that its value representation is two
machine words.

Alternate layouts live under `dyadic::implementations`. Native `BigUint`
storage is the measured arithmetic winner and is exported there as `DEFAULT`.
It is not yet the `BigDyadic` representation because doing so would remove the
borrowed `as_bits()` view and the two-word handle invariant; that is a public
API decision rather than an implementation-only optimization.

## Coder contract

`Coder` intentionally guarantees only exact-path determinism.

Changing, merging, splitting, nesting, or otherwise reparameterizing a model
is not promised to preserve coder state or output, even if the transformed
model has mathematically equivalent aggregate probabilities. This prevents
optimized implementations from accidentally acquiring a compositionality
contract they cannot safely keep.

## Benchmarks

```bash
cargo bench --bench tail_probability
cargo bench --bench dyadic_multiply
cargo bench --bench dyadic_layout
```

The Criterion suites measure each crate-owned tail candidate on patterned,
`vec![1; n]`, and model-shaped flat/peaked/long-tail inputs; consuming tail
implementations with allocation inside and outside timing; production dyadic
multiplication; and five alternate layouts across construction, multiplication,
and `scale_floor_u64`. Sizes span 8 through 200,000 entries where applicable.
Optimization hypotheses and results belong in the
[experiment notebook](experiments/README.md).

Named Criterion samples can be combined into a standalone interactive Plotly
report. It shows all runs in empirical latency/throughput distributions and
provides a selectable, model-free pairwise improvement matrix and empirical
cross-pair log-effect distributions.
