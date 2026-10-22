//! Port of `src/FresnelIntegral.cpp`.

/// Approximation to ideal knife edge diffraction loss, A(v, 0), in dB.
/// `v2` is v^2.
#[inline]
pub(crate) fn fresnel_integral(v2: f64) -> f64 {
    // Note: v2 is v^2, so 5.76 is actually comparing v to 2.4
    if v2 < 5.76 {
        6.02 + 9.11 * v2.sqrt() - 1.27 * v2 // [TN101v2, Eqn III.24b] and [ERL 79-ITS 67, Eqn 3.27a & 3.27b]
    } else {
        12.953 + 10.0 * v2.log10() // [TN101v2, Eqn III.24c] and [ERL 79-ITS 67, Eqn 3.27a & 3.27b]
    }
}
