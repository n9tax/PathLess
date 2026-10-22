//! Port of `src/QuickPfl.cpp`.

use crate::common::{fdim, max, min, Pfl};
use crate::compute_delta_h::compute_delta_h;
use crate::find_horizons::find_horizons;
use crate::linear_least_squares_fit::linear_least_squares_fit;

/// Parameters extracted from the terrain profile by `quick_pfl`.
#[derive(Debug, Clone, Copy)]
pub(crate) struct QuickPflOut {
    pub theta_hzn: [f64; 2],
    pub d_hzn_m: [f64; 2],
    pub h_e_m: [f64; 2],
    pub delta_h_m: f64,
    pub d_m: f64,
}

/// Extract parameters from the terrain profile.
pub(crate) fn quick_pfl(pfl: Pfl<'_>, gamma_e: f64, h_m: &[f64; 2]) -> QuickPflOut {
    let z = pfl.z;
    let d_m = pfl.np_f() * pfl.xi;
    let np = pfl.np;

    let a_e_m = 1.0 / gamma_e; // effective earth radius

    let (mut theta_hzn, mut d_hzn_m) = find_horizons(pfl, a_e_m, h_m);
    let mut h_e_m = [0.0f64; 2];

    // "In our own work we have sometimes said that consideration of terrain elevations should begin at a point about 15 times the tower height"
    //      - [Hufford, 1982] Page 25
    let d_start_m = min(15.0 * h_m[0], 0.1 * d_hzn_m[0]); // take lesser: 10% of horizon distance or 15x terminal height
    let d_end_m = d_m - min(15.0 * h_m[1], 0.1 * d_hzn_m[1]); // << ditto, but measured from the far end of the link >>

    let delta_h_m = compute_delta_h(pfl, d_start_m, d_end_m);

    if d_hzn_m[0] + d_hzn_m[1] > 1.5 * d_m {
        // The combined horizon distance is at least 50% larger than the total path distance
        //  -> so we are well within the line-of-sight range

        let (fit_tx, fit_rx) = linear_least_squares_fit(pfl, d_start_m, d_end_m);

        h_e_m[0] = h_m[0] + fdim(z[0], fit_tx);
        h_e_m[1] = h_m[1] + fdim(z[np], fit_rx);

        for i in 0..2 {
            d_hzn_m[i] = (2.0 * h_e_m[i] * a_e_m).sqrt() * (-0.07 * (delta_h_m / max(h_e_m[i], 5.0)).sqrt()).exp();
        }

        let combined_horizons_m = d_hzn_m[0] + d_hzn_m[1];
        if combined_horizons_m <= d_m {
            let q = (d_m / combined_horizons_m).powi(2);

            for i in 0..2 {
                h_e_m[i] *= q;
                d_hzn_m[i] = (2.0 * h_e_m[i] * a_e_m).sqrt() * (-0.07 * (delta_h_m / max(h_e_m[i], 5.0)).sqrt()).exp();
            }
        }

        for i in 0..2 {
            let q = (2.0 * h_e_m[i] * a_e_m).sqrt();
            theta_hzn[i] = (0.65 * delta_h_m * (q / d_hzn_m[i] - 1.0) - 2.0 * h_e_m[i]) / q;
        }
    } else {
        let (fit_tx, _) = linear_least_squares_fit(pfl, d_start_m, 0.9 * d_hzn_m[0]);
        h_e_m[0] = h_m[0] + fdim(z[0], fit_tx);

        let (_, fit_rx) = linear_least_squares_fit(pfl, d_m - 0.9 * d_hzn_m[1], d_end_m);
        h_e_m[1] = h_m[1] + fdim(z[np], fit_rx);
    }

    QuickPflOut { theta_hzn, d_hzn_m, h_e_m, delta_h_m, d_m }
}
