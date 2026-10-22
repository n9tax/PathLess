//! Port of `src/InitializeArea.cpp`.

use crate::common::{max, min, PI};
use crate::enums::SitingCriteria;

/// Initialize area mode calculations.
///
/// Returns `(h_e_m, d_hzn_m, theta_hzn)`.
pub(crate) fn initialize_area(
    site_criteria: [SitingCriteria; 2],
    gamma_e: f64,
    delta_h_m: f64,
    h_m: &[f64; 2],
) -> ([f64; 2], [f64; 2], [f64; 2]) {
    let mut h_e_m = [0.0f64; 2];
    let mut d_hzn_m = [0.0f64; 2];
    let mut theta_hzn = [0.0f64; 2];

    for i in 0..2 {
        if site_criteria[i] == SitingCriteria::Random {
            h_e_m[i] = h_m[i];
        } else {
            let mut b = if site_criteria[i] == SitingCriteria::Careful {
                4.0
            } else {
                // SITING_CRITERIA__VERY_CAREFUL
                9.0
            };

            if h_m[i] < 5.0 {
                b *= (0.1 * PI * h_m[i]).sin();
            }

            // [Algorithm, Eqn 3.2]
            h_e_m[i] = h_m[i] + (1.0 + b) * (-min(20.0, 2.0 * h_m[i] / max(1e-3, delta_h_m))).exp();
        }

        let d_ls_m = (2.0 * h_e_m[i] / gamma_e).sqrt();

        // [Algorithm, Eqn 3.3]
        const H_3_M: f64 = 5.0;
        d_hzn_m[i] = d_ls_m * (-0.07 * (delta_h_m / max(h_e_m[i], H_3_M)).sqrt()).exp();

        // [Algorithm, Eqn 3.4]
        theta_hzn[i] = (0.65 * delta_h_m * (d_ls_m / d_hzn_m[i] - 1.0) - 2.0 * h_e_m[i]) / d_ls_m;
    }

    (h_e_m, d_hzn_m, theta_hzn)
}
