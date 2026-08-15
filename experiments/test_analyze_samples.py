import math
import unittest

from analyze_samples import (
    cross_pair_log_improvements,
    probability_of_superiority,
)
from plot_results import operation_parts


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

    def test_default_tail_is_labeled_with_its_implementation(self) -> None:
        self.assertEqual(
            operation_parts("Tail probability"),
            ("Tail probability", "online-balanced"),
        )


if __name__ == "__main__":
    unittest.main()
