# cargo-critdash

A live, empirical comparison dashboard for Criterion.rs benchmarks.

The main design rule is that **benchmark harnesses do not know about the dashboard**. `cargo-critdash` wraps `cargo criterion --message-format=json`, consumes cargo-criterion's machine-readable `benchmark-complete` stream, persists each run, and pushes updates to the browser as benchmarks finish.

## Why this boundary

Criterion's `target/criterion/**/sample.json` files are private implementation details. `cargo-criterion --message-format=json` is the supported machine-readable interface and includes the raw iteration counts and measured values needed to reconstruct per-iteration batch estimates.

The dashboard treats those values as Criterion **batch estimates**, not i.i.d. single-call timings.

## Install

For now, from this directory:

```sh
cargo install --path .
cargo install cargo-criterion
```

Then, in any Rust project whose benches already use Criterion:

```sh
cargo critdash
```

No benchmark-source changes are required.

Useful options:

```sh
cargo critdash --label baseline
cargo critdash --label candidate -- --bench tail_probability
cargo critdash --serve-only
cargo critdash --exit-after-run
```

By default the UI is served at `http://127.0.0.1:8787` and run history is stored under `target/critdash/runs/`.

## Zero-config benchmark naming

To build comparison matrices without project-specific parsing, the first version recognizes Criterion IDs following:

```text
family[/fixture]/candidate/scale
```

For example:

```text
tail/flat/online-balanced/4096
tail/flat/batch-balanced/4096
```

become two candidates in the same `tail/flat · 4096` comparison.

IDs which do not match the convention are still stored and displayed; they may simply form singleton groups. A small declarative schema/config file is the intended next step for projects with different naming conventions.

## Statistics

For two series with per-iteration Criterion batch estimates `A` (reference) and `B` (candidate), critdash reports:

```text
P(B < A)
```

by comparing every cross-pair and assigning half credit to exact ties.

It also plots the empirical effect distribution:

```text
log(A / B)
```

so positive values mean the candidate is faster.

The `n*m` cross-pairs are **not treated as independent samples**. They are an empirical effect distribution derived from the original Criterion measurements.

Global summaries are deliberately secondary:

- **Mean empirical superiority**: Borda-like average pairwise probability.
- **Bradley-Terry / Elo**: descriptive one-dimensional compression of the empirical matrix.
- **BT RMS residual**: how badly that one-dimensional model misses the empirical pairwise probabilities.

## Current limitations / next steps

1. Plotly is loaded from a CDN; bundle it for fully offline operation.
2. Add `.critdash.toml` for arbitrary benchmark-id dimensions and lower/higher-is-better measurements.
3. Add experimental-block identity and interleaved/randomized benchmark scheduling support.
4. Add bootstrap uncertainty over the original measurement units (never over the cross-pairs as if independent).
5. Record a richer environment fingerprint and refuse to pool different machines by default.
6. Add golden analytics tests and browser smoke tests.
