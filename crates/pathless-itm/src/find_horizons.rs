//! Port of `src/FindHorizons.cpp`.

use crate::common::Pfl;

/// Compute the radio horizons of the terminals.
///
/// Returns `(theta_hzn, d_hzn_m)`: terminal radio horizon angles (radians)
/// and distances (meters).
pub(crate) fn find_horizons(pfl: Pfl<'_>, a_e_m: f64, h_m: &[f64; 2]) -> ([f64; 2], [f64; 2]) {
    let np = pfl.np;
    let xi = pfl.xi;
    let z = pfl.z;

    let d_m = pfl.np_f() * pfl.xi;

    // compute radials (ignore radius of earth since it cancels out in the later math)
    let z_tx_m = z[0] + h_m[0];
    let z_rx_m = z[np] + h_m[1];

    // set the terminal horizon angles as if the terminals are line-of-sight
    // [TN101, Eq 6.15]
    let mut theta_hzn = [(z_rx_m - z_tx_m) / d_m - d_m / (2.0 * a_e_m), -(z_rx_m - z_tx_m) / d_m - d_m / (2.0 * a_e_m)];

    let mut d_hzn_m = [d_m, d_m];

    let mut d_tx_m = 0.0;
    let mut d_rx_m = d_m;

    for &zi in &z[1..np.max(1)] {
        d_tx_m += xi;
        d_rx_m -= xi;

        let theta_tx = (zi - z_tx_m) / d_tx_m - d_tx_m / (2.0 * a_e_m);
        let theta_rx = -(z_rx_m - zi) / d_rx_m - d_rx_m / (2.0 * a_e_m);

        if theta_tx > theta_hzn[0] {
            theta_hzn[0] = theta_tx;
            d_hzn_m[0] = d_tx_m;
        }

        if theta_rx > theta_hzn[1] {
            theta_hzn[1] = theta_rx;
            d_hzn_m[1] = d_rx_m;
        }
    }

    (theta_hzn, d_hzn_m)
}
