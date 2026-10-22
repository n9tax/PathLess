//! Port of `src/SigmaHFunction.cpp`.

/// Compute sigma_h, the RMS deviation of terrain and terrain clutter within
/// the limits of the first Fresnel zone in the dominant reflecting plane.
#[inline]
pub(crate) fn sigma_h_function(delta_h_m: f64) -> f64 {
    // [ERL 79-ITS 67, Eqn 3.6a]
    0.78 * delta_h_m * (-0.5 * delta_h_m.powf(0.25)).exp()
}
