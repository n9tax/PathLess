//! Port of `src/Variability.cpp`.

use crate::common::{A_9000_METER, THIRD};
use crate::inverse_ccdf::inverse_ccdf;
use crate::terrain_roughness::terrain_roughness;
use crate::warnings::EXTREME_VARIABILITIES;

const SINGLE_MESSAGE_MODE: i32 = 0;
const ACCIDENTAL_MODE: i32 = 1;
const MOBILE_MODE: i32 = 2;

/// Curve helper function for TN101v2 Eqn III.69 & III.70, in dB.
#[inline]
fn curve(c1: f64, c2: f64, x1: f64, x2: f64, x3: f64, d_e_m: f64) -> f64 {
    (c1 + c2 / (1.0 + ((d_e_m - x2) / x3).powi(2))) * ((d_e_m / x1).powi(2)) / (1.0 + ((d_e_m / x1).powi(2)))
}

// Asymptotic values from TN101, Fig 10.13
// -> approximate to TN101v2 Eqn III.69 & III.70
// -> to describe the curves for each climate
const ALL_YEAR: [[f64; 7]; 5] = [
    [-9.67, -0.62, 1.26, -9.21, -0.62, -0.39, 3.15],
    [12.7, 9.19, 15.5, 9.05, 9.19, 2.86, 857.9],
    [144.9e3, 228.9e3, 262.6e3, 84.1e3, 228.9e3, 141.7e3, 2222.0e3],
    [190.3e3, 205.2e3, 185.2e3, 101.1e3, 205.2e3, 315.9e3, 164.8e3],
    [133.8e3, 143.6e3, 99.8e3, 98.6e3, 143.6e3, 167.4e3, 116.3e3],
];

const BSM1: [f64; 7] = [2.13, 2.66, 6.11, 1.98, 2.68, 6.86, 8.51];
const BSM2: [f64; 7] = [159.5, 7.67, 6.65, 13.11, 7.16, 10.38, 169.8];
const XSM1: [f64; 7] = [762.2e3, 100.4e3, 138.2e3, 139.1e3, 93.7e3, 187.8e3, 609.8e3];
const XSM2: [f64; 7] = [123.6e3, 172.5e3, 242.2e3, 132.7e3, 186.8e3, 169.6e3, 119.9e3];
const XSM3: [f64; 7] = [94.5e3, 136.4e3, 178.6e3, 193.5e3, 133.5e3, 108.9e3, 106.6e3];

const BSP1: [f64; 7] = [2.11, 6.87, 10.08, 3.68, 4.75, 8.58, 8.43];
const BSP2: [f64; 7] = [102.3, 15.53, 9.60, 159.3, 8.12, 13.97, 8.19];
const XSP1: [f64; 7] = [636.9e3, 138.7e3, 165.3e3, 464.4e3, 93.2e3, 216.0e3, 136.2e3];
const XSP2: [f64; 7] = [134.8e3, 143.7e3, 225.7e3, 93.1e3, 135.9e3, 152.0e3, 188.5e3];
const XSP3: [f64; 7] = [95.6e3, 98.6e3, 129.7e3, 94.2e3, 113.4e3, 122.7e3, 122.9e3];

const C_D: [f64; 7] = [1.224, 0.801, 1.380, 1.000, 1.224, 1.518, 1.518]; // [Algorithm, Table 5.1], C_d
const Z_D: [f64; 7] = [1.282, 2.161, 1.282, 20.0, 1.282, 1.282, 1.282]; // [Algorithm, Table 5.1], z_d

const BFM1: [f64; 7] = [1.0, 1.0, 1.0, 1.0, 0.92, 1.0, 1.0];
const BFM2: [f64; 7] = [0.0, 0.0, 0.0, 0.0, 0.25, 0.0, 0.0];
const BFM3: [f64; 7] = [0.0, 0.0, 0.0, 0.0, 1.77, 0.0, 0.0];

const BFP1: [f64; 7] = [1.0, 0.93, 1.0, 0.93, 0.93, 1.0, 1.0];
const BFP2: [f64; 7] = [0.0, 0.31, 0.0, 0.19, 0.31, 0.0, 0.0];
const BFP3: [f64; 7] = [0.0, 2.00, 0.0, 1.79, 2.00, 0.0, 0.0];

