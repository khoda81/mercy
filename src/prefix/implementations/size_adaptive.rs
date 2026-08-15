//! Size-directed composition of the strongest durable tail candidates.

use super::{batch_balanced, online_balanced, widening_u128};
use crate::{BigDyadic, RankedPrefix};

/// Compute the exact tail with the candidate selected for this prefix length.
pub fn compute(prefix: &RankedPrefix) -> BigDyadic {
    match prefix.len() {
        0..=8 => batch_balanced::compute(prefix),
        9..=64 => online_balanced::compute(prefix),
        65..=256 => widening_u128::compute(prefix),
        257..=65_535 => online_balanced::compute(prefix),
        65_536..=131_071 => batch_balanced::compute(prefix),
        _ => online_balanced::compute(prefix),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::prefix::implementations::scalar;

    #[test]
    fn transition_boundaries_remain_exact() {
        for size in [0, 8, 9, 64, 65, 256, 257, 65_535, 65_536, 131_071, 131_072] {
            let bytes = (0..size)
                .map(|index| ((index * 73 + 19) & 0xff) as u8)
                .collect::<Vec<_>>();
            let prefix = RankedPrefix::from_slice(&bytes);
            assert_eq!(compute(prefix), scalar::compute(prefix), "size {size}");
        }
    }
}
