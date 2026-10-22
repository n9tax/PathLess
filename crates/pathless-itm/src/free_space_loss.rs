//! Port of `src/FreeSpaceLoss.cpp`.

/// Free space basic transmission loss, in dB.
///
/// `32.45 + 20 log10(f__mhz) + 20 log10(d__km)` with `d_m` in meters and
/// `f_mhz` in MHz. Identical to NTIA `FreeSpaceLoss()`.
#[inline]
pub fn free_space_loss_db(d_m: f64, f_mhz: f64) -> f64 {
    32.45 + 20.0 * f_mhz.log10() + 20.0 * (d_m / 1000.0).log10()
}
