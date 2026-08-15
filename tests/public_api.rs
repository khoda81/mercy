use mercy::{BigDyadic, RankedPrefix};

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
