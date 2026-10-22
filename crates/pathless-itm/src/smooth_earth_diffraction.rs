//! Port of `src/SmoothEarthDiffraction.cpp`.

use crate::common::{A_0_METER, THIRD};
use crate::complex::Complex;

/// Smooth earth diffraction loss using the Vogler 3-radii method, in dB.
pub(crate) fn smooth_earth_diffraction(
    d_m: f64,
    f_mhz: f64,
    a_e_m: f64,
    theta_los: f64,
    d_hzn_m: &[f64; 2],
    h_e_m: &[f64; 2],
    z_g: Complex,
) -> f64 {
    let mut a_m = [0.0f64; 3];
    let mut d_km = [0.0f64; 3];
    let mut f_x_db = [0.0f64; 2];
    let mut k = [0.0f64; 3];
    let mut b_0 = [0.0f64; 3];
    let mut x_km = [0.0f64; 3];
    let mut c_0 = [0.0f64; 3];

    let theta_nlos = d_m / a_e_m - theta_los; // [Algorithm, Eqn 4.12]
    let d_ml_m = d_hzn_m[0] + d_hzn_m[1]; // Maximum line-of-sight distance for actual path

    // compute 3 radii
    a_m[0] = (d_m - d_ml_m) / (d_m / a_e_m - theta_los); // which is a_e when theta_los = d_ML / a_e
    a_m[1] = 0.5 * d_hzn_m[0].powi(2) / h_e_m[0]; // [Vogler 1964, Eqn 3] re-arranged
    a_m[2] = 0.5 * d_hzn_m[1].powi(2) / h_e_m[1]; // [Vogler 1964, Eqn 3] re-arranged

    d_km[0] = (a_m[0] * theta_nlos) / 1000.0; // angular distance of the "diffraction path"
    d_km[1] = d_hzn_m[0] / 1000.0;
    d_km[2] = d_hzn_m[1] / 1000.0;

    let abs_z_g = z_g.abs();
    for i in 0..3 {
        // C_0 = (4 / 3k) ^ (1 / 3) [Vogler 1964, Eqn 2]
        c_0[i] = ((4.0 / 3.0) * A_0_METER / a_m[i]).powf(THIRD);

        // [Vogler 1964, Eqn 6a / 7a]
        k[i] = 0.017778 * c_0[i] * f_mhz.powf(-THIRD) / abs_z_g;

        // compute B_0 for each radius [Vogler 1964, Fig 4]
        b_0[i] = 1.607 - k[i];
    }

    // compute x__km for each radius [Vogler 1964, Eqn 2]
    x_km[1] = b_0[1] * c_0[1].powi(2) * f_mhz.powf(THIRD) * d_km[1];
    x_km[2] = b_0[2] * c_0[2].powi(2) * f_mhz.powf(THIRD) * d_km[2];
    x_km[0] = b_0[0] * c_0[0].powi(2) * f_mhz.powf(THIRD) * d_km[0] + x_km[1] + x_km[2];

    // compute height gain functions
    f_x_db[0] = height_function(x_km[1], k[1]);
    f_x_db[1] = height_function(x_km[2], k[2]);

    // compute distance function
    let g_x_db = 0.05751 * x_km[0] - 10.0 * x_km[0].log10(); // [TN101, Eqn 8.4] & [Vogler 1964, Eqn 13]

    g_x_db - f_x_db[0] - f_x_db[1] - 20.0 // [Algorithm, Eqn 4.20] & [Vogler 1964]
}

/// Height Function, F(x, K) for smooth earth diffraction, in dB.
pub(crate) fn height_function(x_km: f64, k: f64) -> f64 {
    let w;
    let mut result;

    if x_km < 200.0 {
        w = -k.ln();

        if k < 1e-5 || x_km * w.powf(3.0) > 5495.0 {
            result = -117.0;

            if x_km > 1.0 {
                result += 17.372 * x_km.ln();
            }
        } else {
            result = 2.5e-5 * x_km.powi(2) / k - 8.686 * w - 15.0;
        }
    } else {
        result = 0.05751 * x_km - 4.343 * x_km.ln();

        if x_km < 2000.0 {
            w = 0.0134 * x_km * (-0.005 * x_km).exp();
            result = (1.0 - w) * result + w * (17.372 * x_km.ln() - 117.0);
        }
    }

    result
}
