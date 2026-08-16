#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PairwiseSummary {
    pub superiority: f64,
    pub regression: f64,
    pub ties: f64,
    pub median_log_improvement: f64,
    pub median_ratio: f64,
}

pub fn pairwise(reference: &[f64], candidate: &[f64]) -> PairwiseSummary {
    if reference.is_empty() || candidate.is_empty() {
        return PairwiseSummary {
            superiority: f64::NAN,
            regression: f64::NAN,
            ties: f64::NAN,
            median_log_improvement: f64::NAN,
            median_ratio: f64::NAN,
        };
    }

    let mut better = 0usize;
    let mut worse = 0usize;
    let mut ties = 0usize;
    let mut effects = Vec::with_capacity(reference.len() * candidate.len());

    for &candidate_value in candidate {
        for &reference_value in reference {
            if candidate_value < reference_value {
                better += 1;
            } else if candidate_value > reference_value {
                worse += 1;
            } else {
                ties += 1;
            }
            effects.push((reference_value / candidate_value).ln());
        }
    }

    effects.sort_by(f64::total_cmp);
    let median = median_sorted(&effects);
    let total = (reference.len() * candidate.len()) as f64;

    PairwiseSummary {
        superiority: (better as f64 + ties as f64 * 0.5) / total,
        regression: (worse as f64 + ties as f64 * 0.5) / total,
        ties: ties as f64 / total,
        median_log_improvement: median,
        median_ratio: median.exp(),
    }
}

fn median_sorted(values: &[f64]) -> f64 {
    let middle = values.len() / 2;
    if values.len() % 2 == 0 {
        (values[middle - 1] + values[middle]) / 2.0
    } else {
        values[middle]
    }
}

#[cfg(test)]
mod tests {
    use super::pairwise;

    #[test]
    fn superiority_counts_cross_pairs_and_half_ties() {
        let p = pairwise(&[2.0, 4.0], &[1.0, 3.0]);
        assert!((p.superiority - 0.75).abs() < 1e-12);
        assert!((p.regression - 0.25).abs() < 1e-12);
        assert_eq!(p.ties, 0.0);
    }

    #[test]
    fn ratio_is_reference_over_candidate() {
        let p = pairwise(&[2.0], &[1.0]);
        assert!((p.median_ratio - 2.0).abs() < 1e-12);
        assert!(p.median_log_improvement > 0.0);
    }
}
