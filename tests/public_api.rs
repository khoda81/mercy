use mercy::{
    prefix::implementations::{
        self, batch_balanced, online_balanced, scalar, widening_u128, ALL_IMPLEMENTATIONS, DEFAULT,
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
    assert_eq!(widening_u128::compute(prefix), expected);
    assert_eq!(implementations::compute(prefix), expected);
    assert_eq!(DEFAULT.name, "online-balanced");
    assert_eq!(PERFORMANCE_CANDIDATES.len(), 3);
    assert_eq!(ALL_IMPLEMENTATIONS.len(), 4);
}

#[test]
fn owned_tail_candidates_retain_exact_semantics() {
    let bytes = vec![
        0, 1, 64, 127, 128, 192, 254, 255, 13, 29, 41, 73, 99, 111, 201, 239, 7,
    ];
    let expected = scalar::compute(RankedPrefix::from_slice(&bytes));
    for candidate in implementations::owned::CANDIDATES {
        let owned = RankedPrefix::from_boxed_slice(bytes.clone().into_boxed_slice());
        assert_eq!(candidate.compute(owned), expected, "{}", candidate.name);
    }
    let owned = RankedPrefix::from_boxed_slice(bytes.into_boxed_slice());
    assert_eq!(RankedPrefix::into_tail_probability(owned), expected);
}

#[test]
fn dyadic_layout_candidates_are_exact() {
    let left = RankedPrefix::from_slice(&[1, 64, 127, 255]).tail_probability();
    let right = RankedPrefix::from_slice(&[2, 65, 128, 254]).tail_probability();
    let expected_product = &left * &right;
    let expected_scale = left.scale_floor_u64(u64::MAX - 4095);
    for candidate in mercy::dyadic::implementations::CANDIDATES {
        let prepared_left = candidate.prepare(&left);
        let prepared_right = candidate.prepare(&right);
        assert_eq!(prepared_left.to_big_dyadic(), left, "{}", candidate.name);
        assert_eq!(
            candidate
                .multiply(&prepared_left, &prepared_right)
                .to_big_dyadic(),
            expected_product,
            "{}",
            candidate.name
        );
        assert_eq!(
            candidate.scale_floor_u64(&prepared_left, u64::MAX - 4095),
            expected_scale,
            "{}",
            candidate.name
        );
    }
    assert_eq!(
        mercy::dyadic::implementations::DEFAULT.name,
        "native-biguint"
    );
}
