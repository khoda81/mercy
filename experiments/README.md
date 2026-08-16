# Mercy experiment notebook

This directory is a lab notebook for benchmarkable implementation ideas. An
entry records a hypothesis and its risks; it is not an architectural decision
or permission to complicate the production implementation. Update an entry's
status and results only after testing it against the durable public-operation
benchmarks.

## Experiments

- [Multiplication tree](multiplication-tree.md): widen small odd factors through
  a balanced, SIMD-friendly reduction tree.
- [In-place tail product](in-place-tail-product.md): consume owned probability
  storage as scratch space for progressive widening.
- [Dyadic layout](dyadic-layout.md): compare bit and byte layouts by conversion
  and arithmetic cost.
- [Real-model fixtures](real-model-fixtures.md): replace or augment synthetic
  inputs with stable, representative model outputs.
- [Histogram and prime tail product](histogram-prime-tail-product.md): exploit
  commutativity and repeated byte factors through histograms and prime powers.
- [Size-adaptive tail candidate](size-adaptive-tail.md): compose measured
  per-size winners and require an all-scale win before production promotion.

## Durable benchmark targets

The committed Criterion suites measure operations exposed by the abstraction:

```bash
cargo bench --bench tail_probability
cargo bench --bench dyadic_multiply
cargo bench --bench dyadic_layout
```

Tail implementations live in separate crate modules and are enumerated through
`prefix::implementations::PERFORMANCE_CANDIDATES`; the benchmark owns only deterministic
input construction and timing. The simple scalar module remains directly
callable for exactness checks and focused diagnostics but is excluded from the
large matrix because its sequential `BigUint` growth is intentionally not a
performance candidate.

Implementation-specific diagnostics may be useful inside a short-lived
experiment, but should not proliferate in the durable suite. A likely next
public-operation benchmark is `BigDyadic::scale_floor_u64`, which bridges exact
boundaries into a finite arithmetic-coder range.

Every result should record the revision, toolchain, machine, benchmark command,
and enough repetitions to distinguish a consistent effect from Criterion
noise.

Criterion retains all sampled batches as iteration counts and total times in
`target/criterion/<benchmark>/<candidate>/<size>/new/sample.json` for the tail
matrix and `target/criterion/<benchmark>/<size>/new/sample.json` elsewhere.
Since later runs replace those directories, snapshot a named run before
continuing. The recorder includes every registered tail candidate:

```bash
python3 experiments/analyze_samples.py --record baseline-name
```

After recording each run, regenerate the standalone interactive report:

```bash
uv run experiments/plot_results.py
```

The Plotly report embeds its JavaScript and every recorded sample in
`artifacts/benchmark-report/index.html`, so the result remains usable offline.
The pairwise panel comes first, with selectors for operation, input size, and a
reference run. It shows a full candidate-by-reference matrix of the empirical
probability that candidate latency is lower, counting ties as one half. A toggle
switches the same matrix to signed candidate-minus-reference Elo differences
derived from a Bradley-Terry fit; both modes are ordered by Bradley-Terry
strength. Below that, every other candidate is compared with the selected
reference using all cross-pair effects `log(reference / candidate)`, so negative
values are regressions and positive values are improvements. The WebGL ECDF
keeps every effect point but omits markers for responsive interaction. The
legend and summary table report probability of improvement,
Bradley-Terry/Elo scores, median log improvement, and median multiplicative
speedup.

The all-run section then plots median latency, median throughput, and
Bradley-Terry Elo against entry count for every recorded run. Entry count and
the two performance values use logarithmic axes; the latency axis is reversed
so faster results are higher. Elo is relative to the candidates available for a
given operation and scale, so use it to inspect rank and crossover behavior
rather than as an absolute performance measurement across different pools.

The `n×m` cross-pairs are an empirical effect-size distribution, not `n×m`
independent observations. The report fits no parametric distribution or KDE,
uses no Q-Q area score, and makes no independence-based uncertainty claim. The
Bradley-Terry fit is a descriptive ordering only, with no inferential claim.
