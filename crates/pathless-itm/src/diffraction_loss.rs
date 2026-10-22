//! Port of `src/DiffractionLoss.cpp`.

use crate::common::{min, ItmMode};
use crate::complex::Complex;
use crate::knife_edge_diffraction::knife_edge_diffraction;
use crate::sigma_h_function::sigma_h_function;
use crate::smooth_earth_diffraction::smooth_earth_diffraction;
use crate::terrain_roughness::terrain_roughness;

/// Diffraction loss at distance `d_m`, in dB.
#[allow(clippy::too_many_arguments)]
pub(crate) fn diffraction_loss(
    d_m: f64,
    d_hzn_m: &[f64; 2],
    h_e_m: &[f64; 2],
    z_g: Complex,
    a_e_m: f64,
    delta_h_m: f64,
    h_m: &[f64; 2],
    mode: ItmMode,
    theta_los: f64,
    d_sml_m: f64,
    f_mhz: f64,
) -> f64 {
    let a_k_db = knife_edge_diffraction(d_m, f_mhz, a_e_m, theta_los, d_hzn_m);

    let a_se_db = smooth_earth_diffraction(d_m, f_mhz, a_e_m, theta_los, d_hzn_m, h_e_m, z_g);

    //////////////////
    // Terrain clutter

    // Terrain roughness term, using d_sML, per [ERL 79-ITS 67, page 3-13]
    let delta_h_dsml_m = terrain_roughness(d_sml_m, delta_h_m);

    let sigma_h_d_m = sigma_h_function(delta_h_dsml_m);

    // Clutter factor [ERL 79-ITS 67, Eqn 3.38c]
    let a_fo_db = min(15.0, 5.0 * (1.0 + 1e-5 * h_m[0] * h_m[1] * f_mhz * sigma_h_d_m).log10());

    //////////////////////////////
    // Combined diffraction losses

    let delta_h_d_m = terrain_roughness(d_m, delta_h_m);

    let mut q = h_m[0] * h_m[1];
    let qk = h_e_m[0] * h_e_m[1] - q;

    // For low antennas with known path parameters, C ~= 10 [ERL 79-ITS 67, page 3-8]
    if mode == ItmMode::P2p {
        q += 10.0;
    }

    let term1 = (1.0 + qk / q).sqrt(); // square root term in [ERL 79-ITS 67, Eqn 3.23]

    let d_ml_m = d_hzn_m[0] + d_hzn_m[1]; // Maximum line-of-sight distance for actual path
    q = (term1 + (-theta_los * a_e_m + d_ml_m) / d_m) * min(delta_h_d_m * f_mhz / 47.7, 6283.2);

    // weighting factor [ERL 79-ITS 67, Eqn 3.23]
    let w = 25.1 / (25.1 + q.sqrt());

    w * a_se_db + (1.0 - w) * a_k_db + a_fo_db
}
