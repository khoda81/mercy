use crate::RankedPrefix;

/// A path-relative arithmetic coder over ranked-prefix probability models.
///
/// The trait intentionally promises much less than an algebra over probability
/// distributions. A coder may round boundaries, choose internal
/// representatives, delay output, or otherwise exploit implementation-specific
/// state. Correctness is defined only along the *exact path of questions and
/// zooms* used to construct that state.
///
/// # Query model
///
/// A [`RankedPrefix`] of length `N` describes `N` explicit events followed by
/// one implicit tail event, so [`Self::locate`] returns a rank in `0..=N`.
///
/// # Zoom model
///
/// [`Self::zoom`] receives two ranked prefixes:
///
/// ```text
/// denied:   [deny, deny, deny, ...]
/// accepted: [accept, accept, accept, ...]
/// remainder after accepted: irrelevant
/// ```
///
/// If `D` is the tail probability of `denied` and `A` is the tail probability
/// of `accepted`, then the selected probability interval has mass
///
/// ```text
/// D * (1 - A)
/// ```
///
/// and CDF boundaries
///
/// ```text
/// lower = 1 - D
/// upper = 1 - D * A.
/// ```
///
/// Nothing after `accepted` is required to compute either boundary. This is why
/// intervals are represented by two prefixes rather than by an index and size.
///
/// # Required consistency
///
/// For an unchanged coder state, repeating [`Self::locate`] with byte-identical
/// input must return the same rank.
///
/// Replaying the same initial coder state and the same sequence of
/// byte-identical [`Self::zoom`] calls must reproduce the same observable
/// behavior: emitted bytes and later `locate` results must be deterministic.
///
/// After `zoom(denied, accepted)`, consider the finite ranked prefix formed by
/// the exact bytes of `denied` followed immediately by the exact bytes of
/// `accepted`. Locating the resulting coder under that exact finite model must
/// select one of the explicit `accepted` ranks:
///
/// ```text
/// denied.len() <= rank < denied.len() + accepted.len()
/// ```
///
/// Zero-probability explicit events may of course never be selected.
///
/// # Deliberate non-guarantees
///
/// No behavior is specified when the probability model is changed instead of
/// replayed exactly, even if the caller believes the replacement distribution
/// is mathematically equivalent.
///
/// In particular, implementations are **not** required to make any of the
/// following transformations equivalent:
///
/// - merging or splitting ranked events;
/// - replacing a prefix by another prefix with the same aggregate mass;
/// - collapsing nested `zoom` operations into one larger `zoom`;
/// - expanding one `zoom` into several nested `zoom` operations;
/// - "un-zooming" or otherwise recovering an ancestor coder state;
/// - changing probability precision or reparameterizing the same semantic
///   event set.
///
/// This lack of compositionality is intentional. It prevents callers from
/// depending on arithmetic details that optimized coders may legitimately
/// change.
///
/// # Preconditions for `zoom`
///
/// The accepted explicit interval must have nonzero probability. Equivalently,
/// `accepted` must contain at least one nonzero byte. The empty prefix and a
/// prefix containing only zero probabilities describe an empty accepted
/// interval and must not be passed to `zoom`.
pub trait Coder {
    /// Bytes irrevocably settled by one [`Self::zoom`] operation.
    ///
    /// The returned iterator owns whatever data it needs; consuming it must not
    /// further mutate the coder.
    type Output: Iterator<Item = u8>;

    /// Locate the current coded value in a finite ranked-prefix model.
    ///
    /// For a prefix of length `N`, the result is always in `0..=N`; rank `N`
    /// denotes the implicit tail event.
    ///
    /// This operation is observational and does not advance or restrict the
    /// coder.
    fn locate(&self, prefix: &RankedPrefix) -> usize;

    /// Restrict the coder to the interval described by `denied` and `accepted`.
    ///
    /// The coder first excludes every explicit event represented by `denied`,
    /// then keeps the union of the explicit events represented by `accepted`.
    /// The probability model after the accepted prefix is deliberately absent:
    /// it cannot affect the selected interval boundaries.
    ///
    /// Implementations may emit any bytes whose values become irrevocably fixed
    /// by this restriction.
    fn zoom(&mut self, denied: &RankedPrefix, accepted: &RankedPrefix) -> Self::Output;
}
