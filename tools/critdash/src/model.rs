use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DashboardState {
    pub schema_version: u32,
    pub runs: Vec<Run>,
}

impl Default for DashboardState {
    fn default() -> Self {
        Self {
            schema_version: 1,
            runs: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Run {
    pub id: String,
    pub label: String,
    pub started_at_unix_ms: u128,
    pub project: String,
    pub revision: String,
    pub machine: String,
    pub rustc: String,
    pub cargo_criterion: String,
    pub complete: bool,
    pub benchmarks: BTreeMap<String, Benchmark>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Benchmark {
    pub id: String,
    pub family: String,
    pub candidate: String,
    pub scale: String,
    pub unit: String,
    pub iteration_count: Vec<u64>,
    pub measured_values: Vec<f64>,
    pub samples_per_iteration: Vec<f64>,
    #[serde(default)]
    pub throughput: Vec<Throughput>,
    pub typical: Option<Estimate>,
    pub mean: Option<Estimate>,
    pub median: Option<Estimate>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Throughput {
    pub per_iteration: f64,
    pub unit: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Estimate {
    pub estimate: f64,
    pub lower_bound: f64,
    pub upper_bound: f64,
    pub unit: String,
}

#[derive(Debug, Deserialize)]
pub struct CargoCriterionMessage {
    pub reason: String,
    #[serde(default)]
    pub id: String,
    #[serde(default)]
    pub iteration_count: Vec<u64>,
    #[serde(default)]
    pub measured_values: Vec<f64>,
    #[serde(default)]
    pub unit: String,
    #[serde(default)]
    pub throughput: Vec<Throughput>,
    pub typical: Option<Estimate>,
    pub mean: Option<Estimate>,
    pub median: Option<Estimate>,
}

impl Benchmark {
    pub fn from_message(message: CargoCriterionMessage) -> anyhow::Result<Self> {
        anyhow::ensure!(
            message.iteration_count.len() == message.measured_values.len(),
            "benchmark {} has {} iteration counts but {} measured values",
            message.id,
            message.iteration_count.len(),
            message.measured_values.len()
        );

        let samples_per_iteration = message
            .iteration_count
            .iter()
            .zip(&message.measured_values)
            .map(|(&iterations, &value)| value / iterations as f64)
            .collect();
        let (family, candidate, scale) = infer_dimensions(&message.id);

        Ok(Self {
            id: message.id,
            family,
            candidate,
            scale,
            unit: message.unit,
            iteration_count: message.iteration_count,
            measured_values: message.measured_values,
            samples_per_iteration,
            throughput: message.throughput,
            typical: message.typical,
            mean: message.mean,
            median: message.median,
        })
    }
}

/// Infer a comparison key from Criterion's slash-separated benchmark id.
///
/// The zero-config convention is:
///
/// `family[/fixture]/candidate/scale`
///
/// when the final segment is numeric, otherwise `family/candidate`.
/// Unknown shapes still remain visible; they simply form smaller groups.
pub fn infer_dimensions(id: &str) -> (String, String, String) {
    let parts: Vec<&str> = id.split('/').filter(|part| !part.is_empty()).collect();
    match parts.as_slice() {
        [] => (String::new(), String::new(), "default".into()),
        [only] => ((*only).into(), "default".into(), "default".into()),
        [family, scale] if is_scale(scale) => ((*family).into(), "default".into(), (*scale).into()),
        _ if is_scale(parts[parts.len() - 1]) => {
            let candidate = parts[parts.len() - 2].to_owned();
            let family = parts[..parts.len() - 2].join("/");
            let scale = parts[parts.len() - 1].to_owned();
            (family, candidate.into(), scale.into())
        }
        _ => {
            let candidate = parts[parts.len() - 1].to_owned();
            let family = parts[..parts.len() - 1].join("/");
            (family, candidate.into(), "default".into())
        }
    }
}

fn is_scale(value: &str) -> bool {
    value.parse::<f64>().is_ok()
}

#[cfg(test)]
mod tests {
    use super::infer_dimensions;

    #[test]
    fn infers_mercy_shape() {
        assert_eq!(
            infer_dimensions("tail/flat/online-balanced/4096"),
            (
                "tail/flat".to_owned(),
                "online-balanced".to_owned(),
                "4096".to_owned()
            )
        );
    }

    #[test]
    fn infers_group_candidate_without_scale() {
        assert_eq!(
            infer_dimensions("parser/simd"),
            ("parser".to_owned(), "simd".to_owned(), "default".to_owned())
        );
    }

    #[test]
    fn keeps_simple_parameterized_benchmark_visible() {
        assert_eq!(
            infer_dimensions("fib/20"),
            ("fib".to_owned(), "default".to_owned(), "20".to_owned())
        );
    }
}
