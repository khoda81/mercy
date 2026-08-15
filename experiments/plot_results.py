#!/usr/bin/env python3
"""Render a dependency-free SVG report from Mercy's Criterion results."""

from __future__ import annotations

import argparse
import csv
import html
import json
import math
import platform
import subprocess
from dataclasses import dataclass
from datetime import datetime, timezone
from pathlib import Path
from xml.sax.saxutils import escape


SIZES = (8, 16, 64, 256, 4_096, 50_000, 200_000)
BACKGROUND = "#090d18"
PANEL = "#111827"
FOREGROUND = "#e5e7eb"
MUTED = "#94a3b8"
GRID = "#334155"
TAIL = "#22d3ee"
MULTIPLY = "#f472b6"
ACCENT = "#a3e635"
WARNING = "#fbbf24"


@dataclass(frozen=True)
class Measurement:
    operation: str
    size: int
    median_ns: float
    lower_ns: float
    upper_ns: float

    @property
    def relative_ci_width(self) -> float:
        return (self.upper_ns - self.lower_ns) / self.median_ns


@dataclass(frozen=True)
class Series:
    label: str
    color: str
    xs: list[float]
    ys: list[float]
    lower: list[float] | None = None
    upper: list[float] | None = None


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--criterion-root", type=Path, default=Path("target/criterion"))
    parser.add_argument("--output-dir", type=Path, default=Path("artifacts/benchmark-report"))
    return parser.parse_args()


def load_measurement(root: Path, operation: str, group: str, size: int) -> Measurement:
    path = root / group / str(size) / "new" / "estimates.json"
    if not path.exists():
        raise FileNotFoundError(f"missing {path}; run both Criterion suites first")
    median = json.loads(path.read_text())["median"]
    confidence = median["confidence_interval"]
    return Measurement(
        operation,
        size,
        float(median["point_estimate"]),
        float(confidence["lower_bound"]),
        float(confidence["upper_bound"]),
    )


def load_results(root: Path) -> dict[str, list[Measurement]]:
    groups = {
        "Tail probability": "tail_patterned",
        "Dyadic multiply": "dyadic_multiply_equal",
    }
    return {
        operation: [load_measurement(root, operation, group, size) for size in SIZES]
        for operation, group in groups.items()
    }


def transformed(value: float, logarithmic: bool) -> float:
    return math.log10(value) if logarithmic else value


def human(value: float, unit: str = "") -> str:
    def concise(number: float) -> str:
        if abs(number) >= 100:
            return f"{number:.0f}"
        if abs(number) >= 10:
            return f"{number:.1f}"
        return f"{number:.2g}"

    if unit == "ns":
        if abs(value) >= 1_000_000:
            return f"{concise(value / 1_000_000)}ms"
        if abs(value) >= 1_000:
            return f"{concise(value / 1_000)}us"
        return f"{concise(value)}ns"

    absolute = abs(value)
    for threshold, suffix in ((1e9, "G"), (1e6, "M"), (1e3, "k")):
        if absolute >= threshold:
            scaled = value / threshold
            return f"{concise(scaled)}{suffix}{unit}"
    if absolute >= 10:
        return f"{value:.0f}{unit}"
    if absolute >= 1:
        return f"{value:.2g}{unit}"
    return f"{value:.2f}{unit}"


