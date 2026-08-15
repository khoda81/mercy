#!/usr/bin/env -S uv run
# /// script
# requires-python = ">=3.11"
# dependencies = ["plotly>=6.5"]
# ///
"""Build one standalone Plotly report from every recorded Criterion snapshot."""

from __future__ import annotations

import argparse
import csv
import json
import re
from collections import defaultdict
from pathlib import Path
from typing import Any


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--samples-dir",
        type=Path,
        default=Path("artifacts/benchmark-report/samples"),
    )
    parser.add_argument(
        "--output",
        type=Path,
        default=Path("artifacts/benchmark-report/index.html"),
    )
    return parser.parse_args()


def operation_parts(operation: str) -> tuple[str, str]:
    match = re.fullmatch(r"(.+?) \(([^()]+)\)", operation)
    if match:
        return match.group(1), match.group(2)
    if operation == "Tail probability":
        return operation, "online-balanced"
    return operation, ""


def load_samples(samples_dir: Path) -> dict[str, dict[str, dict[str, Any]]]:
    grouped: dict[tuple[str, int, str, str, str], dict[str, list[float]]] = defaultdict(
        lambda: {"latency": [], "throughput": []}
    )

    paths = sorted(samples_dir.glob("*.csv"))
    if not paths:
        raise FileNotFoundError(f"no sample snapshots found under {samples_dir}")

    for path in paths:
        with path.open(newline="") as handle:
            for row in csv.DictReader(handle):
                run = row["run"]
                operation = row["operation"]
                family, implementation = operation_parts(operation)
                size = int(row["size"])
                series_id = f"{run}\N{SYMBOL FOR UNIT SEPARATOR}{operation}"
                label = run
                if implementation:
                    label = f"{run} · {implementation}"
                values = grouped[family, size, series_id, label, implementation]
                values["latency"].append(float(row["latency_ns"]))
                values["throughput"].append(float(row["throughput_per_second"]))

    report: dict[str, dict[str, dict[str, Any]]] = defaultdict(
        lambda: defaultdict(dict)
    )
    for (family, size, series_id, label, implementation), values in grouped.items():
        run = series_id.split("\N{SYMBOL FOR UNIT SEPARATOR}", 1)[0]
        report[family][str(size)][series_id] = {
            "label": label,
            "run": run,
            "implementation": implementation,
            "latency": sorted(values["latency"]),
            "throughput": sorted(values["throughput"]),
        }
    return {
        family: dict(sorted(sizes.items(), key=lambda item: int(item[0])))
        for family, sizes in sorted(report.items())
    }


