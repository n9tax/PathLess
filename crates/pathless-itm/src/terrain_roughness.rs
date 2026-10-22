//! Port of `src/TerrainRoughness.cpp`.

/// Compute delta_h_d, the terrain irregularity of a path of length `d_m`.
#[inline]
pub(crate) fn terrain_roughness(d_m: f64, delta_h_m: f64) -> f64 {
    // [ERL 79-ITS 67, Eqn 3], with distance in meters instead of kilometers
    delta_h_m * (1.0 - 0.8 * (-d_m / 50e3).exp())
}
