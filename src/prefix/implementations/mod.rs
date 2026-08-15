//! Exact tail-probability implementations and benchmark candidates.
//!
//! These algorithms are nested under [`crate::prefix`] because they implement
//! an operation on [`RankedPrefix`]. Benchmark harnesses contain only workload
//! construction and timing; production and benchmarks call the same code.

use crate::{BigDyadic, RankedPrefix};

pub mod batch_balanced;
pub mod online_balanced;
pub mod scalar;

/// Production implementation selected by the durable benchmark matrix.
pub use online_balanced::compute;

/// One exact implementation selectable by benchmark logic.
#[derive(Clone, Copy, Debug)]
pub struct Candidate {
    /// Stable name used in benchmark identifiers and reports.
    pub name: &'static str,
    implementation: fn(&RankedPrefix) -> BigDyadic,
}

impl Candidate {
    const fn new(name: &'static str, implementation: fn(&RankedPrefix) -> BigDyadic) -> Self {
        Self {
            name,
            implementation,
        }
    }

    /// Compute the exact tail with this implementation.
    #[inline]
    pub fn compute(self, prefix: &RankedPrefix) -> BigDyadic {
        (self.implementation)(prefix)
    }
}

pub const ONLINE_BALANCED: Candidate = Candidate::new("online-balanced", online_balanced::compute);
pub const BATCH_BALANCED: Candidate = Candidate::new("batch-balanced", batch_balanced::compute);
pub const SCALAR_REFERENCE: Candidate = Candidate::new("scalar-reference", scalar::compute);

pub const PERFORMANCE_CANDIDATES: &[Candidate] = &[ONLINE_BALANCED, BATCH_BALANCED];
pub const ALL_IMPLEMENTATIONS: &[Candidate] = &[ONLINE_BALANCED, BATCH_BALANCED, SCALAR_REFERENCE];

/// Descriptor for the candidate used by [`RankedPrefix::tail_probability`].
pub const DEFAULT: Candidate = ONLINE_BALANCED;
