# Mercy roadmap

Mercy is still in the proof-of-concept phase. Correctness comes first: the exact
`ReferenceModel` remains the behavioral oracle while optimized implementations
are developed beside it and compared against it.

The current performance target is a single thread on an AMD Ryzen 5 4600H
(Zen 2 / `znver2`). Performance experiments should use
`-C target-cpu=native` on the canonical benchmark machine and should not infer
regressions from GitHub-hosted runner timings.

## Current baseline

Initial Criterion measurements on the Ryzen 5 4600H, before CPU pinning, using
`RUSTFLAGS="-C target-cpu=native"`:

| Operation | Approx. time |
| --- | ---: |
| `BoundaryMerge::from_weights` uniform | 0.99 us |
| `BoundaryMerge::from_weights` skewed | 1.13 us |
| `rank_symbols` | 2.85 ns |
| `rank_bytes` | 2.56 ns |
| `select_symbol` | 12.36 ns |
| `select_byte` | 13.58 ns |
| `symbol_boundaries_in_bucket` | 37.34 ns |
| full scan via repeated `select_symbol` | 3.63 us |
| full scan via repeated `select_byte` | 3.56 us |

These numbers are provisional. Two back-to-back runs showed a few-percent swing
in some select-heavy measurements, which is why the canonical self-hosted runner
is CPU-pinned.

## Phase 1 — trustworthy benchmarks

- [x] Add Criterion.
- [x] Separate 64-byte partition microbenchmarks from the deliberately slow
      arbitrary-precision reference codec.
- [x] Compile benchmark targets on hosted CI without treating hosted timings as
      performance data.
- [x] Add a CPU-pinned self-hosted benchmark workflow for the Ryzen 5 4600H.
- [ ] Bring the isolated Docker benchmark runner online.
- [ ] Record toolchain, CPU topology, governor/frequency state and thermal state
      with benchmark artifacts where practical.
- [ ] Benchmark multiple query ranks instead of repeatedly querying only rank
      127.
- [ ] Add pseudo-random query sequences to exercise realistic branch behavior.
- [ ] Add fixed-rank microbenchmarks for the nth-set-bit primitive at ranks
      0, 1, 4, 8, 16, 31 and 63.

## Phase 2 — 512-bit primitive shootout

Keep the literal implementations as baselines. Add candidates beside them and
let Zen 2 decide.

### Select

- [ ] Baseline: repeated `word &= word - 1` / `BLSR` plus `TZCNT`.
- [ ] BMI2: `PDEP(1 << n, word)` plus `TZCNT`.
- [ ] Small LUT: bytewise select using a ~2 KiB `SELECT8[256][8]` table.
- [ ] Broadword/SWAR select, only if the simpler candidates leave worthwhile
      headroom.
- [ ] Compare isolated nth-bit cost as a function of rank.
- [ ] Compare full `select_symbol` / `select_byte` across ranks
      0, 31, 63, 127, 191 and 255.

### Rank

- [ ] Keep hardware-`POPCNT` baseline.
- [ ] Measure whether prefix metadata is worthwhile for repeated queries.
- [ ] Consider an optional local indexed representation while keeping the public
      `BoundaryMerge` ABI exactly 64 bytes.

A possible local form is:

```rust
struct IndexedBoundaryMerge {
    merge: BoundaryMerge,
    symbol_count_per_word: [u8; 8],
}
```

The wire/public object remains the sacred 64 bytes; auxiliary indices are an
implementation detail.

## Phase 3 — remove avoidable work

### Boundary construction

The current builder performs cross-products on every one of 512 merge events.
Replace repeated multiplication with incrementally maintained scaled positions:

- [ ] Maintain `(byte_i + 1) * total` by adding `total` when crossing a byte cut.
- [ ] Maintain `cumulative * 256` by adding `weight << 8` when crossing a symbol
      boundary.
- [ ] Benchmark uniform, skewed, sparse and pathological distributions.

### Bucket queries

`symbol_boundaries_in_bucket` currently performs two selects and two ranks.
Adjacent byte cuts already delimit a run containing only symbol events.

- [ ] Replace rank-based counting with the distance between adjacent selected
      byte cuts.
- [ ] Try a fused `select_byte_and_preceding_symbol_run` operation that performs
      one select and derives the run length around it.

### Full scans

Repeatedly calling `select(k)` for every `k` intentionally rediscovers the same
64-byte object 256 times.

- [ ] Add one-pass symbol-event iterator.
- [ ] Add one-pass byte-cut iterator.
- [ ] Benchmark one-pass traversal against repeated select.
- [ ] Use the one-pass forms for model composition whenever the whole partition
      is consumed.

This is expected to be one of the largest easy speedups in the current POC.

## Phase 4 — optimized transducer

- [ ] Keep `ReferenceModel` as the exact arbitrary-precision oracle.
- [ ] Add randomized/property roundtrip tests against the oracle.
- [ ] Implement a fixed-width arithmetic/range transducer behind the same
      `push_symbol` / `push_byte` interface.
- [ ] Verify the variable-rate semantics explicitly: one symbol may emit zero
      bytes; one byte may force multiple symbols.
- [ ] Benchmark encode and decode separately, single-threaded.
- [ ] Benchmark end-to-end throughput on representative source distributions.

The entropy coder itself is fundamentally sequential at the stream level. Higher
level compressors may parallelize independent blocks, but the baseline decoder
performance target is one stream on one thread.

## Phase 5 — refinement and model algebra

- [ ] Implement recursive `partition()` refinement.
- [ ] Define the exact refinement protocol around coincident boundaries and the
      current `B before S` tie convention.
- [ ] Benchmark the 64-byte boundary ABI against conventional CDF/range arrays.
- [ ] Implement unweighted product-of-experts composition over exact/refined
      partitions.
- [ ] Explore mixtures and other model algebra after PoE is correct.
- [ ] Measure composition overhead separately from entropy-coder overhead.

## Long-term portability

The first optimized target is Zen 2 because that is the canonical benchmark
machine, not because Mercy should permanently require it.

Eventually:

- [ ] Keep a portable implementation.
- [ ] Add architecture-specific fast paths only where benchmarks justify them.
- [ ] Runtime-dispatch BMI2/other ISA paths if the gain is large enough.
- [ ] Test another x86-64 microarchitecture before treating a Zen 2 result as
      universal.
