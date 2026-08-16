import math
import unittest

from analyze_samples import (
    cross_pair_log_improvements,
    probability_of_superiority,
)
from plot_results import HTML_TEMPLATE, bradley_terry_scores, elo_rating, operation_parts


class EmpiricalComparisonTests(unittest.TestCase):
    def test_cross_pair_effects_use_log_reference_over_candidate(self) -> None:
        effects = cross_pair_log_improvements([2, 4], [1, 2])
        self.assertEqual(len(effects), 4)
        self.assertAlmostEqual(effects[0], math.log(2))
        self.assertAlmostEqual(effects[-1], math.log(2))
        self.assertAlmostEqual(cross_pair_log_improvements([1], [2])[0], -math.log(2))

    def test_probability_of_superiority_counts_ties_as_half(self) -> None:
        self.assertEqual(probability_of_superiority([2, 4], [1, 3]), 0.75)
        self.assertEqual(probability_of_superiority([1, 2], [1, 2]), 0.5)

    def test_candidate_operation_joins_its_family(self) -> None:
        self.assertEqual(
            operation_parts("Tail probability (batch-balanced)"),
            ("Tail probability", "batch-balanced"),
        )

    def test_unqualified_legacy_tail_is_labeled_with_its_implementation(self) -> None:
        self.assertEqual(
            operation_parts("Tail probability"),
            ("Tail probability", "online-balanced"),
        )

    def test_bradley_terry_scores_order_candidates_and_normalize_scale(self) -> None:
        scores = bradley_terry_scores(
            {
                "fast": {"latency": [1.0, 1.1]},
                "middle": {"latency": [2.0, 2.1]},
                "slow": {"latency": [3.0, 3.1]},
            }
        )
        self.assertGreater(scores["fast"], scores["middle"])
        self.assertGreater(scores["middle"], scores["slow"])
        geometric_mean = math.prod(scores.values()) ** (1 / len(scores))
        self.assertAlmostEqual(geometric_mean, 1.0)

    def test_bradley_terry_ties_have_equal_finite_scores(self) -> None:
        scores = bradley_terry_scores(
            {
                "a": {"latency": [1.0, 2.0]},
                "b": {"latency": [1.0, 2.0]},
            }
        )
        self.assertEqual(scores, {"a": 1.0, "b": 1.0})

    def test_elo_uses_the_conventional_log_strength_scale(self) -> None:
        self.assertEqual(elo_rating(1.0), 0.0)
        self.assertAlmostEqual(elo_rating(10.0), 400.0)
        self.assertAlmostEqual(elo_rating(0.1), -400.0)

    def test_pairwise_precedes_scale_curves_without_duplicate_cdfs(self) -> None:
        self.assertLess(
            HTML_TEMPLATE.index("Pairwise benchmark improvement"),
            HTML_TEMPLATE.index("All-run scaling across entries"),
        )
        self.assertNotIn("All-run empirical distributions", HTML_TEMPLATE)
        self.assertNotIn('id="distribution-sections"', HTML_TEMPLATE)
        for plot_id in ("scaling-throughput", "scaling-latency", "scaling-elo"):
            self.assertIn(f'id="{plot_id}"', HTML_TEMPLATE)


if __name__ == "__main__":
    unittest.main()
