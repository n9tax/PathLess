//! Elementary RF formulas and unit conversions.
//!
//! Conventions: frequency in **MHz**, distances in **metres** unless the function name
//! ends in `_km`, power in dBm / dBW / W as named.

/// Speed of light in vacuum (m/s).
pub const SPEED_OF_LIGHT_M_S: f64 = 299_792_458.0;

/// Mean Earth radius used for effective-earth calculations (m). ITU-R P.452/P.526 value.
pub const EARTH_RADIUS_M: f64 = 6_371_000.0;

/// Mean Earth radius (km).
pub const EARTH_RADIUS_KM: f64 = 6_371.0;

/// Standard-atmosphere median effective earth radius factor k = 4/3.
pub const K_FACTOR_STANDARD: f64 = 4.0 / 3.0;

/// Free-space loss constant for d in km and f in MHz: 20·log10(4π·10⁹ / c) = 32.4478 dB.
pub const FSPL_CONST_KM_MHZ: f64 = 32.4478;

/// Thermal noise density used for noise-floor calculations (dBm/Hz), kT at 290 K
/// rounded to the customary −174 dBm/Hz.
pub const THERMAL_NOISE_DBM_PER_HZ: f64 = -174.0;

/// Difference between dBi and dBd: a half-wave dipole has 2.15 dBi gain.
pub const DIPOLE_GAIN_DBI: f64 = 2.15;

/// Wavelength (m) for a frequency in MHz.
///
/// ```
/// let l = pathless_rf::basic::wavelength_m(300.0);
/// assert!((l - 0.9993).abs() < 1e-3);
/// ```
pub fn wavelength_m(f_mhz: f64) -> f64 {
    SPEED_OF_LIGHT_M_S / (f_mhz * 1.0e6)
}

/// Free-space path loss (dB) per ITU-R P.525: `32.4478 + 20·log10(f_MHz) + 20·log10(d_km)`.
pub fn fspl_db(d_km: f64, f_mhz: f64) -> f64 {
    FSPL_CONST_KM_MHZ + 20.0 * f_mhz.log10() + 20.0 * d_km.log10()
}

/// Convert dB to a linear power ratio.
pub fn db_to_linear(db: f64) -> f64 {
    10f64.powf(db / 10.0)
}

/// Convert a linear power ratio to dB.
pub fn linear_to_db(ratio: f64) -> f64 {
    10.0 * ratio.log10()
}

/// Power sum of several levels expressed in dB (dBm, dBW …): `10·log10(Σ 10^(x/10))`.
pub fn power_sum_db(levels_db: &[f64]) -> f64 {
    linear_to_db(levels_db.iter().map(|x| db_to_linear(*x)).sum())
}

/// dBm → watts.
pub fn dbm_to_watts(dbm: f64) -> f64 {
    10f64.powf((dbm - 30.0) / 10.0)
}

/// Watts → dBm.
pub fn watts_to_dbm(w: f64) -> f64 {
    10.0 * w.log10() + 30.0
}

/// dBW → watts.
pub fn dbw_to_watts(dbw: f64) -> f64 {
    10f64.powf(dbw / 10.0)
}

/// Watts → dBW.
pub fn watts_to_dbw(w: f64) -> f64 {
    10.0 * w.log10()
}

/// dBm → dBW (subtract 30 dB).
pub fn dbm_to_dbw(dbm: f64) -> f64 {
    dbm - 30.0
}

/// dBW → dBm (add 30 dB).
pub fn dbw_to_dbm(dbw: f64) -> f64 {
    dbw + 30.0
}

/// Gain in dBd → dBi (+2.15 dB).
pub fn dbd_to_dbi(dbd: f64) -> f64 {
    dbd + DIPOLE_GAIN_DBI
}

/// Gain in dBi → dBd (−2.15 dB).
pub fn dbi_to_dbd(dbi: f64) -> f64 {
    dbi - DIPOLE_GAIN_DBI
}

