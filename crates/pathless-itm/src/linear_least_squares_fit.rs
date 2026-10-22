//! Port of `src/LinearLeastSquaresFit.cpp`.

use crate::common::{fdim, trunc_i, Pfl};

/// Linear least squares fit to the terrain data between `d_start` and
/// `d_end` (in the profile's distance units). Returns `(fit_y1, fit_y2)`,
/// the fitted line's values at the start and the end of the whole profile.
pub(crate) fn linear_least_squares_fit(pfl: Pfl<'_>, d_start: f64, d_end: f64) -> (f64, f64) {
    let z = pfl.z;
    let np = pfl.np_i();

    let mut i_start = trunc_i(fdim(d_start / pfl.xi, 0.0));
    let mut i_end = np - trunc_i(fdim(np as f64, d_end / pfl.xi));

    if i_end <= i_start {
        i_start = trunc_i(fdim(i_start as f64, 1.0));
        i_end = np - trunc_i(fdim(np as f64, i_end as f64 + 1.0));
    }

    let x_length = (i_end - i_start) as f64;

    let mut mid_shifted_index = -0.5 * x_length;
    let mid_shifted_end = i_end as f64 + mid_shifted_index;

    let mut sum_y = 0.5 * (z[i_start as usize] + z[i_end as usize]);
    let mut scaled_sum_y = 0.5 * (z[i_start as usize] - z[i_end as usize]) * mid_shifted_index;

    let mut i = 2i32;
    while (i as f64) <= x_length {
        i_start += 1;
        mid_shifted_index += 1.0;

        let zi = z[i_start as usize];
        sum_y += zi;
        scaled_sum_y += zi * mid_shifted_index;
        i += 1;
    }

    sum_y /= x_length;
    scaled_sum_y = scaled_sum_y * 12.0 / ((x_length * x_length + 2.0) * x_length);

    let fit_y1 = sum_y - scaled_sum_y * mid_shifted_end;
    let fit_y2 = sum_y + scaled_sum_y * (np as f64 - mid_shifted_end);
    (fit_y1, fit_y2)
}
