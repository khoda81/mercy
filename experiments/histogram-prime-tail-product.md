# Histogram and prime-exponent tail product

## Observation

For a ranked hazard prefix with raw bytes `x_i`, the exact implicit-tail
probability is

```text
tail = product_i (1 - x_i / 256)
     = product_i (256 - x_i) / 256^n.
```

Integer multiplication is commutative, so byte order is irrelevant to this
boundary computation. The selected prefix can be treated as a multiset without
changing `RankedPrefix` semantics elsewhere.

For each integer factor `f = 256 - x`, split

```text
f = 2^v2(f) * o_f
```

where `o_f` is odd. The removed powers of two reduce the denominator exponent;
the final numerator is therefore odd and already canonical.

## Candidate A: histogram powers

Build a 256-bin byte histogram, convert each occupied bin to its reduced odd
factor, and accumulate counts for equal odd factors. Compute each repeated
factor with exponentiation by squaring:

```text
product_(odd f) f^count[f]
```

Then combine the nontrivial `BigUint` powers with a balanced product tree.

## Candidate B: histogram prime exponents

Precompute the prime factorization of every odd integer from 1 through 255.
Use the histogram to accumulate one global exponent for each of the 53 odd
primes through 251, compute each prime power with exponentiation by squaring,
and combine at most 53 nontrivial `BigUint` values with a balanced product tree:

```text
product_(odd prime p <= 251) p^E[p]
```

## Why it may win

Inputs contain only 256 distinct bytes, and realistic quantized hazards can
repeat heavily. Histogramming makes work proportional to the small factor
alphabet plus bigint exponentiation rather than multiplying every repeated
factor individually. Prime accumulation may reduce the final tree to far fewer
and better-balanced operands on large prefixes.

## Expected costs and risks

Tiny prefixes still pay for zeroing and scanning fixed-size histograms, so both
candidates should lose before a workload-dependent crossover. Prime
factorization accounting adds another fixed pass. The exponent and precision
accumulators must use checked arithmetic, and all results must remain bit-for-bit
identical to the scalar exact reference.

The candidates may use a crate-private constructor for a known nonzero odd
numerator to avoid redundant trailing-zero canonicalization. This must not
change the public API or weaken `BigDyadic` invariants.

## Benchmark protocol

Benchmark both candidates against every existing performance candidate on the
durable sizes

```text
8, 16, 64, 256, 4,096, 50,000, 200,000
```

using the patterned, `vec![1; n]`, and model-shaped flat, peaked, and long-tail
fixtures. Record raw Criterion samples and compare the empirical probability of
superiority and cross-pair log-improvement distributions in the Plotly
dashboard. Do not promote a candidate or hard-code a crossover threshold from
one run.

## Status

Implemented and measured on 2026-08-16. The separate crate modules are
`prefix::implementations::histogram_powers` and
`prefix::implementations::histogram_primes`; both remain in the durable
performance-candidate registry. Production selection is unchanged.

## Results

The measured crossover depends on factor repetition, not length alone:

- On patterned input, histogram-powers was slower through 4,096 entries, then
  won at 50k and 200k. Against the fastest existing candidate, its empirical
  probability of improvement was 77.7% at 50k with median log improvement
  `+0.0350` (about 1.036x), and 80.0% at 200k with `+0.0395` (about 1.040x).
- On `vec![1; n]`, histogram-powers first narrowly won at 256 entries
  (`P(improvement) = 65.2%`, median log improvement `+0.0327`). It was decisive
  from 4,096 onward: 100% empirical superiority in this run, with median
  speedups of about 1.24x at 4,096, 1.44x at 50k, and 1.59x at 200k.
- Histogram-primes crossed the fastest existing candidate only on repeated-one
  input from 4,096 onward. Its median speedup reached about 1.28x at 200k, but
  histogram-powers was faster at every repeated-one crossover size.
- On all twelve model-shaped size/fixture cells, the histogram candidates lost
  every cross-pair comparison against the fastest existing candidate. Those
  inputs contain many zero hazards, which existing candidates reduce to
  identities without paying the histogram candidates' fixed scan and power
  setup cost.

There is therefore no justified universal threshold. Both candidates remain
available for comparison, while `online-balanced` remains the production
default. Raw samples are recorded as `histogram-candidates-20260816-a`, and the
dashboard exposes their probability-of-superiority matrices and empirical log
improvement distributions.

## Assembly-guided follow-up

Optimized x86-64 assembly and `cargo asm --mca` on the Zen 2 benchmark host
exposed three candidate-internal costs:

- returning `[usize; 256]` by value caused 2 KiB histogram copies and contributed
  to a roughly 6 KiB combined stack footprint;
- prime accumulation visited all 53 primes for every occupied odd factor even
  though an odd integer through 255 has at most three distinct prime factors;
- the balanced product tree allocated a new `Vec` at every level.

The retained implementation now gives `reduced_histogram` caller-owned storage
and iterates the raw histogram by reference, avoiding the by-value array copies.
The dense `[256][53]` factorization table is replaced with at most three sparse
`(prime index, exponent)` entries per odd factor. The sparse table retains the
same checked exponent arithmetic and has an exhaustive reconstruction test for
every odd factor through 255.

An in-place product-tree reduction was also implemented and measured. It helped
some tiny patterned cases, where neither histogram candidate is competitive,
but made the `cargo asm --mca` static block larger (`57.0` versus `48.5` cycles)
and the close-in Criterion ablation favored the original allocating tree by
about 2.6% for both candidates at the relevant 50k patterned crossover. Results
at 200k were mixed. The in-place tree was therefore rejected and the allocating
balanced tree retained.

The final durable snapshot is `histogram-sparse-no-copy-20260816-a`. Compared
nonparametrically with the original `histogram-candidates-20260816-a` snapshot:

- histogram-powers improved in 22 of 26 workload/size cells, with a median
  per-cell speedup of about 1.048x;
- histogram-primes improved in 23 of 26 cells, with a median per-cell speedup of
  about 1.050x;
- at 4,096 model-shaped entries, the median speedup over the original candidate
  was about 1.189x for histogram-powers and 1.284x for histogram-primes;
- on patterned input the retained prime candidate improved by about 1.50x to
  1.69x through size 256, while its 50k change was about 1.021x and its 200k
  result regressed about 1.7%;
- on `vec![1; n]`, histogram-powers remains the strongest histogram candidate:
  against `online-balanced`, its median speedups were about 1.15x at 256, 1.26x
  at 4,096, 1.39x at 50k, and 1.57x at 200k in the final run.

The structural changes do not alter the production decision. Both histogram
candidates still lost essentially every model-shaped cross-pair against
`online-balanced` (`P(improvement) <= 1%`), and patterned results do not support
a stable crossover. `online-balanced` remains the default.