/// Radius (m) of the n-th Fresnel zone at a point `d1_m` from one terminal and `d2_m`
/// from the other: `r_n = sqrt(n·λ·d1·d2 / (d1 + d2))`.
///
/// Returns 0 at the terminals (d1 or d2 = 0).
pub fn fresnel_radius_m(n: u32, f_mhz: f64, d1_m: f64, d2_m: f64) -> f64 {
    let d = d1_m + d2_m;
    if d <= 0.0 || d1_m <= 0.0 || d2_m <= 0.0 {
        return 0.0;
    }
    (n as f64 * wavelength_m(f_mhz) * d1_m * d2_m / d).sqrt()
}

/// Effective earth radius (m) for a k-factor: `k · 6371 km`.
pub fn effective_earth_radius_m(k_factor: f64) -> f64 {
    k_factor * EARTH_RADIUS_M
}

/// Earth bulge (m) at a point `d1_m`/`d2_m` from the two terminals for effective earth
/// radius factor `k_factor`: `h = d1·d2 / (2·k·a)`, a = 6371 km.
///
/// Example: midpoint of a 50 km path at k = 4/3 → ≈ 36.8 m.
pub fn earth_bulge_m(d1_m: f64, d2_m: f64, k_factor: f64) -> f64 {
    d1_m * d2_m / (2.0 * effective_earth_radius_m(k_factor))
}

/// k-factor from the vertical refractivity gradient dN/dh (N-units/km):
/// `k = 157 / (157 + dN/dh)` (ITU-R P.453 / P.310). −40 N/km → k ≈ 4/3.
pub fn k_factor_from_gradient(dn_dh_per_km: f64) -> f64 {
    157.0 / (157.0 + dn_dh_per_km)
}

/// EIRP (dBm) = transmitter power − feeder losses + antenna gain (dBi).
pub fn eirp_dbm(tx_power_dbm: f64, line_loss_db: f64, antenna_gain_dbi: f64) -> f64 {
    tx_power_dbm - line_loss_db + antenna_gain_dbi
}

/// ERP (dBm, referenced to a half-wave dipole) = EIRP − 2.15 dB.
pub fn erp_dbm(tx_power_dbm: f64, line_loss_db: f64, antenna_gain_dbi: f64) -> f64 {
    eirp_dbm(tx_power_dbm, line_loss_db, antenna_gain_dbi) - DIPOLE_GAIN_DBI
}

/// EIRP (dBm) → ERP (dBm).
pub fn eirp_to_erp(eirp_dbm: f64) -> f64 {
    eirp_dbm - DIPOLE_GAIN_DBI
}

/// ERP (dBm) → EIRP (dBm).
pub fn erp_to_eirp(erp_dbm: f64) -> f64 {
    erp_dbm + DIPOLE_GAIN_DBI
}

/// Thermal noise floor (dBm): `−174 + 10·log10(BW_Hz) + NF_dB`.
pub fn thermal_noise_floor_dbm(bandwidth_hz: f64, noise_figure_db: f64) -> f64 {
    THERMAL_NOISE_DBM_PER_HZ + 10.0 * bandwidth_hz.log10() + noise_figure_db
}

/// Receiver sensitivity (dBm) from noise floor and required SNR (dB).
pub fn sensitivity_dbm(bandwidth_hz: f64, noise_figure_db: f64, required_snr_db: f64) -> f64 {
    thermal_noise_floor_dbm(bandwidth_hz, noise_figure_db) + required_snr_db
}

