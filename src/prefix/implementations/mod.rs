//! Exact tail-probability implementations and benchmark candidates.
//!
//! These algorithms are nested under [`crate::prefix`] because they implement
//! an operation on [`RankedPrefix`]. Benchmark harnesses contain only workload
//! construction and timing; production and benchmarks call the same code.

use crate::{BigDyadic, RankedPrefix};

pub mod batch_balanced;
mod histogram;
pub mod histogram_powers;
pub mod histogram_primes;
pub mod online_balanced;
pub mod owned;
pub mod scalar;
pub mod size_adaptive;
pub mod widening_u128;

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
pub const WIDENING_U128: Candidate = Candidate::new("widening-u128", widening_u128::compute);
pub const HISTOGRAM_POWERS: Candidate =
    Candidate::new("histogram-powers", histogram_powers::compute);
pub const HISTOGRAM_PRIMES: Candidate =
    Candidate::new("histogram-primes", histogram_primes::compute);
pub const SIZE_ADAPTIVE: Candidate = Candidate::new("size-adaptive", size_adaptive::compute);
pub const SCALAR_REFERENCE: Candidate = Candidate::new("scalar-reference", scalar::compute);

pub const PERFORMANCE_CANDIDATES: &[Candidate] = &[
    ONLINE_BALANCED,
    BATCH_BALANCED,
    WIDENING_U128,
    HISTOGRAM_POWERS,
    HISTOGRAM_PRIMES,
    SIZE_ADAPTIVE,
];
pub const ALL_IMPLEMENTATIONS: &[Candidate] = &[
    ONLINE_BALANCED,
    BATCH_BALANCED,
    WIDENING_U128,
    HISTOGRAM_POWERS,
    HISTOGRAM_PRIMES,
    SIZE_ADAPTIVE,
    SCALAR_REFERENCE,
];

/// Descriptor for the candidate used by [`RankedPrefix::tail_probability`].
pub const DEFAULT: Candidate = ONLINE_BALANCED;
