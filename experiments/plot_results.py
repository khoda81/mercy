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
import math
import re
from collections import defaultdict
from pathlib import Path
from typing import Any

ELO_SCALE = 400.0


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


def bradley_terry_scores(
    series: dict[str, dict[str, Any]], pseudo_wins: float = 0.5
) -> dict[str, float]:
    """Fit descriptive Bradley-Terry strengths to all cross-sample outcomes."""
    ids = list(series)
    count = len(ids)
    if count < 2:
        return {series_id: 1.0 for series_id in ids}

    wins = [0.0] * count
    games = [[0.0] * count for _ in range(count)]
    for candidate_index in range(count):
        candidate = series[ids[candidate_index]]["latency"]
        for reference_index in range(candidate_index + 1, count):
            reference = series[ids[reference_index]]["latency"]
            candidate_wins = pseudo_wins
            reference_wins = pseudo_wins
            for candidate_latency in candidate:
                for reference_latency in reference:
                    if candidate_latency < reference_latency:
                        candidate_wins += 1.0
                    elif candidate_latency > reference_latency:
                        reference_wins += 1.0
                    else:
                        candidate_wins += 0.5
                        reference_wins += 0.5
            wins[candidate_index] += candidate_wins
            wins[reference_index] += reference_wins
            games[candidate_index][reference_index] = candidate_wins + reference_wins
            games[reference_index][candidate_index] = candidate_wins + reference_wins

    strengths = [1.0] * count
    for _ in range(1_000):
        updated = []
        for candidate_index in range(count):
            denominator = math.fsum(
                games[candidate_index][reference_index]
                / (strengths[candidate_index] + strengths[reference_index])
                for reference_index in range(count)
                if reference_index != candidate_index
            )
            updated.append(wins[candidate_index] / denominator)

        geometric_mean = math.exp(
            math.fsum(math.log(strength) for strength in updated) / count
        )
        updated = [strength / geometric_mean for strength in updated]
        change = max(
            abs(math.log(new_strength / old_strength))
            for new_strength, old_strength in zip(updated, strengths)
        )
        strengths = updated
        if change < 1e-12:
            break

    return dict(zip(ids, strengths))


