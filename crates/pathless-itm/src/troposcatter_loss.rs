//! Port of `src/TroposcatterLoss.cpp`.

use crate::common::{max, min};
use crate::h0_function::h0_function;

/// The attenuation function, F(theta * d), in dB.
pub(crate) fn f_function(td: f64) -> f64 {
    // constants from [Algorithm, 6.9]
    const A: [f64; 3] = [133.4, 104.6, 71.8];
    const B: [f64; 3] = [0.332e-3, 0.212e-3, 0.157e-3];
    const C: [f64; 3] = [-10.0, -2.5, 5.0];

    // select the set of values to use
    let i = if td <= 10e3 {
        0 // <= 10 km
    } else if td <= 70e3 {
        1 // 10 km to 70 km
    } else {
        2 // > 70 km
    };

    A[i] + B[i] * td + C[i] * td.log10() // [Algorithm, 6.9]
}

/// Troposcatter loss, in dB. `h0` is an in/out parameter carrying the
/// H_0() value between the two calls made by `longley_rice` (initialise it
/// to -1).
#[allow(clippy::too_many_arguments)]
pub(crate) fn troposcatter_loss(
    d_m: f64,
    theta_hzn: &[f64; 2],
    d_hzn_m: &[f64; 2],
    h_e_m: &[f64; 2],
    a_e_m: f64,
    n_s: f64,
    f_mhz: f64,
    theta_los: f64,
    h0: &mut f64,
) -> f64 {
    let sqrt2 = 2.0f64.sqrt();
    let mut h_0;

    // wavenumber, k
    let wn = f_mhz / 47.7;

    if *h0 > 15.0 {
        // short-circuit calculations if already greater than 15 dB
        h_0 = *h0;
    } else {
        let mut ad = d_hzn_m[0] - d_hzn_m[1];
        let mut rr = h_e_m[1] / h_e_m[0];

        if ad < 0.0 {
            // ensure correct frame of reference
            ad = -ad;
            rr = 1.0 / rr;
        }

        let theta = theta_hzn[0] + theta_hzn[1] + d_m / a_e_m; // angular distance, in radians

        // [TN101, Eqn 9.4a]
        let r_1 = 2.0 * wn * theta * h_e_m[0];
        let r_2 = 2.0 * wn * theta * h_e_m[1];

        if r_1 < 0.2 && r_2 < 0.2 {
            // "If both r_1 and r_2 are less than 0.2 the function A_scat is not defined (or is infinite)" [Algorithm, page 11]
            return 1001.0;
        }

        let mut s = (d_m - ad) / (d_m + ad); // asymmetry parameter

        // "In all of this, we truncate the values of s and q at 0.1 and 10" [Algorithm, page 16]
        let q = min(max(0.1, rr / s), 10.0); // TN101, Eqn 9.5
        s = max(0.1, s); // TN101, Eqn 9.5

        let h_0_m = (d_m - ad) * (d_m + ad) * theta * 0.25 / d_m; // height of cross-over, [Algorithm, 4.66] [TN101v1, 9.3b]

        const Z_0_M: f64 = 1.7556e3; // Scale height, [Algorithm, 4.67]
        const Z_1_M: f64 = 8.0e3; // [Algorithm, 4.67]
                                  // Scattering efficiency factor, eta_s [TN101 Eqn 9.3a]
        let eta_s = (h_0_m / Z_0_M)
            * (1.0 + (0.031 - n_s * 2.32e-3 + n_s.powi(2) * 5.67e-6) * (-min(1.7, h_0_m / Z_1_M).powf(6.0)).exp());

        let h_00 = (h0_function(r_1, eta_s) + h0_function(r_2, eta_s)) / 2.0; // First term in TN101v1, Eqn 9.5
        let delta_h_0 = min(h_00, 6.0 * (0.6 - max(eta_s, 1.0).log10()) * s.log10() * q.log10());

        h_0 = h_00 + delta_h_0; // TN101, Eqn 9.5
        h_0 = max(h_0, 0.0); // "If Delta_H_0 would make H_0 negative, use H_0 = 0" [TN101v1, p9.4]

        if eta_s < 1.0 {
            // if <=1, interpolate with the special case of eta_s = 0
            h_0 = eta_s * h_0
                + (1.0 - eta_s)
                    * 10.0
                    * (((1.0 + sqrt2 / r_1) * (1.0 + sqrt2 / r_2)).powi(2) * (r_1 + r_2) / (r_1 + r_2 + 2.0 * sqrt2))
                        .log10();
        }

        // "If, at d_5, calculations show that H_0 will exceed 15 dB, they are replaced by the value it has at d_6" [Algorithm, page 12]
        if h_0 > 15.0 && *h0 >= 0.0 {
            h_0 = *h0;
        }
    }

    *h0 = h_0;
    let th = d_m / a_e_m - theta_los;

    const D_0_M: f64 = 40e3; // [Algorithm, 6.8]
    const H_M: f64 = 47.7; // [Algorithm, 4.63]
                           // [Algorithm, 4.63]
    f_function(th * d_m) + 10.0 * (wn * H_M * th.powf(4.0)).log10() - 0.1 * (n_s - 301.0) * (-th * d_m / D_0_M).exp()
        + h_0
}
