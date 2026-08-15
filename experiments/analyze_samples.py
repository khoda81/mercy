#!/usr/bin/env python3
"""Snapshot every current Criterion sampled batch for the Plotly report."""

from __future__ import annotations

import argparse
import csv
import json
import math
import platform
import re
import subprocess
from dataclasses import dataclass
from datetime import datetime, timezone
from pathlib import Path

GROUP_LABELS = {
    "tail/patterned": "Tail probability / patterned",
    "tail/ones": "Tail probability / vec![1; n]",
    "tail/model-flat": "Tail probability / model-shaped flat",
    "tail/model-peaked": "Tail probability / model-shaped peaked",
    "tail/model-long-tail": "Tail probability / model-shaped long-tail",
    "tail-owned/outside/patterned": "Owned tail / allocation outside",
    "tail-owned/inside/patterned": "Owned tail / allocation inside",
    "dyadic_multiply/equal": "Production dyadic multiply",
    "dyadic-layout/multiply": "Dyadic layout / multiply",
    "dyadic-layout/scale-floor-u64": "Dyadic layout / scale floor u64",
    "dyadic-layout/construct": "Dyadic layout / construct",
}


@dataclass(frozen=True)
class Sample:
    run: str
    operation: str
    size: int
    sample_index: int
    iterations: float
    total_time_ns: float
    latency_ns: float
    throughput_per_second: float
    throughput_unit: str


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--criterion-root", type=Path, default=Path("target/criterion"))
    parser.add_argument(
        "--output-dir", type=Path, default=Path("artifacts/benchmark-report")
    )
    parser.add_argument("--record", required=True, metavar="RUN")
    args = parser.parse_args()
    if not re.fullmatch(r"[A-Za-z0-9._-]+", args.record):
        parser.error(f"invalid run label: {args.record!r}")
    return args


def command_output(command: list[str]) -> str:
    try:
        return subprocess.run(
            command, check=True, capture_output=True, text=True
        ).stdout.strip()
    except (OSError, subprocess.CalledProcessError):
        return "unavailable"


def current_samples(root: Path, run: str) -> list[Sample]:
    samples: list[Sample] = []
    for benchmark_path in sorted(root.glob("**/new/benchmark.json")):
        benchmark = json.loads(benchmark_path.read_text())
        group_id = benchmark.get("group_id")
        if group_id not in GROUP_LABELS:
            continue
        try:
            size = int(benchmark["value_str"])
        except (KeyError, TypeError, ValueError) as error:
            raise ValueError(
                f"non-numeric benchmark size in {benchmark_path}"
            ) from error
        if size <= 0:
            # Historical diagnostic groups used zero-byte sentinels. They do
            # not have meaningful throughput and are outside the durable matrix.
            continue
        implementation = benchmark.get("function_id")
        if group_id != "dyadic_multiply/equal" and not implementation:
            # Ignore stale pre-registry diagnostics that reused a durable group
            # name without a candidate function id.
            continue
        operation = GROUP_LABELS[group_id]
        if implementation:
            operation = f"{operation} ({implementation})"
        throughput = benchmark.get("throughput") or {}
        if "Bytes" in throughput:
            unit = "bytes/s"
        elif "Elements" in throughput:
            unit = "entries/s"
        else:
            raise ValueError(f"missing throughput metadata in {benchmark_path}")

        path = benchmark_path.with_name("sample.json")
        if not path.exists():
            raise FileNotFoundError(f"missing {path}; run the Criterion suite first")
        payload = json.loads(path.read_text())
        iterations = payload["iters"]
        times = payload["times"]
        if len(iterations) != len(times):
            raise ValueError(f"mismatched iteration/time arrays in {path}")
        for index, (count, total_ns) in enumerate(zip(iterations, times)):
            latency_ns = float(total_ns) / float(count)
            measured_throughput = size * 1e9 / latency_ns
            samples.append(
                Sample(
                    run,
                    operation,
                    size,
                    index,
                    float(count),
                    float(total_ns),
                    latency_ns,
                    measured_throughput,
                    unit,
                )
            )
    if not samples:
        raise FileNotFoundError(f"no recognized Criterion samples under {root}")
    return samples


def write_snapshot(
    output: Path, criterion_root: Path, run: str, samples: list[Sample]
) -> Path:
    runs = output / "samples"
    runs.mkdir(parents=True, exist_ok=True)
    path = runs / f"{run}.csv"
    with path.open("w", newline="") as handle:
        writer = csv.writer(handle)
        writer.writerow(
            (
                "run",
                "operation",
                "size",
                "sample_index",
                "iterations",
                "total_time_ns",
                "latency_ns",
                "throughput_per_second",
                "throughput_unit",
                "log10_latency_ns",
                "log10_throughput",
            )
        )
        for item in samples:
            writer.writerow(
                (
                    item.run,
                    item.operation,
                    item.size,
                    item.sample_index,
                    f"{item.iterations:.0f}",
                    f"{item.total_time_ns:.6f}",
                    f"{item.latency_ns:.12f}",
                    f"{item.throughput_per_second:.12f}",
                    item.throughput_unit,
                    f"{math.log10(item.latency_ns):.12f}",
                    f"{math.log10(item.throughput_per_second):.12f}",
                )
            )
    metadata = {
        "run": run,
        "recorded_at": datetime.now(timezone.utc).isoformat(),
        "revision": command_output(
            ["jj", "log", "-r", "@", "--no-graph", "-T", "commit_id.short()"]
        ),
        "change_id": command_output(
            ["jj", "log", "-r", "@", "--no-graph", "-T", "change_id.short()"]
        ),
        "rustc": command_output(["rustc", "--version"]),
        "platform": platform.platform(),
        "sample_count": len(samples),
        "criterion_source": str(criterion_root),
        "semantics": "Each row is one Criterion sampled batch; latency_ns = total_time_ns / iterations.",
    }
    (runs / f"{run}.json").write_text(json.dumps(metadata, indent=2) + "\n")
    return path


def cross_pair_log_improvements(
    reference: list[float], candidate: list[float]
) -> list[float]:
    """Return log(reference/candidate) for every empirical cross-pair."""
    return [
        math.log(reference_value / candidate_value)
        for reference_value in reference
        for candidate_value in candidate
    ]


def probability_of_superiority(reference: list[float], candidate: list[float]) -> float:
    """Return P(candidate latency < reference latency), with half-weight ties."""
    wins = ties = 0
    for candidate_value in candidate:
        for reference_value in reference:
            if candidate_value < reference_value:
                wins += 1
            elif candidate_value == reference_value:
                ties += 1
    total = len(reference) * len(candidate)
    return (wins + ties * 0.5) / total


def main() -> None:
    args = parse_args()
    samples = current_samples(args.criterion_root, args.record)
    path = write_snapshot(args.output_dir, args.criterion_root, args.record, samples)
    print(f"recorded {len(samples)} Criterion samples in {path}")


if __name__ == "__main__":
    main()
