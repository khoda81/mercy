# Benchmarks

Mercy's performance target is currently a single x86-64 machine: a Ryzen 5 4600H.
The codec itself is treated as single-threaded; models layered on top may use
parallelism independently.

## Canonical local run

Use the target CPU's native instruction set:

```bash
RUSTFLAGS="-C target-cpu=native" cargo bench --bench partition
```

The deliberately slow exact reference codec has a separate suite:

```bash
RUSTFLAGS="-C target-cpu=native" cargo bench --bench reference_codec
```

Or run everything:

```bash
RUSTFLAGS="-C target-cpu=native" cargo bench
```

Criterion writes detailed reports under `target/criterion/`.

## What we benchmark

### `partition`

Hot fixed-size machinery:

- construction from uniform and skewed weights;
- `rank_symbols` / `rank_bytes`;
- `select_symbol` / `select_byte`;
- symbol-boundary count for one byte bucket;
- complete 256-entry select scans.

These are the first optimization targets. The current implementation is
intentionally literal; the reference behavior matters more than clever code.
Potential implementations to benchmark later include broadword select, small
lookup tables, BMI2, and SIMD-assisted scans.

### `reference_codec`

End-to-end compression/decompression using the arbitrary-precision exact oracle.
This suite exists to track semantics and provide a baseline shape, not as a
performance target. A future fixed-width arithmetic implementation should be
benchmarked against it for both correctness and speed.

## CI policy

GitHub-hosted runners compile the benchmark harnesses with `cargo bench --no-run`
but their timing results are not treated as meaningful. Shared hosted runners do
not provide the stable machine/thermal environment wanted for regression
tracking.

The intended long-term performance runner is the Ryzen 5 4600H machine itself,
registered as a GitHub self-hosted runner with a dedicated `mercy-bench` label.
A manual workflow can then publish benchmark logs/artifacts from exactly the
hardware we care about; once that runner is reliable, the workflow can be moved
to every push.

## Benchmark discipline

For comparable runs:

- use AC power and a stable performance/power profile;
- avoid running other CPU-heavy jobs;
- keep the same compiler/toolchain when comparing implementation changes;
- compare Criterion distributions rather than a single wall-clock observation;
- do not mix generic builds and `target-cpu=native` baselines.
