//! Port of `src/InitializePointToPoint.cpp`.

use crate::complex::Complex;
use crate::enums::Polarization;

/// Output of [`initialize_point_to_point`]: `(Z_g, gamma_e, N_s)`.
pub(crate) struct InitP2p {
    /// Complex ground impedance.
    pub z_g: Complex,
    /// Curvature of the effective earth.
    pub gamma_e: f64,
    /// Surface refractivity, in N-Units.
    pub n_s: f64,
}

/// Initialize parameters for point-to-point mode (also used by area mode
/// with `h_sys_m = 0`).
pub(crate) fn initialize_point_to_point(
    f_mhz: f64,
    h_sys_m: f64,
    n_0: f64,
    pol: Polarization,
    epsilon: f64,
    sigma: f64,
) -> InitP2p {
    // gamma_a is the curvature of the actual earth, ~1 / 6370 km
    const GAMMA_A: f64 = 157e-9;

    // scale the refractivity based on the elevation above mean sea level
    let n_s = if h_sys_m == 0.0 {
        n_0
    } else {
        n_0 * (-h_sys_m / 9460.0).exp() // [TN101, Eq 4.3]
    };

    // gamma_e is the curvature of the effective earth
    let gamma_e = GAMMA_A * (1.0 - 0.04665 * (n_s / 179.3).exp()); // [TN101, Eq 4.4], reworked

    // complex relative permittivity
    let ep_r = Complex::new(epsilon, 18000.0 * sigma / f_mhz);

    let mut z_g = (ep_r - Complex::new(1.0, 0.0)).sqrt(); // ground impedance (horizontal polarization)

    if pol == Polarization::Vertical {
        // adjust for vertical polarization
        z_g = z_g / ep_r;
    }

    InitP2p { z_g, gamma_e, n_s }
}
