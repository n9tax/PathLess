//! Port of `src/KnifeEdgeDiffraction.cpp`.

use crate::fresnel_integral::fresnel_integral;

/// Knife-edge diffraction loss, in dB.
pub(crate) fn knife_edge_diffraction(d_m: f64, f_mhz: f64, a_e_m: f64, theta_los: f64, d_hzn_m: &[f64; 2]) -> f64 {
    let d_ml_m = d_hzn_m[0] + d_hzn_m[1]; // Maximum line-of-sight distance for actual path
    let theta_nlos = d_m / a_e_m - theta_los; // Angular distance of diffraction region [Algorithm, Eqn 4.12]

    let d_nlos_m = d_m - d_ml_m; // Diffraction distance, in meters

    // 1 / (4 pi) = 0.0795775
    // [TN101, Eqn I.7]
    let v_1 = 0.0795775 * (f_mhz / 47.7) * theta_nlos.powi(2) * d_hzn_m[0] * d_nlos_m / (d_nlos_m + d_hzn_m[0]);
    let v_2 = 0.0795775 * (f_mhz / 47.7) * theta_nlos.powi(2) * d_hzn_m[1] * d_nlos_m / (d_nlos_m + d_hzn_m[1]);

    fresnel_integral(v_1) + fresnel_integral(v_2) // [TN101, Eqn I.1]
}