def line_plot(
    output: Path,
    title: str,
    xlabel: str,
    ylabel: str,
    series: list[Series],
    *,
    x_log: bool = True,
    y_log: bool = False,
    horizontal: float | None = None,
    y_unit: str = "",
) -> None:
    width, height = 1100, 680
    left, right, top, bottom = 105, 35, 75, 90
    plot_width = width - left - right
    plot_height = height - top - bottom
    all_x = [x for item in series for x in item.xs]
    all_y = [y for item in series for y in item.ys]
    if horizontal is not None:
        all_y.append(horizontal)
    x_values = [transformed(value, x_log) for value in all_x]
    y_values = [transformed(value, y_log) for value in all_y]
    x_min, x_max = min(x_values), max(x_values)
    y_min, y_max = min(y_values), max(y_values)
    y_padding = (y_max - y_min or 1.0) * 0.08
    y_min -= y_padding
    y_max += y_padding

    def sx(value: float) -> float:
        point = transformed(value, x_log)
        return left + (point - x_min) / (x_max - x_min or 1.0) * plot_width

    def sy(value: float) -> float:
        point = transformed(value, y_log)
        return top + (y_max - point) / (y_max - y_min or 1.0) * plot_height

    parts = [
        f'<svg xmlns="http://www.w3.org/2000/svg" width="{width}" height="{height}" viewBox="0 0 {width} {height}">',
        f'<rect width="100%" height="100%" fill="{BACKGROUND}"/>',
        f'<rect x="{left}" y="{top}" width="{plot_width}" height="{plot_height}" rx="8" fill="{PANEL}"/>',
        f'<text x="{width / 2}" y="38" fill="{FOREGROUND}" font-family="system-ui" font-size="24" font-weight="700" text-anchor="middle">{escape(title)}</text>',
    ]

    x_ticks = sorted(set(all_x))
    if len(x_ticks) > 8:
        x_ticks = x_ticks[:: max(1, len(x_ticks) // 7)]
    for value in x_ticks:
        x = sx(value)
        parts.append(f'<line x1="{x:.2f}" y1="{top}" x2="{x:.2f}" y2="{top + plot_height}" stroke="{GRID}" stroke-dasharray="3 6"/>')
        parts.append(f'<text x="{x:.2f}" y="{top + plot_height + 27}" fill="{MUTED}" font-family="system-ui" font-size="13" text-anchor="middle">{human(value)}</text>')

    for tick in range(6):
        position = y_min + (y_max - y_min) * tick / 5
        value = 10**position if y_log else position
        y = top + plot_height - plot_height * tick / 5
        parts.append(f'<line x1="{left}" y1="{y:.2f}" x2="{left + plot_width}" y2="{y:.2f}" stroke="{GRID}" stroke-dasharray="3 6"/>')
        parts.append(f'<text x="{left - 14}" y="{y + 5:.2f}" fill="{MUTED}" font-family="system-ui" font-size="13" text-anchor="end">{human(value, y_unit)}</text>')

    if horizontal is not None:
        y = sy(horizontal)
        parts.append(f'<line x1="{left}" y1="{y:.2f}" x2="{left + plot_width}" y2="{y:.2f}" stroke="{FOREGROUND}" stroke-width="1.5" stroke-dasharray="8 6"/>')

    for item in series:
        if item.lower is not None and item.upper is not None:
            for x_value, low, high in zip(item.xs, item.lower, item.upper):
                x, y_low, y_high = sx(x_value), sy(low), sy(high)
                parts.append(f'<line x1="{x:.2f}" y1="{y_low:.2f}" x2="{x:.2f}" y2="{y_high:.2f}" stroke="{item.color}" stroke-width="2"/>')
                parts.append(f'<line x1="{x - 5:.2f}" y1="{y_low:.2f}" x2="{x + 5:.2f}" y2="{y_low:.2f}" stroke="{item.color}"/>')
                parts.append(f'<line x1="{x - 5:.2f}" y1="{y_high:.2f}" x2="{x + 5:.2f}" y2="{y_high:.2f}" stroke="{item.color}"/>')
        points = " ".join(f"{sx(x):.2f},{sy(y):.2f}" for x, y in zip(item.xs, item.ys))
        parts.append(f'<polyline points="{points}" fill="none" stroke="{item.color}" stroke-width="3" stroke-linejoin="round"/>')
        for x_value, y_value in zip(item.xs, item.ys):
            parts.append(f'<circle cx="{sx(x_value):.2f}" cy="{sy(y_value):.2f}" r="5" fill="{item.color}" stroke="{BACKGROUND}" stroke-width="2"/>')

    legend_x = left + 18
    for index, item in enumerate(series):
        y = top + 25 + index * 25
        parts.append(f'<line x1="{legend_x}" y1="{y}" x2="{legend_x + 28}" y2="{y}" stroke="{item.color}" stroke-width="4"/>')
        parts.append(f'<text x="{legend_x + 38}" y="{y + 5}" fill="{FOREGROUND}" font-family="system-ui" font-size="14">{escape(item.label)}</text>')

    parts.extend(
        (
            f'<text x="{left + plot_width / 2}" y="{height - 25}" fill="{FOREGROUND}" font-family="system-ui" font-size="16" text-anchor="middle">{escape(xlabel)}</text>',
            f'<text x="24" y="{top + plot_height / 2}" fill="{FOREGROUND}" font-family="system-ui" font-size="16" text-anchor="middle" transform="rotate(-90 24 {top + plot_height / 2})">{escape(ylabel)}</text>',
            "</svg>",
        )
    )
    output.write_text("\n".join(parts))


def bar_plot(output: Path, title: str, labels: list[str], first: Series, second: Series) -> None:
    width, height = 1100, 680
    left, right, top, bottom = 105, 35, 75, 90
    plot_width = width - left - right
    plot_height = height - top - bottom
    maximum = max(first.ys + second.ys) * 1.12
    group_width = plot_width / len(labels)
    bar_width = group_width * 0.32
    parts = [
        f'<svg xmlns="http://www.w3.org/2000/svg" width="{width}" height="{height}" viewBox="0 0 {width} {height}">',
        f'<rect width="100%" height="100%" fill="{BACKGROUND}"/>',
        f'<rect x="{left}" y="{top}" width="{plot_width}" height="{plot_height}" rx="8" fill="{PANEL}"/>',
        f'<text x="{width / 2}" y="38" fill="{FOREGROUND}" font-family="system-ui" font-size="24" font-weight="700" text-anchor="middle">{escape(title)}</text>',
    ]
    for tick in range(6):
        value = maximum * tick / 5
        y = top + plot_height - plot_height * tick / 5
        parts.append(f'<line x1="{left}" y1="{y:.2f}" x2="{left + plot_width}" y2="{y:.2f}" stroke="{GRID}" stroke-dasharray="3 6"/>')
        parts.append(f'<text x="{left - 14}" y="{y + 5:.2f}" fill="{MUTED}" font-family="system-ui" font-size="13" text-anchor="end">{human(value, "ns")}</text>')
    for index, label in enumerate(labels):
        center = left + group_width * (index + 0.5)
        for offset, value, color in ((-bar_width, first.ys[index], first.color), (0, second.ys[index], second.color)):
            bar_height = value / maximum * plot_height
            parts.append(f'<rect x="{center + offset:.2f}" y="{top + plot_height - bar_height:.2f}" width="{bar_width:.2f}" height="{bar_height:.2f}" fill="{color}" rx="3"/>')
        parts.append(f'<text x="{center:.2f}" y="{top + plot_height + 27}" fill="{MUTED}" font-family="system-ui" font-size="13" text-anchor="middle">{escape(label)}</text>')
    for index, item in enumerate((first, second)):
        x = left + 18 + index * 190
        parts.append(f'<rect x="{x}" y="{top + 12}" width="18" height="18" fill="{item.color}"/>')
        parts.append(f'<text x="{x + 28}" y="{top + 27}" fill="{FOREGROUND}" font-family="system-ui" font-size="14">{escape(item.label)}</text>')
    parts.extend(
        (
            f'<text x="{left + plot_width / 2}" y="{height - 25}" fill="{FOREGROUND}" font-family="system-ui" font-size="16" text-anchor="middle">Model entries per operand</text>',
            f'<text x="24" y="{top + plot_height / 2}" fill="{FOREGROUND}" font-family="system-ui" font-size="16" text-anchor="middle" transform="rotate(-90 24 {top + plot_height / 2})">Median time</text>',
            "</svg>",
        )
    )
    output.write_text("\n".join(parts))


def local_slopes(values: list[Measurement]) -> tuple[list[float], list[float]]:
    xs, slopes = [], []
    for left, right in zip(values, values[1:]):
        xs.append(math.sqrt(left.size * right.size))
        slopes.append(math.log(right.median_ns / left.median_ns) / math.log(right.size / left.size))
    return xs, slopes


def render_plots(results: dict[str, list[Measurement]], output: Path) -> list[tuple[str, str]]:
    tail = results["Tail probability"]
    multiply = results["Dyadic multiply"]
    tail_series = Series("Tail probability", TAIL, list(SIZES), [m.median_ns for m in tail], [m.lower_ns for m in tail], [m.upper_ns for m in tail])
    multiply_series = Series("Dyadic multiply", MULTIPLY, list(SIZES), [m.median_ns for m in multiply], [m.lower_ns for m in multiply], [m.upper_ns for m in multiply])
    plots: list[tuple[str, str]] = []

    def add(name: str, title: str, *args: object, **kwargs: object) -> None:
        line_plot(output / f"{name}.svg", title, *args, **kwargs)
        plots.append((name, title))

    add("01-latency-loglog", "Public-operation latency", "Model entries per operand", "Median time", [tail_series, multiply_series], y_log=True, y_unit="ns")
    bar_plot(output / "02-small-model-latency.svg", "Small-model latency", [str(size) for size in SIZES[:4]], Series("Tail probability", TAIL, [], [m.median_ns for m in tail[:4]]), Series("Dyadic multiply", MULTIPLY, [], [m.median_ns for m in multiply[:4]]))
    plots.append(("02-small-model-latency", "Small-model latency"))
    add("03-tail-latency", "Exact tail latency", "Prefix entries", "Median time", [tail_series], y_log=True, y_unit="ns")
    add("04-tail-throughput", "Exact tail throughput", "Prefix entries", "Input throughput", [Series("Tail probability", TAIL, list(SIZES), [m.size / (m.median_ns * 1e-9) / (1024**2) for m in tail])], y_unit=" MiB/s")
    add("05-tail-cost-per-entry", "Tail cost per probability byte", "Prefix entries", "Median ns / entry", [Series("Tail probability", ACCENT, list(SIZES), [m.median_ns / m.size for m in tail])], y_log=True, y_unit="ns")
    add("06-multiply-latency", "Exact dyadic multiplication latency", "Source entries per operand", "Median time", [multiply_series], y_log=True, y_unit="ns")
    add("07-multiply-rate", "Exact dyadic multiply rate", "Source entries per operand", "Operations / second", [Series("Dyadic multiply", MULTIPLY, list(SIZES), [1e9 / m.median_ns for m in multiply])], y_log=True)
    add("08-multiply-cost-per-entry", "Multiply cost normalized by source model", "Source entries per operand", "Median ns / entry", [Series("Dyadic multiply", WARNING, list(SIZES), [m.median_ns / m.size for m in multiply])], y_log=True, y_unit="ns")
    ratios = [right.median_ns / left.median_ns for left, right in zip(tail, multiply)]
    add("09-operation-ratio", "Multiply / tail latency ratio", "Model entries per operand", "Median latency ratio", [Series("Multiply / tail", WARNING, list(SIZES), ratios)], horizontal=1.0)
    add("10-relative-confidence-width", "Criterion median confidence width", "Model entries per operand", "95% CI width", [Series("Tail probability", TAIL, list(SIZES), [100 * m.relative_ci_width for m in tail]), Series("Dyadic multiply", MULTIPLY, list(SIZES), [100 * m.relative_ci_width for m in multiply])], y_unit="%")
    tail_x, tail_slope = local_slopes(tail)
    multiply_x, multiply_slope = local_slopes(multiply)
    add("11-local-scaling-exponent", "Local scaling exponent", "Adjacent-size geometric midpoint", "d log(time) / d log(size)", [Series("Tail probability", TAIL, tail_x, tail_slope), Series("Dyadic multiply", MULTIPLY, multiply_x, multiply_slope)], horizontal=1.0)
    add("12-normalized-scaling", "Latency normalized to 8 entries", "Model entries per operand", "Multiple of 8-entry latency", [Series("Tail probability", TAIL, list(SIZES), [m.median_ns / tail[0].median_ns for m in tail]), Series("Dyadic multiply", MULTIPLY, list(SIZES), [m.median_ns / multiply[0].median_ns for m in multiply])], y_log=True)
    large_sizes = list(SIZES[4:])
    add("13-large-model-latency", "Large-model latency", "Model entries per operand", "Median time", [Series("Tail probability", TAIL, large_sizes, [m.median_ns for m in tail[4:]]), Series("Dyadic multiply", MULTIPLY, large_sizes, [m.median_ns for m in multiply[4:]])], y_log=True, y_unit="ns")
    return plots


def command_output(command: list[str]) -> str:
    try:
        return subprocess.run(command, check=True, capture_output=True, text=True).stdout.strip()
    except (OSError, subprocess.CalledProcessError):
        return "unavailable"


def format_ns(value: float) -> str:
    if value < 1_000:
        return f"{value:.2f} ns"
    if value < 1_000_000:
        return f"{value / 1_000:.2f} us"
    return f"{value / 1_000_000:.2f} ms"


def write_reports(results: dict[str, list[Measurement]], plots: list[tuple[str, str]], output: Path) -> None:
    with (output / "results.csv").open("w", newline="") as handle:
        writer = csv.writer(handle)
        writer.writerow(("operation", "size", "median_ns", "lower_95_ns", "upper_95_ns"))
        for values in results.values():
            for item in values:
                writer.writerow((item.operation, item.size, f"{item.median_ns:.6f}", f"{item.lower_ns:.6f}", f"{item.upper_ns:.6f}"))

    generated = datetime.now(timezone.utc).isoformat()
    revision = command_output(["jj", "log", "-r", "@", "--no-graph", "-T", 'commit_id.short() ++ " " ++ description.first_line()'])
    rustc = command_output(["rustc", "--version"])
    machine = platform.platform()
    rows = ["| Operation | Entries | Median | 95% confidence interval |", "|---|---:|---:|---:|"]
    for values in results.values():
        for item in values:
            rows.append(f"| {item.operation} | {item.size:,} | {format_ns(item.median_ns)} | {format_ns(item.lower_ns)} - {format_ns(item.upper_ns)} |")
    gallery = "\n\n".join(f"## {title}\n\n![{title}]({name}.svg)" for name, title in plots)
    (output / "README.md").write_text(
        f"""# Mercy benchmark report

Generated: `{generated}`  
Working-copy revision: `{revision}`  
Rust: `{rustc}`  
Platform: `{machine}`

This report visualizes one Criterion run of the two durable public-operation
benchmarks. Confidence bars use Criterion's median 95% interval. Treat this as
a structural baseline, not evidence for an optimization; performance decisions
need repeated comparable runs.

Raw values are in [results.csv](results.csv). Criterion's detailed density,
regression, and per-case plots remain under `target/criterion/report/index.html`.

{"\n".join(rows)}

{gallery}
"""
    )
    cards = "".join(f'<section><h2>{html.escape(title)}</h2><a href="{name}.svg"><img src="{name}.svg" alt="{html.escape(title)}"></a></section>' for name, title in plots)
    table_rows = "".join(f"<tr><td>{html.escape(item.operation)}</td><td>{item.size:,}</td><td>{format_ns(item.median_ns)}</td><td>{format_ns(item.lower_ns)} - {format_ns(item.upper_ns)}</td></tr>" for values in results.values() for item in values)
    (output / "index.html").write_text(
        f"""<!doctype html><html lang="en"><head><meta charset="utf-8"><meta name="viewport" content="width=device-width"><title>Mercy benchmark report</title><style>
:root {{ color-scheme:dark; font-family:Inter,system-ui,sans-serif; background:{BACKGROUND}; color:{FOREGROUND}; }} body {{ max-width:1500px; margin:auto; padding:2rem; }} h1 {{ color:{TAIL}; }} h2 {{ margin:0 0 1rem; }} .meta {{ color:{MUTED}; line-height:1.6; }} .gallery {{ display:grid; grid-template-columns:repeat(auto-fit,minmax(520px,1fr)); gap:1.2rem; }} section {{ background:{PANEL}; border:1px solid {GRID}; border-radius:14px; padding:1rem; }} img {{ width:100%; height:auto; }} table {{ border-collapse:collapse; width:100%; margin:2rem 0; }} th,td {{ padding:.65rem; border-bottom:1px solid {GRID}; text-align:right; }} th:first-child,td:first-child {{ text-align:left; }}</style></head><body>
<h1>Mercy benchmark report</h1><p class="meta">Generated {html.escape(generated)}<br>Revision {html.escape(revision)}<br>Rust {html.escape(rustc)}<br>Platform {html.escape(machine)}</p><p>One structural Criterion baseline. Repeat comparable runs before optimization decisions.</p><table><thead><tr><th>Operation</th><th>Entries</th><th>Median</th><th>95% confidence interval</th></tr></thead><tbody>{table_rows}</tbody></table><main class="gallery">{cards}</main></body></html>"""
    )


def main() -> None:
    args = parse_args()
    args.output_dir.mkdir(parents=True, exist_ok=True)
    results = load_results(args.criterion_root)
    plots = render_plots(results, args.output_dir)
    write_reports(results, plots, args.output_dir)
    print(f"wrote {len(plots)} SVG plots to {args.output_dir}")


if __name__ == "__main__":
    main()
