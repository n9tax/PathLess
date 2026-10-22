//! Port of `src/ComputeDeltaH.cpp`.

use crate::common::{max_i, min_i, trunc_i, Pfl};
use crate::linear_least_squares_fit::linear_least_squares_fit;

/// Maximum number of resampled points: n = 10 * p10 - 5 with p10 <= 25.
const N_MAX: usize = 245;

/// Compute the terrain irregularity parameter, delta_h, in meters, over the
/// part of the profile between `d_start_m` and `d_end_m`.
///
/// Unlike the C++ (which uses a `std::vector` for the residuals) this uses
/// fixed-size stack buffers, so it never allocates.
pub(crate) fn compute_delta_h(pfl: Pfl<'_>, d_start_m: f64, d_end_m: f64) -> f64 {
    let mut s = [0.0f64; N_MAX + 2]; // Temp pfl data array (C++: double s[247])
    let z = pfl.z;

    let np = pfl.np_i();
    let mut x_start = d_start_m / pfl.xi; // index to start considering terrain points
    let mut x_end = d_end_m / pfl.xi; // index to stop considering terrain points

    // if there are less than 2 terrain points, return delta_h = 0
    if x_end - x_start < 2.0 {
        return 0.0;
    }

    let mut p10 = trunc_i(0.1 * (x_end - x_start + 8.0));
    p10 = min_i(max_i(4, p10), 25); // 10% index

    let n = 10 * p10 - 5;
    let p90 = n - p10; // 90% index

    let np_s = (n - 1) as f64;
    s[0] = np_s;
    s[1] = 1.0;

    x_end = (x_end - x_start) / np_s;
    let mut i = trunc_i(x_start);
    // C++: x_start -= float(i + 1.0);
    x_start -= ((i as f64 + 1.0) as f32) as f64;

    for j in 0..n as usize {
        while x_start > 0.0 && (i + 1) < np {
            x_start -= 1.0;
            i += 1;
        }

        let iu = i as usize;
        s[j + 2] = z[iu + 1] + (z[iu + 1] - z[iu]) * x_start;

        x_start += x_end;
    }

    let n_us = n as usize;
    let s_pfl = Pfl { np: n_us - 1, xi: 1.0, z: &s[2..2 + n_us] };
    let (mut fit_y1, mut fit_y2) = linear_least_squares_fit(s_pfl, 0.0, np_s);

    fit_y2 = (fit_y2 - fit_y1) / np_s;

    // compute the difference between fitted line and actual data
    let mut diffs_buf = [0.0f64; N_MAX];
    let diffs = &mut diffs_buf[..n_us];
    for j in 0..n_us {
        diffs[j] = s[j + 2] - fit_y1;
        fit_y1 += fit_y2;
    }

    // std::nth_element(..., std::greater<double>()): k-th largest value.
    let desc = |a: &f64, b: &f64| b.total_cmp(a);
    let k10 = (p10 - 1) as usize;
    diffs.select_nth_unstable_by(k10, desc);
    let q10 = diffs[k10];

    let k90 = p90 as usize;
    diffs.select_nth_unstable_by(k90, desc);
    let q90 = diffs[k90];

    let delta_h_d_m = q10 - q90;

    // [ERL 79-ITS 67, Eqn 3], inverted
    delta_h_d_m / (1.0 - 0.8 * (-(d_end_m - d_start_m) / 50e3).exp())
}