/// Compute the variability loss, in dB (to be added to free space loss).
///
/// `time`, `location`, `situation` are percentages (0 < x < 100);
/// `climate` is the raw NTIA value 1..=7 (already validated).
#[allow(clippy::too_many_arguments)]
pub(crate) fn variability(
    time: f64,
    location: f64,
    situation: f64,
    h_e_m: &[f64; 2],
    delta_h_m: f64,
    f_mhz: f64,
    d_m: f64,
    a_ref_db: f64,
    climate: i32,
    mdvar: i32,
    warnings: &mut u32,
) -> f64 {
    let mut z_t = inverse_ccdf(time / 100.0);
    let mut z_l = inverse_ccdf(location / 100.0);
    let z_s = inverse_ccdf(situation / 100.0);

    let ci = (climate - 1) as usize; // 0-based indexes

    let wn = f_mhz / 47.7;

    // compute the effective distance
    let d_ex_m =
        (2.0 * A_9000_METER * h_e_m[0]).sqrt() + (2.0 * A_9000_METER * h_e_m[1]).sqrt() + (575.7e12 / wn).powf(THIRD); // [Algorithm, Eqn 5.3]

    let d_e_m = if d_m < d_ex_m { 130e3 * d_m / d_ex_m } else { 130e3 + d_m - d_ex_m };

    //////////////////////////////////
    // situation variability calcs

    // if mdvar >= 20, then "Direct situation variability is to be eliminated as it should when
    //                       considering interference problems.  Note that there may still be a
    //                       small residual situation variability" [Hufford, 1982]
    let mut mdvar_internal = mdvar;
    let plus20 = mdvar_internal >= 20;
    if plus20 {
        mdvar_internal -= 20;
    }

    let sigma_s = if plus20 {
        0.0
    } else {
        let d_scale_m = 100e3; // Scale distance, D = 100 km
        5.0 + 3.0 * (-d_e_m / d_scale_m).exp() // [Algorithm, Eqn 5.10]
    };

    let plus10 = mdvar_internal >= 10;
    if plus10 {
        mdvar_internal -= 10;
    }

    let v_med_db = curve(ALL_YEAR[0][ci], ALL_YEAR[1][ci], ALL_YEAR[2][ci], ALL_YEAR[3][ci], ALL_YEAR[4][ci], d_e_m);

    if mdvar_internal == SINGLE_MESSAGE_MODE {
        z_t = z_s;
        z_l = z_s;
    } else if mdvar_internal == ACCIDENTAL_MODE {
        z_l = z_s;
    } else if mdvar_internal == MOBILE_MODE {
        z_l = z_t;
    }
    // else using Broadcast Mode (no additional operations)

    if z_t.abs() > 3.10 || z_l.abs() > 3.10 || z_s.abs() > 3.10 {
        *warnings |= EXTREME_VARIABILITIES;
    }

    //////////////////////////////////
    // location variability calcs

    let sigma_l = if plus10 {
        0.0
    } else {
        let delta_h_d_m = terrain_roughness(d_m, delta_h_m);
        10.0 * wn * delta_h_d_m / (wn * delta_h_d_m + 13.0) // Context of [Algorithm, Eqn 5.9]
    };
    let y_l = sigma_l * z_l;

    //////////////////////////////////
    // time variability calcs

    let q = (0.133 * wn).ln();
    let g_minus = BFM1[ci] + BFM2[ci] / ((BFM3[ci] * q).powi(2) + 1.0);
    let g_plus = BFP1[ci] + BFP2[ci] / ((BFP3[ci] * q).powi(2) + 1.0);

    let sigma_t_minus = curve(BSM1[ci], BSM2[ci], XSM1[ci], XSM2[ci], XSM3[ci], d_e_m) * g_minus;
    let sigma_t_plus = curve(BSP1[ci], BSP2[ci], XSP1[ci], XSP2[ci], XSP3[ci], d_e_m) * g_plus;

    let sigma_td = C_D[ci] * sigma_t_plus;
    let tgtd = (sigma_t_plus - sigma_td) * Z_D[ci];

    let sigma_t = if z_t < 0.0 {
        sigma_t_minus
    } else if z_t <= Z_D[ci] {
        sigma_t_plus
    } else {
        sigma_td + tgtd / z_t
    };
    let y_t = sigma_t * z_t;

    /////////////////////////////////

    // Part of [Algorithm, Eqn 5.11]
    let y_s_temp = sigma_s.powi(2) + y_t.powi(2) / (7.8 + z_s.powi(2)) + y_l.powi(2) / (24.0 + z_s.powi(2));

    let (y_r, y_s) = if mdvar_internal == SINGLE_MESSAGE_MODE {
        (0.0, (sigma_t.powi(2) + sigma_l.powi(2) + y_s_temp).sqrt() * z_s)
    } else if mdvar_internal == ACCIDENTAL_MODE {
        (y_t, (sigma_l.powi(2) + y_s_temp).sqrt() * z_s)
    } else if mdvar_internal == MOBILE_MODE {
        ((sigma_t.powi(2) + sigma_l.powi(2)).sqrt() * z_t, y_s_temp.sqrt() * z_s)
    } else {
        // BROADCAST_MODE
        (y_t + y_l, y_s_temp.sqrt() * z_s)
    };

    let mut result = a_ref_db - v_med_db - y_r - y_s;

    // [Algorithm, Eqn 52]
    if result < 0.0 {
        result = result * (29.0 - result) / (29.0 - 10.0 * result);
    }

    result
}
