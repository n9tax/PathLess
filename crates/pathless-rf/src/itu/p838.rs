//! ITU-R P.838-3 (03/2005): specific attenuation model for rain.
//!
//! `γR = k·R^α` (dB/km), with k and α from the regression equations (2), (3) and the
//! polarization / elevation combination equations (4), (5). Valid 1–1000 GHz.
//!
//! Coefficients are transcribed from Tables 1–4 of the Recommendation (cross-checked with
//! ITU-Rpy `itur/models/itu838.py`), and the implementation is validated against Table 5 of
//! the Recommendation and the ITU-R SG3 validation data set.

use serde::{Deserialize, Serialize};

/// k_H, α_H, k_V, α_V at one frequency.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct RainCoefficientsHV {
    /// k for horizontal polarization.
    pub k_h: f64,
    /// α for horizontal polarization.
    pub alpha_h: f64,
    /// k for vertical polarization.
    pub k_v: f64,
    /// α for vertical polarization.
    pub alpha_v: f64,
}

/// Combined k and α for a given polarization tilt and path elevation.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct RainCoefficients {
    /// Coefficient k.
    pub k: f64,
    /// Exponent α.
    pub alpha: f64,
}

// Table 1: kH  (aj, bj, cj), mk, ck
const KH_A: [f64; 4] = [-5.33980, -0.35351, -0.23789, -0.94158];
const KH_B: [f64; 4] = [-0.10008, 1.26970, 0.86036, 0.64552];
const KH_C: [f64; 4] = [1.13098, 0.45400, 0.15354, 0.16817];
const KH_M: f64 = -0.18961;
const KH_CK: f64 = 0.71147;
// Table 2: kV
const KV_A: [f64; 4] = [-3.80595, -3.44965, -0.39902, 0.50167];
const KV_B: [f64; 4] = [0.56934, -0.22911, 0.73042, 1.07319];
const KV_C: [f64; 4] = [0.81061, 0.51059, 0.11899, 0.27195];
const KV_M: f64 = -0.16398;
const KV_CK: f64 = 0.63297;
// Table 3: αH
const AH_A: [f64; 5] = [-0.14318, 0.29591, 0.32177, -5.37610, 16.1721];
const AH_B: [f64; 5] = [1.82442, 0.77564, 0.63773, -0.96230, -3.29980];
const AH_C: [f64; 5] = [-0.55187, 0.19822, 0.13164, 1.47828, 3.43990];
const AH_M: f64 = 0.67849;
const AH_CA: f64 = -1.95537;
// Table 4: αV
const AV_A: [f64; 5] = [-0.07771, 0.56727, -0.20238, -48.2991, 48.5833];
const AV_B: [f64; 5] = [2.33840, 0.95545, 1.14520, 0.791669, 0.791459];
const AV_C: [f64; 5] = [-0.76284, 0.54039, 0.26809, 0.116226, 0.116479];
const AV_M: f64 = -0.053739;
const AV_CA: f64 = 0.83433;

fn gauss_sum(lf: f64, a: &[f64], b: &[f64], c: &[f64]) -> f64 {
    a.iter().zip(b).zip(c).map(|((a, b), c)| a * (-((lf - b) / c).powi(2)).exp()).sum()
}

/// k_H, α_H, k_V, α_V at frequency `f_ghz` (GHz), P.838-3 equations (2) and (3).
pub fn coefficients_hv(f_ghz: f64) -> RainCoefficientsHV {
    let lf = f_ghz.log10();
    let k_h = 10f64.powf(gauss_sum(lf, &KH_A, &KH_B, &KH_C) + KH_M * lf + KH_CK);
    let k_v = 10f64.powf(gauss_sum(lf, &KV_A, &KV_B, &KV_C) + KV_M * lf + KV_CK);
    let alpha_h = gauss_sum(lf, &AH_A, &AH_B, &AH_C) + AH_M * lf + AH_CA;
    let alpha_v = gauss_sum(lf, &AV_A, &AV_B, &AV_C) + AV_M * lf + AV_CA;
    RainCoefficientsHV { k_h, alpha_h, k_v, alpha_v }
}

/// Combined k and α for polarization tilt τ (`pol_tilt_deg`, 0 = horizontal, 90 = vertical,
/// 45 = circular) and path elevation θ (`elevation_deg`), P.838-3 eqs. (4), (5).
pub fn coefficients(f_ghz: f64, pol_tilt_deg: f64, elevation_deg: f64) -> RainCoefficients {
    let c = coefficients_hv(f_ghz);
    let cos2el = elevation_deg.to_radians().cos().powi(2);
    let cos2tau = (2.0 * pol_tilt_deg).to_radians().cos();
    let k = (c.k_h + c.k_v + (c.k_h - c.k_v) * cos2el * cos2tau) / 2.0;
    let alpha = (c.k_h * c.alpha_h + c.k_v * c.alpha_v + (c.k_h * c.alpha_h - c.k_v * c.alpha_v) * cos2el * cos2tau)
        / (2.0 * k);
    RainCoefficients { k, alpha }
}