def elo_rating(strength: float) -> float:
    """Convert a Bradley-Terry strength to a zero-centered conventional Elo rating."""
    return ELO_SCALE * math.log10(strength)


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
    report = {
        family: dict(sorted(sizes.items(), key=lambda item: int(item[0])))
        for family, sizes in sorted(report.items())
    }
    for sizes in report.values():
        for series in sizes.values():
            for series_id, score in bradley_terry_scores(series).items():
                series[series_id]["bt_score"] = score
                series[series_id]["elo_score"] = elo_rating(score)
    return report


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
    .toggle { display: flex; align-self: end; overflow: hidden; border: 1px solid #475569; border-radius: 8px; }
    .toggle button { padding: 8px 12px; border: 0; border-right: 1px solid #475569; background: #0f172a; color: #94a3b8; cursor: pointer; }
    .toggle button:last-child { border-right: 0; }
    .toggle button:hover { color: #e5e7eb; background: #1e293b; }
    .toggle button.active { color: #07111f; background: #67e8f9; font-weight: 700; }
    .plot { min-height: 550px; }
    #pairwise-heatmap { min-height: 680px; }
    #effect-distribution { min-height: 680px; }
    #scaling-elo { grid-column: 1 / -1; }
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
  <p>Every value comes from Criterion's recorded sampled batches directly. No normal, log-normal, KDE, or other latency/throughput sampling distribution is fitted. The pairwise matrix uses a Bradley–Terry fit only to rank the empirical win counts.</p>

  <section class="card">
    <h2>Pairwise benchmark improvement</h2>
    <p>For reference latency <code>Aᵢ</code> and candidate latency <code>Bⱼ</code>, the empirical effect is <code>Iᵢⱼ = log(Aᵢ/Bⱼ)</code>. Positive values mean the candidate is faster. The probability matrix shows <code>P(B &lt; A)</code>: rows are candidates, columns are references, ties count one half, and the diagonal is 50%. Elo mode instead shows candidate Elo minus reference Elo. Both axes are sorted from highest to lowest Bradley–Terry score fitted to the full pairwise win counts. Strengths are normalized to geometric mean 1 and Elo uses <code>400 log₁₀(strength)</code>, centered at zero. A symmetric half-win pseudocount per pair keeps complete separations finite.</p>
    <div class="controls">
      <label>Operation<select id="comparison-family"></select></label>
      <label>Entries<select id="comparison-size"></select></label>
      <label>Reference run<select id="comparison-reference"></select></label>
      <div class="toggle" role="group" aria-label="Matrix value">
        <button id="matrix-probability" class="active" type="button">Probability</button>
        <button id="matrix-elo" type="button">Elo Δ</button>
      </div>
    </div>
    <div id="pairwise-heatmap" class="plot"></div>
    <div id="effect-distribution" class="plot"></div>
    <div id="effect-summary"></div>
    <p class="note">Each curve contains all cross-pair effects for one candidate against the selected reference. Those <code>n×m</code> values describe an empirical effect-size distribution; they are not claimed to be <code>n×m</code> independent samples. Bradley–Terry scores are used only as a descriptive global ranking; no independence-based uncertainty or significance is claimed. No fitted latency distribution, KDE, or Q-Q score is used.</p>
  </section>

  <section class="card">
    <h2>All-run scaling across entries</h2>
    <p>Every recorded run is shown across its available input scales. Each point is the median of the recorded Criterion sample batches at that scale. Both performance plots use logarithmic entry and value axes; latency is reversed so faster results are higher. Elo is relative to the contenders available at each operation and scale, so its curve is most useful for comparing rank and crossover behavior rather than absolute performance.</p>
    <div class="controls">
      <label>Operation<select id="scaling-family"></select></label>
    </div>
    <div class="grid">
      <div id="scaling-throughput" class="plot"></div>
      <div id="scaling-latency" class="plot"></div>
      <div id="scaling-elo" class="plot"></div>
    </div>
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

    function colorFor(seriesId) {
      const allIds = [];
      for (const sizes of Object.values(DATA)) {
        for (const series of Object.values(sizes)) allIds.push(...Object.keys(series));
      }
      const unique = [...new Set(allIds)].sort();
      return COLORS[unique.indexOf(seriesId) % COLORS.length];
    }

    function addOption(select, value, label) {
      const option = document.createElement('option');
      option.value = value;
      option.textContent = label;
      select.appendChild(option);
    }

    function median(values) {
      const ordered = sortedNumbers(values);
      const middle = Math.floor(ordered.length / 2);
      return ordered.length % 2
        ? ordered[middle]
        : (ordered[middle - 1] + ordered[middle]) / 2;
    }

    function scaleSeries(family) {
      const grouped = new Map();
      const sizes = Object.keys(DATA[family]).sort((a, b) => Number(a) - Number(b));
      for (const size of sizes) {
        for (const [seriesId, series] of Object.entries(DATA[family][size])) {
          if (!grouped.has(seriesId)) {
            grouped.set(seriesId, {seriesId, label: series.label, points: []});
          }
          grouped.get(seriesId).points.push({
            entries: Number(size),
            latency: median(series.latency),
            throughput: median(series.throughput),
            elo: series.elo_score,
            sampleCount: series.latency.length
          });
        }
      }
      return [...grouped.values()].sort((a, b) =>
        a.label.localeCompare(b.label) || a.seriesId.localeCompare(b.seriesId)
      );
    }

    function scaleTrace(item, metric) {
      const y = item.points.map(point => point[metric]);
      const customdata = item.points.map(point => [
        point.latency,
        point.throughput,
        point.elo,
        point.sampleCount
      ]);
      const valueLine = metric === 'latency'
        ? 'median latency=%{customdata[0]:.5g} ns'
        : metric === 'throughput'
          ? 'median throughput=%{customdata[1]:.5g}/s'
          : 'Bradley–Terry Elo=%{customdata[2]:+.1f}';
      return {
        type: 'scatter',
        mode: 'lines+markers',
        name: item.label,
        x: item.points.map(point => point.entries),
        y,
        customdata,
        connectgaps: false,
        line: {width: 1.8, color: colorFor(item.seriesId)},
        marker: {
          size: 8,
          color: colorFor(item.seriesId),
          opacity: 1,
          line: {color: '#08101f', width: 1.25}
        },
        hovertemplate: `<b>%{fullData.name}</b><br>entries=%{x:,.0f}<br>${valueLine}<br>Criterion batches=%{customdata[3]:.0f}<extra></extra>`
      };
    }

    function scaleLayout(family, metric) {
      const latency = metric === 'latency';
      const throughput = metric === 'throughput';
      const title = latency ? 'median latency' : throughput ? 'median throughput' : 'Bradley–Terry Elo';
      const entryScales = Object.keys(DATA[family])
        .map(Number)
        .sort((a, b) => a - b);
      const yaxis = {
        title: {
          text: latency
            ? 'Median latency per operation (ns, log; faster is higher)'
            : throughput
              ? 'Median throughput per second (log; higher is better)'
              : 'Bradley–Terry Elo (higher is better)'
        },
        gridcolor: '#253247'
      };
      if (latency || throughput) yaxis.type = 'log';
      if (latency) yaxis.autorange = 'reversed';
      if (!latency && !throughput) {
        yaxis.zeroline = true;
        yaxis.zerolinecolor = '#94a3b8';
      }
      return {
        ...BASE_LAYOUT,
        height: 620,
        uirevision: family,
        title: {text: `${family} · ${title} across input scale`},
        xaxis: {
          title: {text: 'Entries (log scale)'},
          type: 'log',
          tickmode: 'array',
          tickvals: entryScales,
          ticktext: entryScales.map(value => value.toLocaleString()),
          gridcolor: '#253247'
        },
        yaxis,
        legend: {orientation: 'h', y: -0.22}
      };
    }

    function renderScalingDashboard() {
      const family = scalingFamily.value;
      const items = scaleSeries(family);
      for (const metric of ['throughput', 'latency', 'elo']) {
        const traces = items.map(item => scaleTrace(item, metric));
        Plotly.react(`scaling-${metric}`, traces, scaleLayout(family, metric), CONFIG);
      }
    }

    function sortedSeriesEntries(series) {
      return Object.entries(series).sort(([idA, itemA], [idB, itemB]) => {
        const scoreDifference = itemB.bt_score - itemA.bt_score;
        return scoreDifference || itemA.label.localeCompare(itemB.label) || idA.localeCompare(idB);
      });
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
    const matrixProbability = document.getElementById('matrix-probability');
    const matrixElo = document.getElementById('matrix-elo');
    const scalingFamily = document.getElementById('scaling-family');
    let matrixMode = 'probability';

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
      const entries = sortedSeriesEntries(series).map(([id, item]) => [
        id,
        `${item.label} · Elo ${item.elo_score >= 0 ? '+' : ''}${item.elo_score.toFixed(0)}`
      ]);
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
      const entries = sortedSeriesEntries(series);
      const labels = entries.map(([, item]) => item.label);
      const probabilityMatrix = entries.map(([candidateId, candidate]) =>
        entries.map(([referenceId, reference]) =>
          candidateId === referenceId
            ? 0.5
            : probabilityOfSuperiority(reference, candidate)
        )
      );
      const eloMatrix = entries.map(([, candidate]) =>
        entries.map(([, reference]) => candidate.elo_score - reference.elo_score)
      );
      const customdata = entries.map(([, candidate], candidateIndex) =>
        entries.map(([, reference], referenceIndex) => [
          candidate.bt_score,
          reference.bt_score,
          candidate.elo_score,
          reference.elo_score,
          probabilityMatrix[candidateIndex][referenceIndex]
        ])
      );
      const probabilityMode = matrixMode === 'probability';
      const matrix = probabilityMode ? probabilityMatrix : eloMatrix;
      const text = probabilityMode
        ? matrix.map(row => row.map(value => `${(value * 100).toFixed(1)}%`))
        : matrix.map(row => row.map(value => `${value >= 0 ? '+' : ''}${value.toFixed(0)}`));
      const eloExtent = Math.max(...eloMatrix.flat().map(Math.abs), 1);
      const trace = {
        type: 'heatmap',
        x: labels,
        y: labels,
        z: matrix,
        text,
        customdata,
        texttemplate: '%{text}',
        textfont: {color: '#f8fafc'},
        zmin: probabilityMode ? 0 : -eloExtent,
        zmax: probabilityMode ? 1 : eloExtent,
        zmid: probabilityMode ? 0.5 : 0,
        colorscale: [[0, RED], [0.5, '#334155'], [1, GREEN]],
        colorbar: probabilityMode
          ? {title: {text: 'P(candidate faster)'}, tickformat: '.0%'}
          : {title: {text: 'Candidate − reference Elo'}},
        hovertemplate: probabilityMode
          ? '<b>candidate</b> %{y}<br>Elo=%{customdata[2]:.1f}<br><b>reference</b> %{x}<br>Elo=%{customdata[3]:.1f}<br>P(candidate faster)=%{z:.2%}<extra></extra>'
          : '<b>candidate</b> %{y}<br>Elo=%{customdata[2]:.1f}<br><b>reference</b> %{x}<br>Elo=%{customdata[3]:.1f}<br>candidate − reference Elo=%{z:.1f}<br>empirical P(candidate faster)=%{customdata[4]:.2%}<extra></extra>'
      };
      const layout = {
        ...BASE_LAYOUT,
        height: Math.max(680, labels.length * 88 + 300),
        margin: {l: 300, r: 100, t: 82, b: 210},
        title: {text: `${family} · ${Number(size).toLocaleString()} entries · ${probabilityMode ? 'empirical probability of superiority' : 'Bradley–Terry Elo difference'} · highest score → lowest`},
        xaxis: {title: {text: 'Reference run (A) · highest BT score → lowest'}, side: 'bottom', tickangle: -35},
        yaxis: {title: {text: 'Candidate run (B) · highest BT score → lowest'}, autorange: 'reversed'}
      };
      Plotly.react('pairwise-heatmap', [trace], layout, CONFIG);
    }

    function renderSummary(rows) {
      const host = document.getElementById('effect-summary');
      host.replaceChildren();
      const table = document.createElement('table');
      table.className = 'summary';
      const header = table.createTHead().insertRow();
      for (const label of ['Candidate vs selected reference', 'Bradley–Terry score', 'Elo', 'P(improvement)', 'Median log improvement', 'Median speedup']) {
        const cell = document.createElement('th');
        cell.textContent = label;
        header.appendChild(cell);
      }
      const body = table.createTBody();
      for (const row of rows) {
        const cells = [
          row.label,
          row.btScore.toFixed(5),
          `${row.elo >= 0 ? '+' : ''}${row.elo.toFixed(1)}`,
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
      for (const [candidateId, candidate] of sortedSeriesEntries(series)) {
        if (candidateId === referenceId) continue;
        const stats = pairwiseEffects(reference, candidate);
        const effects = sortedNumbers(stats.effects);
        const cumulative = effects.map((_, index) => (index + 0.5) / effects.length);
        rows.push({label: candidate.label, btScore: candidate.bt_score, elo: candidate.elo_score, ...stats});
        traces.push({
          type: 'scattergl',
          mode: 'lines',
          name: `${candidate.label} · P=${(stats.probability * 100).toFixed(1)}% · median I=${stats.medianLog.toFixed(4)} · ${stats.medianSpeedup.toFixed(3)}×`,
          x: effects,
          y: cumulative,
          customdata: effects.map(value => Math.exp(value)),
          line: {width: 2, color: colorFor(candidateId)},
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

    const families = compatibleFamilies();
    refill(comparisonFamily, families.map(family => [family, family]), families.includes('Tail probability') ? 'Tail probability' : families[0]);
    syncComparisonSizes(true);
    const scalingFamilies = Object.keys(DATA);
    refill(scalingFamily, scalingFamilies.map(family => [family, family]), scalingFamilies.includes('Tail probability') ? 'Tail probability' : scalingFamilies[0]);
    renderScalingDashboard();
    comparisonFamily.addEventListener('change', () => syncComparisonSizes(false));
    comparisonSize.addEventListener('change', () => syncReferences(false));
    scalingFamily.addEventListener('change', renderScalingDashboard);
    comparisonReference.addEventListener('change', () => {
      const family = comparisonFamily.value;
      const size = comparisonSize.value;
      renderEffectDistributions(family, size, DATA[family][size]);
    });
    for (const [button, mode] of [[matrixProbability, 'probability'], [matrixElo, 'elo']]) {
      button.addEventListener('click', () => {
        matrixMode = mode;
        matrixProbability.classList.toggle('active', mode === 'probability');
        matrixElo.classList.toggle('active', mode === 'elo');
        const family = comparisonFamily.value;
        const size = comparisonSize.value;
        renderHeatmap(family, size, DATA[family][size]);
      });
    }
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
