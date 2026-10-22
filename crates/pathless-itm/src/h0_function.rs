//! Port of `src/H0Function.cpp`.

use crate::common::{max, min, trunc_i};

/// Curve fit helper function to approximate H_0(); `j` selects the eta_s
/// curve, `r` is r_1,2. Result in dB.
fn h0_curve(j: usize, r: f64) -> f64 {
    // values from [Algorithm, 6.13]
    const A: [f64; 5] = [25.0, 80.0, 177.0, 395.0, 705.0];
    const B: [f64; 5] = [24.0, 45.0, 68.0, 80.0, 105.0];

    // related to TN101v2, Eqn III.49, but from [Algorithm, 6.13]
    10.0 * (1.0 + A[j] * (1.0 / r).powf(4.0) + B[j] * (1.0 / r).powi(2)).log10()
}

/// Troposcatter frequency gain function, H_0(), from [TN101v1, Ch 9.2], in dB.
pub(crate) fn h0_function(r: f64, eta_s: f64) -> f64 {
    let eta_s = min(max(eta_s, 1.0), 5.0); // range 1 <= eta_s <= 5

    let i = trunc_i(eta_s); // integer part of eta_s
    let q = eta_s - i as f64; // decimal part of eta_s

    let mut result = h0_curve((i - 1) as usize, r);

    if q != 0.0 {
        // interpolate with next curve, if needed
        result = (1.0 - q) * result + q * h0_curve(i as usize, r);
    }

    result
}