/// Rain specific attenuation γR = k·R^α (dB/km).
///
/// * `f_ghz` – frequency (GHz), 1–1000
/// * `r_mm_per_h` – rain rate (mm/h)
/// * `pol_tilt_deg` – polarization tilt τ (deg): 0 H, 90 V, 45 circular
/// * `elevation_deg` – path elevation angle θ (deg), 0 for terrestrial links
pub fn gamma_r_db_per_km(f_ghz: f64, r_mm_per_h: f64, pol_tilt_deg: f64, elevation_deg: f64) -> f64 {
    if r_mm_per_h <= 0.0 {
        return 0.0;
    }
    let c = coefficients(f_ghz, pol_tilt_deg, elevation_deg);
    c.k * r_mm_per_h.powf(c.alpha)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rel(a: f64, b: f64) -> f64 {
        ((a - b) / b).abs()
    }

    /// Table 5 of P.838-3 (values printed with 4–5 significant digits).
    #[test]
    fn table5_values() {
        // f, kH, αH, kV, αV
        let rows = [
            (1.0, 0.0000259, 0.9691, 0.0000308, 0.8592),
            (4.0, 0.0001071, 1.6009, 0.0002461, 1.2476),
            (6.0, 0.0007056, 1.5900, 0.0004878, 1.5728),
            (10.0, 0.01217, 1.2571, 0.01129, 1.2156),
            (11.0, 0.01772, 1.2140, 0.01731, 1.1617),
            (18.0, 0.07078, 1.0818, 0.07708, 1.0025),
            (20.0, 0.09164, 1.0568, 0.09611, 0.9847),
            (23.0, 0.1286, 1.0214, 0.1284, 0.9630),
            (38.0, 0.4001, 0.8816, 0.3844, 0.8552),
            (60.0, 0.8606, 0.7656, 0.8515, 0.7486),
            (80.0, 1.1704, 0.7115, 1.1668, 0.7021),
            (100.0, 1.3671, 0.6815, 1.3680, 0.6765),
            (400.0, 1.5860, 0.6262, 1.5820, 0.6256),
            (1000.0, 1.3795, 0.6396, 1.3822, 0.6365),
        ];
        for (f, kh, ah, kv, av) in rows {
            let c = coefficients_hv(f);
            assert!(rel(c.k_h, kh) < 2e-3, "kH at {f}: {} vs {kh}", c.k_h);
            assert!(rel(c.k_v, kv) < 2e-3, "kV at {f}: {} vs {kv}", c.k_v);
            assert!((c.alpha_h - ah).abs() < 1e-4, "aH at {f}: {} vs {ah}", c.alpha_h);
            assert!((c.alpha_v - av).abs() < 1e-4, "aV at {f}: {} vs {av}", c.alpha_v);
        }
    }

    /// ITU-R SG3 validation data (ITURP838-3_rain_specific_attenuation.csv, τ = 0).
    #[test]
    fn itu_validation_examples() {
        // el, f, R, k, alpha, gamma
        let rows = [
            (31.07699124, 14.25, 26.48052, 0.03975488, 1.12418043, 1.58130839),
            (40.232036, 14.25, 33.936232, 0.04007624, 1.11804138, 2.06173213),
            (46.35969261, 14.25, 27.13586832, 0.04030344, 1.11376017, 1.5920842),
            (31.07699124, 29.0, 26.48052, 0.22106804, 0.95320005, 5.02180189),
            (40.232036, 29.0, 33.936232, 0.22031404, 0.9504446, 6.27846024),
            (22.27833468, 14.25, 50.639304, 0.03949319, 1.12925336, 3.32139638),
            (52.67898486, 29.0, 78.2994993, 0.21923716, 0.9464763, 13.59290086),
        ];
        for (el, f, r, k, a, g) in rows {
            let c = coefficients(f, 0.0, el);
            assert!(rel(c.k, k) < 1e-6, "k {f} {el}: {} vs {k}", c.k);
            assert!(rel(c.alpha, a) < 1e-6, "alpha {f} {el}: {} vs {a}", c.alpha);
            assert!(rel(gamma_r_db_per_km(f, r, 0.0, el), g) < 1e-6);
        }
    }

    #[test]
    fn polarization_combination() {
        let hv = coefficients_hv(23.0);
        let h = coefficients(23.0, 0.0, 0.0);
        let v = coefficients(23.0, 90.0, 0.0);
        assert!(rel(h.k, hv.k_h) < 1e-12 && (h.alpha - hv.alpha_h).abs() < 1e-12);
        assert!(rel(v.k, hv.k_v) < 1e-12 && (v.alpha - hv.alpha_v).abs() < 1e-12);
        let c = coefficients(23.0, 45.0, 0.0);
        assert!(rel(c.k, (hv.k_h + hv.k_v) / 2.0) < 1e-12);
        // 18 GHz, 42 mm/h, V: 0.07708 * 42^1.0025 ≈ 3.27 dB/km
        let g = gamma_r_db_per_km(18.0, 42.0, 90.0, 0.0);
        assert!((g - 0.07708 * 42f64.powf(1.0025)).abs() < 0.01, "{g}");
        assert_eq!(gamma_r_db_per_km(18.0, 0.0, 90.0, 0.0), 0.0);
    }
}
