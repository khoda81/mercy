# Design notes

This branch intentionally starts with almost no surface area. The goal is to
make the mathematical representation difficult to misuse and leave arithmetic
coder implementations free to compete on speed and compression efficiency.

## 1. The only probability model is `RankedPrefix`

A raw byte `x` represents

```text
p = x / 256
```

with `p in [0, 1)`. At rank `i`, `p[i]` is the probability of accepting that
rank conditioned on every earlier rank having been denied.

Define survival/tail mass

```text
S(0) = 1
S(i + 1) = S(i) * (1 - p[i]).
```

Then

```text
P(X = i) = S(i) * p[i]
P(X >= i) = S(i).
```

A finite prefix has one implicit final event with probability `S(N)`. There is
no separate categorical type because this implicit event is already the
categorical remainder.

### Probability one is structural

An explicit probability of one would make all later ranks unreachable. The
byte representation therefore excludes it. To express certainty, truncate the
prefix: its implicit tail is selected with probability one relative to the
truncated model.

This gives two useful construction-time invariants:

1. every finite prefix has strictly positive tail mass; and
2. no stored event can accidentally invalidate later structure.

Zero remains representable because impossible explicit events are harmless.

## 2. Prefixes are CDF boundaries

Every anchored boundary is a tail probability. For two boundaries `i <= j`,

```text
P(i <= X < j) = S(i) - S(j).
```

After the first `i` events have been denied, the same interval can be described
without indices as

```text
denied   = p[..i]
accepted = p[i..j].
```

Let

```text
D = tail(denied)
A = tail(accepted).
```

The selected global CDF interval is

```text
[1 - D, 1 - D*A)
```

and has mass `D * (1 - A)`. The probability model after `j` is provably
irrelevant, so it is not part of the coder interface.

## 3. Exact boundaries are dyadic

For one byte

```text
1 - p = (256 - x) / 256.
```

Products of these values are dyadic fractions. `BigDyadic` therefore stores
only a bit string

```text
b0 b1 ... bk
```

with value

```text
integer(bits) / 2^k.
```

The allocation length carries the denominator exponent; there is no exponent
field in the value. Numerator powers of two are cancelled so the representation
is canonical.

`BigDyadic` deliberately uses a fixed-size owning bit-slice handle rather than
`BigUint` as its public representation. `BigUint` is an implementation tool for
multiplication, not part of the mathematical state we want to expose.

## 4. Tail multiplication strategy

The hot operation is

```text
product_i ((256 - x[i]) / 256).
```

For each factor, powers of two are removed immediately. This has three effects:

- `x = 0` becomes the identity exactly and costs no precision bits;
- every remaining numerator factor is odd and at most 255; and
- eight factors fit in one `u64` because `255^8 < 2^64`.

Current reduction shape:

```text
u8 probabilities
    -> small odd factors + precision-bit count
    -> products of up to 8 factors in u64
    -> online balanced BigUint multiplication tree
    -> BigDyadic
```

Each candidate owns that full computation in a separate module under
`prefix::implementations`. The production entry point directly re-exports the
measured all-workload winner, while a crate-owned registry exposes every serious
performance candidate to Criterion. The benchmark harness therefore contains
input and timing logic only; it does not become a second home for production
arithmetic.

The first two stages are the main SIMD target. Callers do not choose a kernel
through `RankedPrefix`, so scalar, autovectorized, portable-SIMD, and
architecture-specific implementations can be benchmarked without changing its
semantics. Individual modules remain public for focused measurements and
correctness checks.

## 5. Coder semantics are path-relative

`Coder` has two operations:

- `locate(prefix)` asks which explicit/tail event contains the current coded
  value under that exact finite prefix;
- `zoom(denied, accepted)` conditions the coder on the contiguous interval
  described by those exact prefixes.

The determinism contract applies only to exact replay of the same initial state
and the same sequence of byte-identical calls.

There is intentionally no promise that equivalent probability constructions
produce equivalent coder states. In particular, callers must not depend on
nested zooms being collapsible, split/merged events being interchangeable, or
an encoded interval being recoverable by "un-zooming".

This weak contract is important: it permits integer rounding, delayed output,
different representative choices, and future coder-specific optimizations
without silently breaking a stronger algebraic expectation.

## 6. Current benchmark targets

The first benchmark target is `RankedPrefix::tail_probability`. Criterion uses
patterned, repeated-one, and model-shaped flat/peaked/long-tail workloads at
prefix lengths through 200k and applies every entry in
`prefix::implementations::PERFORMANCE_CANDIDATES` to each identical input. The
scalar reference is excluded from the full matrix because its sequential
large-integer growth is not a viable performance candidate. A separate owned
matrix accounts for input allocation inside and outside the timed region.

The second target is exact `BigDyadic` multiplication, using equal-size values
derived from deterministic prefix tails. A layout matrix additionally compares
MSB/LSB bit storage, big-/little-endian bytes, and native `BigUint` storage for
conversion, multiplication, and `scale_floor_u64`.

Speculative implementation work and its results belong in the `experiments/`
lab notebook. Durable implementation comparisons should enumerate crate code
through the shared registry rather than copying candidate logic into `benches/`.

Arithmetic coder implementations should be added only after the exact prefix
boundary primitive and its benchmarks are stable. They can then be compared
against the same model representation without changing the model API.
