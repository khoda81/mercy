# Size-adaptive tail candidate

## Goal

Test whether a size-only dispatcher can combine the strongest exact tail
implementations into a production default. The policy must not inspect byte
values or fixture identity, and promotion requires the merged candidate to beat
every existing candidate at every durable measured scale.

## Policy derivation

Aggregate the empirical pairwise wins from
`histogram-sparse-no-copy-20260816-a` across every available fixture at each
size, then fit the same descriptive Bradley-Terry scores used by the dashboard.
The measured winners were:

```text
8                    batch-balanced
16, 64               online-balanced
256                   widening-u128
4,096, 50,000         online-balanced
100,000               batch-balanced
200,000               online-balanced
```

Power-of-two transition bands preserve those choices without pretending the
single measured sizes establish exact crossover points:

```text
0..=8                 batch-balanced
9..=64                online-balanced
65..=256              widening-u128
257..=65,535          online-balanced
65,536..=131,071      batch-balanced
131,072 and larger    online-balanced
```

Histogram candidates are not selected because their repeated-factor wins are
outweighed by model-shaped regressions at the same sizes.

## Promotion rule

Keep `online-balanced` as production while benchmarking `size-adaptive` in the
full durable matrix. Promote only if the merged candidate ranks above every
individual candidate at every measured size under the dashboard's empirical
probability and Bradley-Terry/Elo comparisons. Inspect optimized dispatch
assembly before promotion so benchmark gains are not hiding avoidable dispatch
overhead.

## Results

The full durable tail matrix was recorded as
`size-adaptive-20260816-a`: 20 Criterion sample batches for each of six
candidates across the patterned, `vec![1; n]`, and three model-shaped fixtures.
The same nonparametric cross-pair win counts as the dashboard were aggregated
across every fixture available at each size, with one symmetric half-win
pseudocount per candidate pair before fitting Bradley-Terry strengths and
converting them to zero-centered Elo.

| Entries | Fixtures | Aggregate winner (Elo) | `size-adaptive` Elo | Rank | P(adaptive faster than winner) |
| ------: | -------: | ---------------------- | ------------------: | ---: | ------------------------------: |
| 8 | 2 | `batch-balanced` (+920.2) | +829.9 | 2 | 34.2% |
| 16 | 2 | `online-balanced` (+624.6) | +523.0 | 3 | 35.9% |
| 64 | 2 | `online-balanced` (+335.6) | +200.0 | 3 | 35.6% |
| 256 | 2 | `online-balanced` (+146.7) | +131.5 | 2 | 45.8% |
| 4,096 | 5 | `online-balanced` (+195.2) | +192.6 | 2 | 46.7% |
| 50,000 | 5 | `online-balanced` (+142.1) | +113.0 | 2 | 43.2% |
| 100,000 | 3 | `batch-balanced` (+1026.2) | +885.2 | 3 | 28.7% |
| 200,000 | 5 | `online-balanced` (+147.8) | +96.0 | 2 | 42.2% |

Across all 26 matching fixture/size cells, `online-balanced` also leads the
aggregate fit at +176.8 Elo; `size-adaptive` is second at +146.6 Elo and wins
45.28% of its empirical cross-pairs against `online-balanced`.

The absolute Elo magnitudes are descriptive and depend on the candidate pool;
the rank and head-to-head probability are the promotion evidence. The adaptive
candidate did not lead at any measured scale, so it fails the all-scale gate.
`online-balanced` remains the production default and `size-adaptive` remains a
public benchmark candidate.

Release assembly from `cargo asm --lib --asm --intel --rust --simplify` is a
five-comparison decision tree whose leaves are tail jumps to the chosen
implementation. There is no prefix copy, allocation, indirect candidate call,
or extra return frame. `cargo asm --lib --mca --intel --simplify` reports 4.5
cycles block reciprocal throughput for the complete synthetic instruction
listing; an executed path evaluates only the comparisons leading to its tail
jump.

Because the dispatcher and its selected direct implementation ran in separate
sequential benchmark blocks, their near-equal comparisons remain sensitive to
machine drift and order effects. A future promotion attempt should interleave
the direct and adaptive calls within each fixture/size benchmark before using a
small apparent dispatcher win as evidence. This run provides no reason to
override the strict promotion rule.