/// Great-circle style straight-line distance helper: slant range (m) between two points
/// given horizontal separation (m) and height difference (m).
pub fn slant_range_m(horizontal_m: f64, dh_m: f64) -> f64 {
    horizontal_m.hypot(dh_m)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn close(a: f64, b: f64, tol: f64) -> bool {
        (a - b).abs() <= tol
    }

    #[test]
    fn wavelength() {
        assert!(close(wavelength_m(299.792458), 1.0, 1e-12));
        assert!(close(wavelength_m(2400.0), 0.124913524, 1e-8));
    }

    #[test]
    fn fspl_known_values() {
        // 1 km @ 1 MHz = constant
        assert!(close(fspl_db(1.0, 1.0), 32.4478, 1e-9));
        // 10 km @ 2400 MHz: 32.4478 + 67.6042 + 20 = 120.052
        assert!(close(fspl_db(10.0, 2400.0), 120.0520, 1e-3));
        // Exact physics check: (4πd/λ)²
        let d = 25_000.0;
        let f = 5800.0;
        let exact = 20.0 * (4.0 * std::f64::consts::PI * d / wavelength_m(f)).log10();
        assert!(close(fspl_db(d / 1000.0, f), exact, 1e-3));
    }

    #[test]
    fn power_conversions() {
        assert!(close(dbm_to_watts(30.0), 1.0, 1e-12));
        assert!(close(dbm_to_watts(0.0), 1e-3, 1e-15));
        assert!(close(watts_to_dbm(100.0), 50.0, 1e-12));
        assert!(close(watts_to_dbw(100.0), 20.0, 1e-12));
        assert!(close(dbw_to_watts(10.0), 10.0, 1e-12));
        assert!(close(dbm_to_dbw(43.0), 13.0, 1e-12));
        assert!(close(dbw_to_dbm(13.0), 43.0, 1e-12));
        assert!(close(power_sum_db(&[-90.0, -90.0]), -86.9897, 1e-4));
        assert!(close(db_to_linear(3.0), 1.995262, 1e-6));
        assert!(close(linear_to_db(2.0), 3.0103, 1e-4));
    }

    #[test]
    fn gain_conversions() {
        assert!(close(dbd_to_dbi(0.0), 2.15, 1e-12));
        assert!(close(dbi_to_dbd(17.15), 15.0, 1e-12));
    }

    #[test]
    fn fresnel() {
        // 10 km path, midpoint, 5.8 GHz: F1 = 17.32*sqrt(5*5/(5.8*10)) = 11.37 m
        let r = fresnel_radius_m(1, 5800.0, 5000.0, 5000.0);
        assert!(close(r, 17.3 * (25.0f64 / 58.0).sqrt(), 0.03), "{r}");
        assert!(close(fresnel_radius_m(2, 5800.0, 5000.0, 5000.0), r * 2f64.sqrt(), 1e-9));
        assert_eq!(fresnel_radius_m(1, 5800.0, 0.0, 5000.0), 0.0);
    }

    #[test]
    fn bulge() {
        // midpoint of 50 km at k=4/3 → 25000²/(2*8494666) = 36.79 m
        let b = earth_bulge_m(25_000.0, 25_000.0, 4.0 / 3.0);
        assert!(close(b, 36.79, 0.01), "{b}");
        // common rule of thumb h_ft = d1*d2/(1.5k) (miles) is consistent; k=1 → 49.05 m
        assert!(close(earth_bulge_m(25_000.0, 25_000.0, 1.0), 49.05, 0.01));
        assert!(close(k_factor_from_gradient(-39.25), 4.0 / 3.0, 1e-9));
    }

    #[test]
    fn eirp_noise() {
        assert!(close(eirp_dbm(30.0, 2.0, 15.0), 43.0, 1e-12));
        assert!(close(erp_dbm(30.0, 2.0, 15.0), 40.85, 1e-12));
        assert!(close(eirp_to_erp(erp_to_eirp(10.0)), 10.0, 1e-12));
        // 20 MHz, NF 5 dB: -174 + 73.01 + 5 = -95.99
        assert!(close(thermal_noise_floor_dbm(20e6, 5.0), -95.9897, 1e-4));
        assert!(close(sensitivity_dbm(1e6, 0.0, 10.0), -104.0, 1e-9));
        assert!(close(slant_range_m(3.0, 4.0), 5.0, 1e-12));
    }
}
