//! Port of `src/LineOfSightLoss.cpp`.

use crate::common::{max, min, PI};
use crate::complex::Complex;
use crate::sigma_h_function::sigma_h_function;
use crate::terrain_roughness::terrain_roughness;

/// Loss in the line-of-sight region, in dB.
#[allow(clippy::too_many_arguments)]
pub(crate) fn line_of_sight_loss(
    d_m: f64,
    h_e_m: &[f64; 2],
    z_g: Complex,
    delta_h_m: f64,
    m_d: f64,
    a_d0: f64,
    d_sml_m: f64,
    f_mhz: f64,
) -> f64 {
    let delta_h_d_m = terrain_roughness(d_m, delta_h_m);

    let sigma_h_d_m = sigma_h_function(delta_h_d_m);

    // wavenumber, k
    let wn = f_mhz / 47.7;

    // [Algorithm, Eqn 4.46]
    let sin_psi = (h_e_m[0] + h_e_m[1]) / (d_m.powi(2) + (h_e_m[0] + h_e_m[1]).powi(2)).sqrt();

    // [Algorithm, Eqn 4.47]
    let sp = Complex::new(sin_psi, 0.0);
    let mut r_e = (sp - z_g) / (sp + z_g) * (-min(10.0, wn * sigma_h_d_m * sin_psi)).exp();

    // q = Magnitude of R_e', [Algorithm, Eqn 4.48]
    let q = r_e.re.powi(2) + r_e.im.powi(2);
    if q < 0.25 || q < sin_psi {
        r_e = r_e * (sin_psi / q).sqrt();
    }

    // phase difference between rays, [Algorithm, Eqn 4.49]
    let mut delta_phi = wn * 2.0 * h_e_m[0] * h_e_m[1] / d_m;

    // [Algorithm, Eqn 4.50]
    if delta_phi > PI / 2.0 {
        delta_phi = PI - (PI / 2.0).powi(2) / delta_phi;
    }

    // Two-ray attenuation
    let rr = Complex::new(delta_phi.cos(), -delta_phi.sin()) + r_e;
    let a_t_db = -10.0 * (rr.re.powi(2) + rr.im.powi(2)).log10();

    // Extended diffraction attenuation
    let a_d_db = m_d * d_m + a_d0;

    // weighting factor
    let w = 1.0 / (1.0 + f_mhz * delta_h_m / max(10e3, d_sml_m));

    w * a_t_db + (1.0 - w) * a_d_db
}