HTML_TEMPLATE = r"""<!doctype html>
<html lang="en">
<head>
  <meta charset="utf-8">
  <meta name="viewport" content="width=device-width,initial-scale=1">
  <title>Mercy empirical benchmark report</title>
  <script>__PLOTLY_JS__</script>
  <style>
    :root { color-scheme: dark; font-family: Inter, ui-sans-serif, system-ui, sans-serif; background: #090d18; color: #e5e7eb; }
    * { box-sizing: border-box; }
    body { margin: 0 auto; max-width: 1600px; padding: 28px; }
    h1 { margin-bottom: 6px; color: #a3e635; }
    h2 { margin: 0; }
    p { color: #aebbd0; line-height: 1.55; }
    code { color: #67e8f9; }
    .grid { display: grid; grid-template-columns: repeat(auto-fit, minmax(min(680px, 100%), 1fr)); gap: 20px; }
    .card { margin: 20px 0; padding: 20px; border: 1px solid #334155; border-radius: 16px; background: #111827; box-shadow: 0 14px 36px rgb(0 0 0 / 24%); }
    .controls { display: flex; flex-wrap: wrap; gap: 12px; margin: 14px 0; }
    label { display: grid; gap: 5px; color: #94a3b8; font-size: 13px; }
    select { min-width: 180px; padding: 8px 30px 8px 10px; border: 1px solid #475569; border-radius: 8px; background: #0f172a; color: #e5e7eb; }
    .plot { min-height: 550px; }
    #pairwise-heatmap { min-height: 680px; }
    #effect-distribution { min-height: 680px; }
    .note { margin: 6px 0 0; color: #94a3b8; font-size: 14px; }
    .summary { width: 100%; margin-top: 14px; border-collapse: collapse; font-variant-numeric: tabular-nums; }
    .summary th, .summary td { padding: 9px 12px; border-bottom: 1px solid #334155; text-align: right; }
    .summary th:first-child, .summary td:first-child { text-align: left; }
    .summary th { color: #94a3b8; }
    @media (max-width: 720px) { body { padding: 14px; } .card { padding: 12px; } .plot { min-height: 430px; } select { min-width: 150px; } }
  </style>
</head>
<body>
  <h1>Mercy empirical benchmark report</h1>
  <p>Every line uses Criterion's recorded sampled batches directly. No normal, log-normal, KDE, regression, or other sampling distribution is fitted.</p>

  <section class="card">
    <h2>All-run empirical distributions</h2>
    <p>Every available run is visible for the selected operation size. Throughput increases to the right. The latency axis is reversed so lower latency—and therefore better performance—is farther to the right.</p>
    <div id="distribution-sections"></div>
  </section>

  <section class="card">
    <h2>Pairwise benchmark improvement</h2>
    <p>For reference latency <code>Aᵢ</code> and candidate latency <code>Bⱼ</code>, the empirical effect is <code>Iᵢⱼ = log(Aᵢ/Bⱼ)</code>. Positive values mean the candidate is faster. The matrix shows <code>P(B &lt; A)</code>: rows are candidates, columns are references, ties count one half, and the diagonal is 50%.</p>
    <div class="controls">
      <label>Operation<select id="comparison-family"></select></label>
      <label>Entries<select id="comparison-size"></select></label>
      <label>Reference run<select id="comparison-reference"></select></label>
    </div>
    <div id="pairwise-heatmap" class="plot"></div>
    <div id="effect-distribution" class="plot"></div>
    <div id="effect-summary"></div>
    <p class="note">Each curve contains all cross-pair effects for one candidate against the selected reference. Those <code>n×m</code> values describe an empirical effect-size distribution; they are not claimed to be <code>n×m</code> independent samples. No fitted distribution, KDE, Q-Q score, or independence-based uncertainty calculation is used.</p>
  </section>

  <script>
    const DATA = __DATA_JSON__;
    const COLORS = ['#22d3ee', '#f472b6', '#a3e635', '#fbbf24', '#c084fc', '#fb7185', '#60a5fa', '#34d399'];
    const GREEN = '#22c55e';
    const RED = '#ef4444';
    const CONFIG = {responsive: true, displaylogo: false};
    const BASE_LAYOUT = {
      template: 'plotly_dark',
      paper_bgcolor: 'rgba(0,0,0,0)',
      plot_bgcolor: '#0b1220',
      font: {color: '#dbe4f0'},
      margin: {l: 84, r: 32, t: 72, b: 76},
      hovermode: 'closest',
      legend: {orientation: 'h', y: -0.18}
    };

    function sortedNumbers(values) {
      return [...values].sort((a, b) => a - b);
    }

    function cdf(values) {
      const x = sortedNumbers(values);
      const n = x.length;
      return {x, y: x.map((_, index) => (index + 0.5) / n)};
    }

    function colorFor(seriesId) {
      const allIds = [];
      for (const sizes of Object.values(DATA)) {
        for (const series of Object.values(sizes)) allIds.push(...Object.keys(series));
      }
      const unique = [...new Set(allIds)].sort();
      return COLORS[unique.indexOf(seriesId) % COLORS.length];
    }

    function distributionTrace(seriesId, series, metric) {
      const points = cdf(series[metric]);
      const latency = metric === 'latency';
      return {
        type: 'scatter', mode: 'lines',
        name: series.label,
        x: points.x, y: points.y,
        line: {width: 2.4, color: colorFor(seriesId)},
        hovertemplate: latency
          ? '<b>%{fullData.name}</b><br>latency=%{x:.4g} ns<br>CDF=%{y:.1%}<extra></extra>'
          : '<b>%{fullData.name}</b><br>throughput=%{x:.4g}/s<br>CDF=%{y:.1%}<extra></extra>'
      };
    }

    function distributionLayout(family, size, metric) {
      const latency = metric === 'latency';
      return {
        ...BASE_LAYOUT,
        title: {text: `${family} · ${Number(size).toLocaleString()} entries · ${latency ? 'latency' : 'throughput'} ECDF`},
        xaxis: {
          title: {text: latency ? 'Latency per operation (ns, log; lower is farther right)' : 'Throughput per second (log; higher is farther right)'},
          type: 'log', autorange: latency ? 'reversed' : true, gridcolor: '#253247'
        },
        yaxis: {title: {text: 'Empirical cumulative probability'}, range: [0, 1], tickformat: '.0%', gridcolor: '#253247'}
      };
    }

    function renderDistribution(family, size, metric, elementId) {
      const series = DATA[family][size];
      const traces = Object.entries(series).map(([id, item]) => distributionTrace(id, item, metric));
      Plotly.react(elementId, traces, distributionLayout(family, size, metric), CONFIG);
    }

    function addOption(select, value, label) {
      const option = document.createElement('option');
      option.value = value;
      option.textContent = label;
      select.appendChild(option);
    }

    function buildDistributionSections() {
      const host = document.getElementById('distribution-sections');
      for (const family of Object.keys(DATA)) {
        const section = document.createElement('section');
        section.className = 'card';
        const title = document.createElement('h2');
        title.textContent = family;
        const controls = document.createElement('div');
        controls.className = 'controls';
        const label = document.createElement('label');
        label.textContent = 'Entries';
        const select = document.createElement('select');
        const sizes = Object.keys(DATA[family]).sort((a, b) => Number(a) - Number(b));
        for (const size of sizes) addOption(select, size, Number(size).toLocaleString());
        select.value = sizes.includes('50000') ? '50000' : sizes[0];
        label.appendChild(select);
        controls.appendChild(label);
        const grid = document.createElement('div');
        grid.className = 'grid';
        const latency = document.createElement('div');
        const throughput = document.createElement('div');
        latency.className = throughput.className = 'plot';
        const slug = family.toLowerCase().replace(/[^a-z0-9]+/g, '-');
        latency.id = `${slug}-latency`;
        throughput.id = `${slug}-throughput`;
        grid.append(latency, throughput);
        section.append(title, controls, grid);
        host.appendChild(section);
        const update = () => {
          renderDistribution(family, select.value, 'latency', latency.id);
          renderDistribution(family, select.value, 'throughput', throughput.id);
        };
        select.addEventListener('change', update);
        update();
      }
    }

    function median(values) {
      const ordered = sortedNumbers(values);
      const middle = Math.floor(ordered.length / 2);
      return ordered.length % 2
        ? ordered[middle]
        : (ordered[middle - 1] + ordered[middle]) / 2;
    }

    function probabilityOfSuperiority(reference, candidate) {
      let wins = 0;
      let ties = 0;
      for (const referenceLatency of reference.latency) {
        for (const candidateLatency of candidate.latency) {
          if (candidateLatency < referenceLatency) wins += 1;
          else if (candidateLatency === referenceLatency) ties += 1;
        }
      }
      return (wins + ties * 0.5) / (reference.latency.length * candidate.latency.length);
    }

    function pairwiseEffects(reference, candidate) {
      const effects = [];
      for (const referenceLatency of reference.latency) {
        for (const candidateLatency of candidate.latency) {
          effects.push(Math.log(referenceLatency / candidateLatency));
        }
      }
      const medianLog = median(effects);
      return {
        effects,
        probability: probabilityOfSuperiority(reference, candidate),
        medianLog,
        medianSpeedup: Math.exp(medianLog)
      };
    }

    const comparisonFamily = document.getElementById('comparison-family');
    const comparisonSize = document.getElementById('comparison-size');
    const comparisonReference = document.getElementById('comparison-reference');

    function refill(select, entries, preferred) {
      select.replaceChildren();
      for (const [value, label] of entries) addOption(select, value, label);
      if (preferred && entries.some(([value]) => value === preferred)) select.value = preferred;
    }

    function compatibleFamilies() {
      return Object.keys(DATA).filter(family =>
        Object.values(DATA[family]).some(series => Object.keys(series).length >= 2)
      );
    }

    function syncComparisonSizes(initial = false) {
      const sizes = Object.entries(DATA[comparisonFamily.value])
        .filter(([, series]) => Object.keys(series).length >= 2)
        .map(([size]) => size)
        .sort((a, b) => Number(a) - Number(b));
      refill(
        comparisonSize,
        sizes.map(size => [size, Number(size).toLocaleString()]),
        initial && sizes.includes('50000') ? '50000' : comparisonSize.value
      );
      syncReferences(initial);
    }

    function syncReferences(initial = false) {
      const series = DATA[comparisonFamily.value][comparisonSize.value];
      const entries = Object.entries(series).map(([id, item]) => [id, item.label]);
      let reference = comparisonReference.value;
      if (initial) {
        const preferred = Object.entries(series).find(
          ([, item]) => item.run === 'candidate-modules-run-a' && item.implementation === 'online-balanced'
        );
        reference = preferred ? preferred[0] : entries[0][0];
      }
      refill(comparisonReference, entries, reference);
      renderPairwiseDashboard();
    }

    function renderHeatmap(family, size, series) {
      const entries = Object.entries(series);
      const labels = entries.map(([, item]) => item.label);
      const matrix = entries.map(([candidateId, candidate]) =>
        entries.map(([referenceId, reference]) =>
          candidateId === referenceId
            ? 0.5
            : probabilityOfSuperiority(reference, candidate)
        )
      );
      const text = matrix.map(row => row.map(value => `${(value * 100).toFixed(1)}%`));
      const trace = {
        type: 'heatmap',
        x: labels,
        y: labels,
        z: matrix,
        text,
        texttemplate: '%{text}',
        textfont: {color: '#f8fafc'},
        zmin: 0,
        zmax: 1,
        zmid: 0.5,
        colorscale: [[0, RED], [0.5, '#334155'], [1, GREEN]],
        colorbar: {title: {text: 'P(candidate faster)'}, tickformat: '.0%'},
        hovertemplate: '<b>candidate</b> %{y}<br><b>reference</b> %{x}<br>P(candidate faster)=%{z:.2%}<extra></extra>'
      };
      const layout = {
        ...BASE_LAYOUT,
        height: Math.max(680, labels.length * 88 + 300),
        margin: {l: 300, r: 100, t: 82, b: 210},
        title: {text: `${family} · ${Number(size).toLocaleString()} entries · probability of superiority`},
        xaxis: {title: {text: 'Reference run (A)'}, side: 'bottom', tickangle: -35},
        yaxis: {title: {text: 'Candidate run (B)'}, autorange: 'reversed'}
      };
      Plotly.react('pairwise-heatmap', [trace], layout, CONFIG);
    }

    function renderSummary(rows) {
      const host = document.getElementById('effect-summary');
      host.replaceChildren();
      const table = document.createElement('table');
      table.className = 'summary';
      const header = table.createTHead().insertRow();
      for (const label of ['Candidate vs selected reference', 'P(improvement)', 'Median log improvement', 'Median speedup']) {
        const cell = document.createElement('th');
        cell.textContent = label;
        header.appendChild(cell);
      }
      const body = table.createTBody();
      for (const row of rows) {
        const cells = [
          row.label,
          `${(row.probability * 100).toFixed(2)}%`,
          row.medianLog.toFixed(5),
          `${row.medianSpeedup.toFixed(4)}×`
        ];
        const tableRow = body.insertRow();
        for (const value of cells) {
          const cell = tableRow.insertCell();
          cell.textContent = value;
        }
      }
      host.appendChild(table);
    }

    function renderEffectDistributions(family, size, series) {
      const referenceId = comparisonReference.value;
      const reference = series[referenceId];
      const rows = [];
      const traces = [];
      for (const [candidateId, candidate] of Object.entries(series)) {
        if (candidateId === referenceId) continue;
        const stats = pairwiseEffects(reference, candidate);
        const effects = sortedNumbers(stats.effects);
        const cumulative = effects.map((_, index) => (index + 0.5) / effects.length);
        rows.push({label: candidate.label, ...stats});
        traces.push({
          type: 'scatter',
          mode: 'lines',
          name: `${candidate.label} · P=${(stats.probability * 100).toFixed(1)}% · median I=${stats.medianLog.toFixed(4)} · ${stats.medianSpeedup.toFixed(3)}×`,
          x: effects,
          y: cumulative,
          customdata: effects.map(value => Math.exp(value)),
          line: {width: 2.5, color: colorFor(candidateId)},
          hovertemplate: '<b>%{fullData.name}</b><br>log(reference/candidate)=%{x:.6f}<br>multiplicative speedup=%{customdata:.5f}×<br>empirical CDF=%{y:.2%}<extra></extra>'
        });
      }
      const allEffects = traces.flatMap(trace => trace.x);
      const extent = Math.max(...allEffects.map(Math.abs), 1e-9) * 1.06;
      const layout = {
        ...BASE_LAYOUT,
        height: 700,
        margin: {l: 90, r: 42, t: 92, b: 150},
        title: {text: `Cross-pair log improvements against ${reference.label}`},
        xaxis: {
          title: {text: 'log(reference latency / candidate latency) · negative = regression · positive = improvement'},
          range: [-extent, extent],
          gridcolor: '#253247',
          zeroline: false
        },
        yaxis: {
          title: {text: 'Empirical cumulative probability'},
          range: [0, 1],
          tickformat: '.0%',
          gridcolor: '#253247'
        },
        legend: {orientation: 'h', y: -0.28},
        shapes: [
          {type: 'rect', xref: 'x', yref: 'paper', x0: -extent, x1: 0, y0: 0, y1: 1, fillcolor: 'rgba(239,68,68,0.12)', line: {width: 0}, layer: 'below'},
          {type: 'rect', xref: 'x', yref: 'paper', x0: 0, x1: extent, y0: 0, y1: 1, fillcolor: 'rgba(34,197,94,0.12)', line: {width: 0}, layer: 'below'},
          {type: 'line', xref: 'x', yref: 'paper', x0: 0, x1: 0, y0: 0, y1: 1, line: {color: '#e5e7eb', width: 2, dash: 'dash'}}
        ],
        annotations: [
          {xref: 'paper', yref: 'paper', x: 0.02, y: 0.98, text: '<b style="color:#ef4444">REGRESSION</b>', showarrow: false},
          {xref: 'paper', yref: 'paper', x: 0.98, y: 0.98, xanchor: 'right', text: '<b style="color:#22c55e">IMPROVEMENT</b>', showarrow: false}
        ]
      };
      Plotly.react('effect-distribution', traces, layout, CONFIG);
      renderSummary(rows);
    }

    function renderPairwiseDashboard() {
      const family = comparisonFamily.value;
      const size = comparisonSize.value;
      const series = DATA[family][size];
      renderHeatmap(family, size, series);
      renderEffectDistributions(family, size, series);
    }

    buildDistributionSections();
    const families = compatibleFamilies();
    refill(comparisonFamily, families.map(family => [family, family]), families.includes('Tail probability') ? 'Tail probability' : families[0]);
    syncComparisonSizes(true);
    comparisonFamily.addEventListener('change', () => syncComparisonSizes(false));
    comparisonSize.addEventListener('change', () => syncReferences(false));
    comparisonReference.addEventListener('change', () => {
      const family = comparisonFamily.value;
      const size = comparisonSize.value;
      renderEffectDistributions(family, size, DATA[family][size]);
    });
  </script>
</body>
</html>
"""


def write_report(data: dict[str, dict[str, dict[str, Any]]], output: Path) -> None:
    from plotly.offline.offline import get_plotlyjs

    payload = json.dumps(data, separators=(",", ":")).replace("</", "<\\/")
    document = HTML_TEMPLATE.replace("__PLOTLY_JS__", get_plotlyjs()).replace(
        "__DATA_JSON__", payload
    )
    output.parent.mkdir(parents=True, exist_ok=True)
    output.write_text(document)


def main() -> None:
    args = parse_args()
    data = load_samples(args.samples_dir)
    write_report(data, args.output)
    series_count = sum(
        len(series) for sizes in data.values() for series in sizes.values()
    )
    print(
        f"wrote standalone Plotly report with {len(data)} operations and "
        f"{series_count} operation/size/run series to {args.output}"
    )


if __name__ == "__main__":
    main()
