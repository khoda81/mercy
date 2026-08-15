//! Reproducible benchmark workloads for ranked conditional probabilities.

/// A deterministic input family used by the Criterion matrix.
#[derive(Clone, Copy, Debug)]
pub struct Fixture {
    pub name: &'static str,
    generate: fn(usize) -> Vec<u8>,
}

impl Fixture {
    const fn new(name: &'static str, generate: fn(usize) -> Vec<u8>) -> Self {
        Self { name, generate }
    }

    pub fn generate(self, size: usize) -> Vec<u8> {
        (self.generate)(size)
    }
}

pub const PATTERNED: Fixture = Fixture::new("patterned", patterned);
pub const ONES: Fixture = Fixture::new("ones", ones);
pub const MODEL_FLAT: Fixture = Fixture::new("model-flat", model_flat);
pub const MODEL_PEAKED: Fixture = Fixture::new("model-peaked", model_peaked);
pub const MODEL_LONG_TAIL: Fixture = Fixture::new("model-long-tail", model_long_tail);

pub const GENERAL_FIXTURES: &[Fixture] = &[PATTERNED, ONES];
pub const MODEL_SHAPED_FIXTURES: &[Fixture] = &[MODEL_FLAT, MODEL_PEAKED, MODEL_LONG_TAIL];

fn patterned(size: usize) -> Vec<u8> {
    (0..size)
        .map(|i| ((i.wrapping_mul(73).wrapping_add(i >> 3).wrapping_add(19)) & 0xff) as u8)
        .collect()
}

fn ones(size: usize) -> Vec<u8> {
    vec![1; size]
}

fn ranked_hazards(weights: impl IntoIterator<Item = u64>) -> Vec<u8> {
    let weights: Vec<u64> = weights.into_iter().collect();
    let mut remaining: u128 = weights.iter().map(|&weight| u128::from(weight)).sum();
    weights
        .into_iter()
        .map(|weight| {
            let numerator = u128::from(weight) * 256;
            let raw = numerator.checked_div(remaining).unwrap_or(0).min(255) as u8;
            remaining -= u128::from(weight);
            raw
        })
        .collect()
}

fn model_flat(size: usize) -> Vec<u8> {
    ranked_hazards(std::iter::repeat_n(1, size))
}

fn model_peaked(size: usize) -> Vec<u8> {
    let weights = (0..size)
        .map(|index| {
            if index == 0 {
                size.max(1) as u64 * 32
            } else {
                1
            }
        })
        .collect::<Vec<_>>();
    ranked_hazards(weights)
}

fn model_long_tail(size: usize) -> Vec<u8> {
    ranked_hazards((0..size).map(|index| (size / (index + 1)).max(1) as u64))
}
