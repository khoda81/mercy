use mercy::{
    prefix::implementations::{
        self, batch_balanced, online_balanced, scalar, ALL_IMPLEMENTATIONS, DEFAULT,
        PERFORMANCE_CANDIDATES,
    },
    BigDyadic, RankedPrefix,
};

#[test]
fn prefix_tail_is_exact() {
    // (1 - 1/2) * (1 - 1/4) = 3/8
    let tail = RankedPrefix::from_slice(&[128, 64]).tail_probability();
    assert_eq!(tail.numerator(), 3u8.into());
    assert_eq!(tail.fractional_bits(), 3);
}

#[test]
fn truncation_is_the_only_explicit_certainty() {
    assert_eq!(
        RankedPrefix::from_slice(&[]).tail_probability(),
        BigDyadic::one()
    );
    assert!(RankedPrefix::from_slice(&[255]).tail_probability() < BigDyadic::one());
}

#[test]
fn tail_candidates_are_public_and_default_to_the_measured_winner() {
    let prefix = RankedPrefix::from_slice(&[1, 64, 127, 128, 191, 254, 255]);
    let expected = scalar::compute(prefix);

    assert_eq!(online_balanced::compute(prefix), expected);
    assert_eq!(batch_balanced::compute(prefix), expected);
    assert_eq!(implementations::compute(prefix), expected);
    assert_eq!(DEFAULT.name, "online-balanced");
    assert_eq!(PERFORMANCE_CANDIDATES.len(), 2);
    assert_eq!(ALL_IMPLEMENTATIONS.len(), 3);
}
