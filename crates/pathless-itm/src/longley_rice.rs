//! Port of `src/LongleyRice.cpp`.

use crate::common::{dim, max, min, ItmMode};
use crate::complex::Complex;
use crate::diffraction_loss::diffraction_loss;
use crate::enums::PropMode;
use crate::errors::ItmError;
use crate::line_of_sight_loss::line_of_sight_loss;
use crate::troposcatter_loss::troposcatter_loss;
use crate::warnings::*;

/// Compute the reference attenuation, using the Longley-Rice method.
///
/// Returns `(A_ref__db, propmode)`; warning flags are OR-ed into `warnings`.
#[allow(clippy::too_many_arguments)]
pub(crate) fn longley_rice(
    theta_hzn: &[f64; 2],
    f_mhz: f64,
    z_g: Complex,
    d_hzn_m: &[f64; 2],
    h_e_m: &[f64; 2],
    gamma_e: f64,
    n_s: f64,
    delta_h_m: f64,
    h_m: &[f64; 2],
    d_m: f64,
    mode: ItmMode,
    warnings: &mut u32,
) -> Result<(f64, PropMode), ItmError> {
    // effective earth radius
    let a_e_m = 1.0 / gamma_e;

    // Terrestrial smooth earth horizon distance approximation
    let d_hzn_s_m = [(2.0 * h_e_m[0] * a_e_m).sqrt(), (2.0 * h_e_m[1] * a_e_m).sqrt()];

    // Maximum line-of-sight distance for smooth earth
    let d_sml_m = d_hzn_s_m[0] + d_hzn_s_m[1];

    // Maximum line-of-sight distance for actual path
    let d_ml_m = d_hzn_m[0] + d_hzn_m[1];

    // Angular distance of line-of-sight region
    let theta_los = -max(theta_hzn[0] + theta_hzn[1], -d_ml_m / a_e_m);

    // Check validity of small angle approximation
    if theta_hzn[0].abs() > 200e-3 {
        *warnings |= TX_HORIZON_ANGLE;
    }
    if theta_hzn[1].abs() > 200e-3 {
        *warnings |= RX_HORIZON_ANGLE;
    }

    // Checks that the actual horizon distance can't be less than 1/10 of the smooth earth horizon distance
    if d_hzn_m[0] < 0.1 * d_hzn_s_m[0] {
        *warnings |= TX_HORIZON_DISTANCE_1;
    }
    if d_hzn_m[1] < 0.1 * d_hzn_s_m[1] {
        *warnings |= RX_HORIZON_DISTANCE_1;
    }

    // Checks that the actual horizon distance can't be greater than 3 times the smooth earth horizon distance
    if d_hzn_m[0] > 3.0 * d_hzn_s_m[0] {
        *warnings |= TX_HORIZON_DISTANCE_2;
    }
    if d_hzn_m[1] > 3.0 * d_hzn_s_m[1] {
        *warnings |= RX_HORIZON_DISTANCE_2;
    }

    // Check the surface refractivity
    if n_s < 150.0 {
        return Err(ItmError::SurfaceRefractivitySmall);
    }
    if n_s > 400.0 {
        return Err(ItmError::SurfaceRefractivityLarge);
    }
    if n_s < 250.0 {
        // 150 <= N_s < 250
        *warnings |= SURFACE_REFRACTIVITY;
    }

    // Check effective earth size
    if a_e_m < 4000000.0 || a_e_m > 13333333.0 {
        return Err(ItmError::EffectiveEarth);
    }

    // Check ground impedance
    if z_g.re <= z_g.im.abs() {
        return Err(ItmError::GroundImpedance);
    }

    // Select two distances far in the diffraction region
    let d_3_m = max(d_sml_m, d_ml_m + 5.0 * (a_e_m.powi(2) / f_mhz).powf(1.0 / 3.0));
    let d_4_m = d_3_m + 10.0 * (a_e_m.powi(2) / f_mhz).powf(1.0 / 3.0);

    // Compute the diffraction loss at the two distances
    let a_3_db = diffraction_loss(d_3_m, d_hzn_m, h_e_m, z_g, a_e_m, delta_h_m, h_m, mode, theta_los, d_sml_m, f_mhz);
    let a_4_db = diffraction_loss(d_4_m, d_hzn_m, h_e_m, z_g, a_e_m, delta_h_m, h_m, mode, theta_los, d_sml_m, f_mhz);

    // Compute the slope and intercept of the diffraction line
    let m_d = (a_4_db - a_3_db) / (d_4_m - d_3_m);
    let a_d0_db = a_3_db - m_d * d_3_m;

    let d_min_m = (h_e_m[0] - h_e_m[1]).abs() / 200e-3;

    if d_m < d_min_m {
        *warnings |= PATH_DISTANCE_TOO_SMALL_1;
    }
    if d_m < 1e3 {
        *warnings |= PATH_DISTANCE_TOO_SMALL_2;
    }
    if d_m > 1000e3 {
        *warnings |= PATH_DISTANCE_TOO_BIG_1;
    }
    if d_m > 2000e3 {
        *warnings |= PATH_DISTANCE_TOO_BIG_2;
    }

    let mut a_ref_db;
    let propmode;

    // if the path distance is less than the maximum smooth earth line of sight distance...
    if d_m < d_sml_m {
        // Compute the diffraction loss at the maximum smooth earth line of sight distance
        let a_sml_db = d_sml_m * m_d + a_d0_db;

        // [ERL 79-ITS 67, Eqn 3.16a], in meters instead of km and with MIN() part below
        let mut d_0_m = 0.04 * f_mhz * h_e_m[0] * h_e_m[1];

        let d_1_m = if a_d0_db >= 0.0 {
            d_0_m = min(d_0_m, 0.5 * d_ml_m); // other part of [ERL 79-ITS 67, Eqn 3.16a]
            d_0_m + 0.25 * (d_ml_m - d_0_m) // [ERL 79-ITS 67, Eqn 3.16d]
        } else {
            max(-a_d0_db / m_d, 0.25 * d_ml_m)
        };

        let a_1_db = line_of_sight_loss(d_1_m, h_e_m, z_g, delta_h_m, m_d, a_d0_db, d_sml_m, f_mhz);

        let mut flag = false;

        let mut khat_1_db_per_meter = 0.0;
        let mut khat_2_db_per_meter = 0.0;

        if d_0_m < d_1_m {
            let a_0_db = line_of_sight_loss(d_0_m, h_e_m, z_g, delta_h_m, m_d, a_d0_db, d_sml_m, f_mhz);

            let q = (d_sml_m / d_0_m).ln();

            // [ERL 79-ITS 67, Eqn 3.20]
            khat_2_db_per_meter = max(
                0.0,
                ((d_sml_m - d_0_m) * (a_1_db - a_0_db) - (d_1_m - d_0_m) * (a_sml_db - a_0_db))
                    / ((d_sml_m - d_0_m) * (d_1_m / d_0_m).ln() - (d_1_m - d_0_m) * q),
            );

            flag = a_d0_db > 0.0 || khat_2_db_per_meter > 0.0;

            if flag {
                // [ERL 79-ITS 67, Eqn 3.21]
                khat_1_db_per_meter = (a_sml_db - a_0_db - khat_2_db_per_meter * q) / (d_sml_m - d_0_m);

                if khat_1_db_per_meter < 0.0 {
                    khat_1_db_per_meter = 0.0;
                    khat_2_db_per_meter = dim(a_sml_db, a_0_db) / q;

                    if khat_2_db_per_meter == 0.0 {
                        khat_1_db_per_meter = m_d;
                    }
                }
            }
        }

        if !flag {
            khat_1_db_per_meter = dim(a_sml_db, a_1_db) / (d_sml_m - d_1_m);
            khat_2_db_per_meter = 0.0;

            if khat_1_db_per_meter == 0.0 {
                khat_1_db_per_meter = m_d;
            }
        }

        let a_o_db = a_sml_db - khat_1_db_per_meter * d_sml_m - khat_2_db_per_meter * d_sml_m.ln();

        // [ERL 79-ITS 67, Eqn 3.19]
        a_ref_db = a_o_db + khat_1_db_per_meter * d_m + khat_2_db_per_meter * d_m.ln();
        propmode = PropMode::LineOfSight;
    } else {
        // this is a trans-horizon path

        // select to points far into the troposcatter region
        let d_5_m = d_ml_m + 200e3;
        let d_6_m = d_ml_m + 400e3;

        // Compute the troposcatter loss at the two distances
        let mut h0 = -1.0;
        let a_6_db = troposcatter_loss(d_6_m, theta_hzn, d_hzn_m, h_e_m, a_e_m, n_s, f_mhz, theta_los, &mut h0);
        let a_5_db = troposcatter_loss(d_5_m, theta_hzn, d_hzn_m, h_e_m, a_e_m, n_s, f_mhz, theta_los, &mut h0);

        let m_s;
        let a_s0_db;
        let d_x_m;

        // if we got a reasonable prediction value back...
        if a_5_db < 1000.0 {
            // Compute the slope of the troposcatter line
            m_s = (a_6_db - a_5_db) / 200e3;

            // Find the diffraction-troposcatter transition distance
            d_x_m = max(
                max(d_sml_m, d_ml_m + 1.088 * (a_e_m.powi(2) / f_mhz).powf(1.0 / 3.0) * f_mhz.ln()),
                (a_5_db - a_d0_db - m_s * d_5_m) / (m_d - m_s),
            );

            // Compute the intercept of the troposcatter line
            a_s0_db = (m_d - m_s) * d_x_m + a_d0_db;
        } else {
            // troposcatter gives no real results - so use diffraction line parameters for tropo line
            m_s = m_d;
            a_s0_db = a_d0_db;
            d_x_m = 10e6;
        }

        // Determine if its diffraction or troposcatter and compute the loss
        if d_m > d_x_m {
            a_ref_db = m_s * d_m + a_s0_db;
            propmode = PropMode::Troposcatter;
        } else {
            a_ref_db = m_d * d_m + a_d0_db;
            propmode = PropMode::Diffraction;
        }
    }

    // Don't allow a negative loss
    a_ref_db = max(a_ref_db, 0.0);

    Ok((a_ref_db, propmode))
}
