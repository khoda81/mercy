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

## Durable benchmark targets

The committed Criterion suites measure operations exposed by the abstraction:

```bash
cargo bench --bench tail_probability
cargo bench --bench dyadic_multiply
```

Implementation-specific diagnostics may be useful inside a short-lived
experiment, but should not proliferate in the durable suite. A likely next
public-operation benchmark is `BigDyadic::scale_floor_u64`, which bridges exact
boundaries into a finite arithmetic-coder range.

Every result should record the revision, toolchain, machine, benchmark command,
and enough repetitions to distinguish a consistent effect from Criterion
noise.

After running both suites, render the consolidated plot gallery with:

```bash
python3 experiments/plot_results.py
```

This dependency-free renderer writes SVG plots, raw CSV values, Markdown, and a
browsable HTML report under `artifacts/benchmark-report/`. Criterion's own
detailed per-case plots remain available under `target/criterion/report/`.
