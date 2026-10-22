//! ITU-R P.530: propagation prediction for terrestrial line-of-sight links.
//!
//! # Editions
//!
//! * **Multipath fading (§2.3.1)** – two models are provided:
//!   * P.530-17 (12/2017) eqs. (4)–(8): geoclimatic factor from the refractivity gradient
//!     dN1 and terrain roughness sa, `pw = K·d^3.4·(1+|εp|)^-1.03·f^0.8·10^(-0.00076·hL - A/10)`
//!     (detailed) and the "quick planning" variant. These need only two scalar inputs, which
//!     makes them convenient when the ITU digital maps are not available.
//!   * P.530-18 (09/2021) / P.530-19 (09/2025) eqs. (5)–(9): `K` from the `LogK.csv` map
//!     (to be supplied by the caller), mean path clearance hc and the sub-refraction
//!     parameter vsr from `dN75`:
//!     `pw = K·d^3.51·(f²+13)^0.447·10^(−0.376·tanh((hc−147)/125) − 0.334·|εp|^0.39 − 0.00027·hL + 17.85·vsr − A/10)`.
//!     The equations are identical in P.530-18 and P.530-19.
//! * **All fade depths (§2.3.2)** – eqs. (12)–(18), identical in -17/-18/-19.
//! * **Worst month → average year (§2.3.4)** – the P.530-17/-18 ΔG method (eqs. (24),
//!   (25), with the shallow-fading modification of Step 4) and the P.530-19 method
//!   `p = pw/Q` using P.841 with Q1 = 2.85, β = 0.13 ([`super::p841`]).
//! * **Rain (§2.4.1)** – P.530-19: distance factor r of eq. (32) with the -19 input limits
//!   (f ≥ 6 GHz, d ≤ 30 km, R0.01 ≥ 3 mm/h inside eq. (32) only), A0.01 = γR·d·r, and the
//!   C0..C3 time-percentage scaling of eqs. (34)–(36) (C0 = 0.12 below 10 GHz). The P.530-17
//!   variant (r capped at 2.5, no input limits) is also available.
//!
//! Units: distances km, heights m above sea level, frequency GHz, percentages in %.

use serde::{Deserialize, Serialize};

use super::p838;
use super::p841::{annual_from_worst_month, P841Params};

/// Geoclimatic factor K (P.530-17 eq. (4)) for detailed link design:
/// `K = 10^(-4.4 - 0.0027·dN1)·(10 + sa)^-0.46`.
///
/// * `dn1` – point refractivity gradient in the lowest 65 m not exceeded for 1 % of an
///   average year (N-units/km, typically −860…−150; from P.453 maps)
/// * `s_a_m` – area terrain roughness: standard deviation of terrain heights (m) within a
///   110 km × 110 km area (lower limit 1 m applied).
pub fn geoclimatic_factor_k(dn1: f64, s_a_m: f64) -> f64 {
    10f64.powf(-4.4 - 0.0027 * dn1) * (10.0 + s_a_m.max(1.0)).powf(-0.46)
}

/// Geoclimatic factor K for quick planning (P.530-17 eq. (5)): `K = 10^(-4.6 - 0.0027·dN1)`.
pub fn geoclimatic_factor_k_quick(dn1: f64) -> f64 {
    10f64.powf(-4.6 - 0.0027 * dn1)
}

/// Magnitude of the path inclination |εp| (mrad) = |hr − he| / d (heights m, d km).
pub fn path_inclination_mrad(he_m: f64, hr_m: f64, d_km: f64) -> f64 {
    (hr_m - he_m).abs() / d_km
}

/// Mean path terrain clearance hc (m), P.530-18/19 eq. (6):
/// `hc = (hr + he)/2 − d²/102 − ht` where ht is the mean terrain elevation (m AMSL,
/// excluding trees) along the path.
pub fn mean_path_clearance_m(he_m: f64, hr_m: f64, d_km: f64, mean_terrain_amsl_m: f64) -> f64 {
    (hr_m + he_m) / 2.0 - d_km * d_km / 102.0 - mean_terrain_amsl_m
}

/// Sub-refraction parameter vsr, P.530-18/19 eqs. (8), (9):
/// `vsr = min((dN75/50)^1.8·exp(−hc/(2.5·√d)), dN75·d^1.5·f^0.5/24730)`.
pub fn subrefractive_parameter(dn75: f64, hc_m: f64, d_km: f64, f_ghz: f64) -> f64 {
    let dn75 = dn75.max(0.0);
    let v = (dn75 / 50.0).powf(1.8) * (-hc_m / (2.5 * d_km.sqrt())).exp();
    let limit = dn75 * d_km.powf(1.5) * f_ghz.sqrt() / 24_730.0;
    v.min(limit)
}

/// Rough lower frequency limit (GHz) of the multipath model: `fmin = 15/d`.
pub fn multipath_min_frequency_ghz(d_km: f64) -> f64 {
    15.0 / d_km
}

/// Multipath prediction model (P.530 §2.3.1 Step 1–3).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum MultipathModel {
    /// P.530-17 detailed link design: K from eq. (4), pw from eq. (7).
    P530v17 {
        /// dN1 (N-units/km), see [`geoclimatic_factor_k`].
        dn1: f64,
        /// Terrain roughness sa (m).
        s_a_m: f64,
    },
    /// P.530-17 quick planning: K from eq. (5), pw from eq. (8).
    P530v17Quick {
        /// dN1 (N-units/km).
        dn1: f64,
    },
    /// P.530-17 eq. (7) with a measured / externally supplied geoclimatic factor K.
    P530v17WithK {
        /// Geoclimatic factor K.
        k: f64,
    },
    /// P.530-18 / P.530-19 eq. (7).
    P530v18 {
        /// Geoclimatic factor K = 10^(LogK map value) or from local fading data.
        k: f64,
        /// dN75 (N-units), from the `dN75.csv` map (0 in some high-latitude regions).
        dn75: f64,
        /// Mean terrain elevation along the path, excluding trees (m AMSL).
        mean_terrain_amsl_m: f64,
    },
}

/// Path geometry for multipath predictions.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct MultipathGeometry {
    /// Path length d (km).
    pub path_length_km: f64,
    /// Frequency f (GHz).
    pub frequency_ghz: f64,
    /// Transmit (or one end) antenna height he (m above sea level).
    pub he_amsl_m: f64,
    /// Receive (other end) antenna height hr (m above sea level).
    pub hr_amsl_m: f64,
}

impl MultipathGeometry {
    /// |εp| (mrad).
    pub fn inclination_mrad(&self) -> f64 {
        path_inclination_mrad(self.he_amsl_m, self.hr_amsl_m, self.path_length_km)
    }
    /// Lower antenna altitude hL (m).
    pub fn h_lower_m(&self) -> f64 {
        self.he_amsl_m.min(self.hr_amsl_m)
    }
}

/// Multipath occurrence factor p0 (%) = pw at A = 0 dB, P.530 eq. (7) (-17) or eq. (11)
/// (-18/-19). This is the intercept of the deep-fading distribution.
pub fn multipath_occurrence_factor_p0(model: &MultipathModel, g: &MultipathGeometry) -> f64 {
    let d = g.path_length_km;
    let f = g.frequency_ghz;
    let eps = g.inclination_mrad();
    let hl = g.h_lower_m();
    let eq7_v17 = |k: f64| k * d.powf(3.4) * (1.0 + eps).powf(-1.03) * f.powf(0.8) * 10f64.powf(-0.00076 * hl);
    match *model {
        MultipathModel::P530v17 { dn1, s_a_m } => eq7_v17(geoclimatic_factor_k(dn1, s_a_m)),
        MultipathModel::P530v17WithK { k } => eq7_v17(k),
        MultipathModel::P530v17Quick { dn1 } => {
            let k = geoclimatic_factor_k_quick(dn1);
            k * d.powf(3.1) * (1.0 + eps).powf(-1.29) * f.powf(0.8) * 10f64.powf(-0.00089 * hl)
        }
        MultipathModel::P530v18 { k, dn75, mean_terrain_amsl_m } => {
            let hc = mean_path_clearance_m(g.he_amsl_m, g.hr_amsl_m, d, mean_terrain_amsl_m);
            let vsr = subrefractive_parameter(dn75, hc, d, f);
            let expo = -0.376 * ((hc - 147.0) / 125.0).tanh() - 0.334 * eps.powf(0.39) - 0.00027 * hl + 17.85 * vsr;
            k * d.powf(3.51) * (f * f + 13.0).powf(0.447) * 10f64.powf(expo)
        }
    }
}

/// Deep-fade percentage of the average worst month that fade depth `a_db` is exceeded,
/// `pw = p0·10^(−A/10)` (P.530 eq. (7)/(13)). Valid for large fade depths.
pub fn deep_fade_worst_month_pct(p0: f64, a_db: f64) -> f64 {
    p0 * 10f64.powf(-a_db / 10.0)
}

/// Transition fade depth At = 25 + 1.2·log10(p0) (dB), P.530 eq. (12).
pub fn transition_fade_depth_db(p0: f64) -> f64 {
    25.0 + 1.2 * p0.log10()
}

/// Shallow-fading interpolation (P.530 eqs. (15)–(18)) given the transition percentage pt.
fn shallow_fade_pct(pt: f64, at: f64, a: f64) -> f64 {
    let pt = pt.min(99.9999);
    let qa_p = -20.0 * (-((100.0 - pt) / 100.0).ln()).log10() / at;
    let qt = (qa_p - 2.0) / ((1.0 + 0.3 * 10f64.powf(-at / 20.0)) * 10f64.powf(-0.016 * at))
        - 4.3 * (10f64.powf(-at / 20.0) + at / 800.0);
    let qa = 2.0
        + (1.0 + 0.3 * 10f64.powf(-a / 20.0))
            * 10f64.powf(-0.016 * a)
            * (qt + 4.3 * (10f64.powf(-a / 20.0) + a / 800.0));
    100.0 * (1.0 - (-(10f64.powf(-qa * a / 20.0))).exp())
}

/// Percentage of the average worst month that fade depth `a_db` is exceeded, for **any**
/// fade depth (P.530 §2.3.2, eqs. (12)–(18)). `p0` from [`multipath_occurrence_factor_p0`].
pub fn fade_exceedance_worst_month_pct(p0: f64, a_db: f64) -> f64 {
    if p0 <= 0.0 {
        return 0.0;
    }
    let at = transition_fade_depth_db(p0);
    if a_db >= at {
        deep_fade_worst_month_pct(p0, a_db)
    } else {
        let pt = deep_fade_worst_month_pct(p0, at);
        shallow_fade_pct(pt, at, a_db.max(1e-9))
    }
}

/// Fade depth A (dB) exceeded for `pw_percent` of the average worst month (inverse of
/// [`fade_exceedance_worst_month_pct`], by bisection; monotonic for p0 < 2000).
pub fn fade_depth_for_worst_month_pct(p0: f64, pw_percent: f64) -> f64 {
    if p0 <= 0.0 || pw_percent <= 0.0 {
        return f64::INFINITY;
    }
    let (mut lo, mut hi) = (0.0f64, 200.0f64);
    if fade_exceedance_worst_month_pct(p0, lo) <= pw_percent {
        return 0.0;
    }
    for _ in 0..200 {
        let mid = 0.5 * (lo + hi);
        if fade_exceedance_worst_month_pct(p0, mid) > pw_percent {
            lo = mid;
        } else {
            hi = mid;
        }
    }
    0.5 * (lo + hi)
}

/// Logarithmic geoclimatic conversion factor ΔG (dB), P.530-17/-18 eq. (24):
/// `ΔG = 10.5 − 5.6·log10(1.1 ± |cos 2ξ|^0.7) − 2.7·log10 d + 1.7·log10(1 + |εp|)`,
/// limited to ≤ 10.8 dB; "+" for |ξ| ≤ 45°, "−" for |ξ| > 45° (ξ latitude, degrees).
pub fn delta_g_db(latitude_deg: f64, d_km: f64, eps_p_mrad: f64) -> f64 {
    let xi = latitude_deg.abs();
    let c = (2.0 * xi).to_radians().cos().abs().powf(0.7);
    let inner = if xi <= 45.0 { 1.1 + c } else { 1.1 - c };
    let dg = 10.5 - 5.6 * inner.log10() - 2.7 * d_km.log10() + 1.7 * (1.0 + eps_p_mrad).log10();
    dg.min(10.8)
}

/// Method to convert worst-month multipath statistics to average-year statistics.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum WorstMonthConversion {
    /// P.530-17/-18 §2.3.4 ΔG method (needs path-centre latitude).
    DeltaG {
        /// Latitude of the path centre (degrees, sign ignored).
        latitude_deg: f64,
    },
    /// P.530-19 §2.3.4: `p = pw/Q` with P.841 parameters (global: Q1 = 2.85, β = 0.13).
    P841(P841Params),
}

impl Default for WorstMonthConversion {
    fn default() -> Self {
        WorstMonthConversion::P841(P841Params::GLOBAL)
    }
}

/// Percentage of an **average year** that fade depth `a_db` is exceeded due to multipath,
/// using the all-fade-depth method and the chosen worst-month conversion.
pub fn fade_exceedance_annual_pct(
    p0: f64,
    a_db: f64,
    conversion: &WorstMonthConversion,
    d_km: f64,
    eps_p_mrad: f64,
) -> f64 {
    if p0 <= 0.0 {
        return 0.0;
    }
    match conversion {
        WorstMonthConversion::P841(params) => {
            annual_from_worst_month(fade_exceedance_worst_month_pct(p0, a_db), params)
        }
        WorstMonthConversion::DeltaG { latitude_deg } => {
            let factor = 10f64.powf(-delta_g_db(*latitude_deg, d_km, eps_p_mrad) / 10.0);
            let at = transition_fade_depth_db(p0);
            if a_db >= at {
                factor * deep_fade_worst_month_pct(p0, a_db)
            } else {
                // Step 4: annual pt used in eq. (15); result of eq. (18) is the annual value.
                let pt_annual = factor * deep_fade_worst_month_pct(p0, at);
                shallow_fade_pct(pt_annual, at, a_db.max(1e-9))
            }
        }
    }
}

/// Coefficients C1, C2, C3 of P.530 eqs. (35a)–(35c), with C0 from eq. (36).
pub fn rain_scaling_coefficients(f_ghz: f64) -> (f64, f64, f64) {
    let c0 = if f_ghz >= 10.0 { 0.12 + 0.4 * (f_ghz / 10.0).log10().powf(0.8) } else { 0.12 };
    let c1 = 0.07f64.powf(c0) * 0.12f64.powf(1.0 - c0);
    let c2 = 0.855 * c0 + 0.546 * (1.0 - c0);
    let c3 = 0.139 * c0 + 0.043 * (1.0 - c0);
    (c1, c2, c3)
}

/// Rain distance factor r, P.530-19 eq. (32), with the -19 input limits applied inside the
/// formula (f ≥ 6 GHz, d ≤ 30 km, R0.01 ≥ 3 mm/h):
/// `r = 1 / (0.477·d^0.633·R0.01^(0.073·α)·f^0.123 − 10.579·(1 − exp(−0.024·d)))`.
pub fn rain_distance_factor_r(d_km: f64, f_ghz: f64, r001: f64, alpha: f64) -> f64 {
    let d = d_km.min(30.0);
    let f = f_ghz.max(6.0);
    let r = r001.max(3.0);
    let den = 0.477 * d.powf(0.633) * r.powf(0.073 * alpha) * f.powf(0.123) - 10.579 * (1.0 - (-0.024 * d).exp());
    if den <= 0.0 {
        return 2.5;
    }
    1.0 / den
}

/// Rain distance factor r per P.530-17 eq. (32): no input limits; maximum r = 2.5
/// (i.e. r = 2.5 if the denominator is below 0.4).
pub fn rain_distance_factor_r_v17(d_km: f64, f_ghz: f64, r001: f64, alpha: f64) -> f64 {
    let den = 0.477 * d_km.powf(0.633) * r001.powf(0.073 * alpha) * f_ghz.powf(0.123)
        - 10.579 * (1.0 - (-0.024 * d_km).exp());
    if den < 0.4 {
        2.5
    } else {
        1.0 / den
    }
}

/// Intermediate values of the rain attenuation calculation.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct RainAttenuation001 {
    /// R0.01 used (mm/h).
    pub r001_mm_h: f64,
    /// Specific attenuation γR at R0.01 (dB/km).
    pub gamma_r_db_per_km: f64,
    /// P.838 exponent α.
    pub alpha: f64,
    /// Distance factor r.
    pub distance_factor: f64,
    /// Effective path length d·r (km).
    pub effective_path_km: f64,
    /// Attenuation exceeded for 0.01 % of an average year (dB).
    pub a001_db: f64,
}

fn a001_with(
    d_km: f64,
    f_ghz: f64,
    r001: f64,
    pol_tilt_deg: f64,
    elevation_deg: f64,
    rfn: fn(f64, f64, f64, f64) -> f64,
) -> RainAttenuation001 {
    let c = p838::coefficients(f_ghz, pol_tilt_deg, elevation_deg);
    let gamma = if r001 > 0.0 { c.k * r001.powf(c.alpha) } else { 0.0 };
    let r = rfn(d_km, f_ghz, r001, c.alpha);
    RainAttenuation001 {
        r001_mm_h: r001,
        gamma_r_db_per_km: gamma,
        alpha: c.alpha,
        distance_factor: r,
        effective_path_km: d_km * r,
        a001_db: gamma * d_km * r,
    }
}

/// A0.01 (dB) and intermediate values, P.530-19 §2.4.1 Steps 1–4.
///
/// * `d_km` path length, `f_ghz` frequency, `r001` rain rate exceeded 0.01 % (mm/h)
/// * `pol_tilt_deg` polarization tilt (0 H, 90 V, 45 circular), `elevation_deg` path elevation
pub fn rain_attenuation_001(
    d_km: f64,
    f_ghz: f64,
    r001: f64,
    pol_tilt_deg: f64,
    elevation_deg: f64,
) -> RainAttenuation001 {
    a001_with(d_km, f_ghz, r001, pol_tilt_deg, elevation_deg, rain_distance_factor_r)
}

/// A0.01 per P.530-17 (distance factor without the -19 input limits, r ≤ 2.5).
pub fn rain_attenuation_001_v17(
    d_km: f64,
    f_ghz: f64,
    r001: f64,
    pol_tilt_deg: f64,
    elevation_deg: f64,
) -> RainAttenuation001 {
    a001_with(d_km, f_ghz, r001, pol_tilt_deg, elevation_deg, rain_distance_factor_r_v17)
}

/// Rain attenuation (dB) exceeded for `p_percent` of an average year from A0.01,
/// P.530 eq. (34): `Ap = A0.01·C1·p^−(C2 + C3·log10 p)`. Valid 0.001 % ≤ p ≤ 1 %
/// (values outside are extrapolated).
pub fn rain_attenuation_db(a001_db: f64, f_ghz: f64, p_percent: f64) -> f64 {
    let (c1, c2, c3) = rain_scaling_coefficients(f_ghz);
    a001_db * c1 * p_percent.powf(-(c2 + c3 * p_percent.log10()))
}

/// Percentage of an average year (%) that rain attenuation `a_db` is exceeded: analytical
/// inverse of eq. (34) (quadratic in log10 p). Returns 100 for `a_db ≤ 0`, 0 if `a001_db`
/// is 0 or `a_db` lies beyond the maximum of the power law. Results outside 0.001–1 % are
/// extrapolations.
pub fn rain_exceedance_pct(a001_db: f64, f_ghz: f64, a_db: f64) -> f64 {
    if a_db <= 0.0 {
        return 100.0;
    }
    if a001_db <= 0.0 {
        return 0.0;
    }
    let (c1, c2, c3) = rain_scaling_coefficients(f_ghz);
    let l = (a_db / (a001_db * c1)).log10();
    let disc = c2 * c2 - 4.0 * c3 * l;
    if disc < 0.0 {
        return 0.0;
    }
    let x = (-c2 + disc.sqrt()) / (2.0 * c3);
    10f64.powf(x).min(100.0)
}

/// Average number per year of multipath attenuation events lasting 10 s or longer,
/// P.530-19 §2.3.8 eq. (31): `N10s = 3650·p(A)^0.95`, with p(A) the annual exceedance
/// percentage of the fade depth (%).
pub fn multipath_events_10s_per_year(p_annual_percent: f64) -> f64 {
    3650.0 * p_annual_percent.max(0.0).powf(0.95)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rel(a: f64, b: f64) -> f64 {
        ((a - b) / b).abs()
    }

    #[test]
    fn geoclimatic_and_inclination() {
        // dN1 = -400, sa = 50 → K = 10^(-4.4+1.08)·60^-0.46
        let k = geoclimatic_factor_k(-400.0, 50.0);
        let expected = 10f64.powf(-3.32) * 60f64.powf(-0.46);
        assert!(rel(k, expected) < 1e-12);
        assert!((k - 7.279e-5).abs() < 0.001e-5, "{k}");
        assert!(rel(geoclimatic_factor_k_quick(-400.0), 10f64.powf(-3.52)) < 1e-12);
        assert!((path_inclination_mrad(100.0, 400.0, 30.0) - 10.0).abs() < 1e-12);
        // sa lower limit of 1 m
        assert_eq!(geoclimatic_factor_k(-300.0, 0.0), geoclimatic_factor_k(-300.0, 1.0));
        assert!((multipath_min_frequency_ghz(30.0) - 0.5).abs() < 1e-12);
    }

    #[test]
    fn multipath_p530v17_eq7() {
        // K from dN1=-400, sa=50; d=40 km, f=6 GHz, he=300, hr=350
        let g = MultipathGeometry { path_length_km: 40.0, frequency_ghz: 6.0, he_amsl_m: 300.0, hr_amsl_m: 350.0 };
        let m = MultipathModel::P530v17 { dn1: -400.0, s_a_m: 50.0 };
        let p0 = multipath_occurrence_factor_p0(&m, &g);
        let k = geoclimatic_factor_k(-400.0, 50.0);
        let eps: f64 = 50.0 / 40.0;
        let manual = k * 40f64.powf(3.4) * (1.0 + eps).powf(-1.03) * 6f64.powf(0.8) * 10f64.powf(-0.00076 * 300.0);
        assert!(rel(p0, manual) < 1e-12);
        // ≈ 21.9 %
        assert!((p0 - 21.92).abs() < 0.01, "{p0}");
        // 35 dB deep fade → p0·10^-3.5
        assert!(rel(deep_fade_worst_month_pct(p0, 35.0), p0 * 10f64.powf(-3.5)) < 1e-12);
        let mk = MultipathModel::P530v17WithK { k };
        assert!(rel(multipath_occurrence_factor_p0(&mk, &g), p0) < 1e-12);
    }

    #[test]
    fn multipath_p530v18_eq7() {
        let g = MultipathGeometry { path_length_km: 40.0, frequency_ghz: 6.0, he_amsl_m: 300.0, hr_amsl_m: 350.0 };
        let m = MultipathModel::P530v18 { k: 10f64.powf(-4.0), dn75: 20.0, mean_terrain_amsl_m: 150.0 };
        let p0 = multipath_occurrence_factor_p0(&m, &g);
        let hc = 325.0 - 1600.0 / 102.0 - 150.0;
        let vsr = ((20.0f64 / 50.0).powf(1.8) * (-hc / (2.5 * 40f64.sqrt())).exp())
            .min(20.0 * 40f64.powf(1.5) * 6f64.sqrt() / 24730.0);
        let eps: f64 = 1.25;
        let manual = 1e-4
            * 40f64.powf(3.51)
            * 49f64.powf(0.447)
            * 10f64
                .powf(-0.376 * ((hc - 147.0) / 125.0).tanh() - 0.334 * eps.powf(0.39) - 0.00027 * 300.0 + 17.85 * vsr);
        assert!(rel(p0, manual) < 1e-12);
        assert!(rel(mean_path_clearance_m(300.0, 350.0, 40.0, 150.0), hc) < 1e-12);
    }

    #[test]
    fn all_fade_depths_continuity() {
        for p0 in [0.1, 1.0, 10.0, 100.0, 1000.0] {
            let at = transition_fade_depth_db(p0);
            let below = fade_exceedance_worst_month_pct(p0, at - 1e-7);
            let above = fade_exceedance_worst_month_pct(p0, at + 1e-7);
            assert!(rel(below, above) < 1e-4, "p0={p0}: {below} vs {above}");
            // monotonic decreasing in A
            let mut last = f64::INFINITY;
            for a in 0..60 {
                let p = fade_exceedance_worst_month_pct(p0, a as f64);
                assert!(p <= last + 1e-12);
                last = p;
            }
            // For A → 0 eq. (18) tends to 100·(1 − e^-1) = 63.2 %
            let p_small = fade_exceedance_worst_month_pct(p0, 1e-6);
            assert!(p_small > 60.0 && p_small <= 100.0);
            // inverse
            let a = fade_depth_for_worst_month_pct(p0, 0.01);
            assert!(rel(fade_exceedance_worst_month_pct(p0, a), 0.01) < 1e-6);
        }
    }

    #[test]
    fn delta_g_conversion() {
        // ξ = 45°: cos 90° = 0 → ΔG = 10.5 − 5.6 log 1.1 − 2.7 log d + 1.7 log(1+ε)
        let dg = delta_g_db(45.0, 40.0, 1.25);
        let manual = 10.5 - 5.6 * 1.1f64.log10() - 2.7 * 40f64.log10() + 1.7 * 2.25f64.log10();
        assert!((dg - manual).abs() < 1e-9);
        // cap 10.8 dB
        assert_eq!(delta_g_db(70.0, 1.0, 0.0), 10.8);
        let p0 = 50.0;
        let conv = WorstMonthConversion::DeltaG { latitude_deg: 45.0 };
        let pa = fade_exceedance_annual_pct(p0, 40.0, &conv, 40.0, 1.25);
        assert!(rel(pa, 10f64.powf(-dg / 10.0) * p0 * 1e-4) < 1e-12);
        // P.841 global: deep fade annual ≈ 0.30·pw^1.15
        let pw = p0 * 1e-4;
        let pa841 = fade_exceedance_annual_pct(p0, 40.0, &WorstMonthConversion::default(), 40.0, 1.25);
        assert!(rel(pa841, 0.30 * pw.powf(1.15)) < 0.02);
    }

    #[test]
    fn rain_scaling_known_values() {
        // Below 10 GHz: C0 = 0.12
        let (c1, c2, c3) = rain_scaling_coefficients(8.0);
        assert!(rel(c1, 0.07f64.powf(0.12) * 0.12f64.powf(0.88)) < 1e-12);
        assert!(rel(c2, 0.855 * 0.12 + 0.546 * 0.88) < 1e-12);
        assert!(rel(c3, 0.139 * 0.12 + 0.043 * 0.88) < 1e-12);
        // Ap(0.01) = A0.01 for all frequencies (C1·0.01^-(C2-2C3) ≈ 1)
        for f in [5.0, 10.0, 18.0, 23.0, 38.0, 80.0] {
            let r = rain_attenuation_db(10.0, f, 0.01);
            assert!((r - 10.0).abs() < 0.1, "f={f}: {r}");
        }
    }

    #[test]
    fn rain_attenuation_worked_example() {
        // 18 GHz, V-pol, 20 km, R0.01 = 42 mm/h (zone K)
        let r = rain_attenuation_001(20.0, 18.0, 42.0, 90.0, 0.0);
        let c = p838::coefficients(18.0, 90.0, 0.0);
        let gamma = c.k * 42f64.powf(c.alpha);
        let den = 0.477 * 20f64.powf(0.633) * 42f64.powf(0.073 * c.alpha) * 18f64.powf(0.123)
            - 10.579 * (1.0 - (-0.024f64 * 20.0).exp());
        assert!(rel(r.gamma_r_db_per_km, gamma) < 1e-12);
        assert!(rel(r.distance_factor, 1.0 / den) < 1e-12);
        assert!(rel(r.a001_db, gamma * 20.0 / den) < 1e-12);
        // values: γ ≈ 3.27 dB/km, r ≈ 0.519, A0.01 ≈ 33.9 dB
        assert!((r.gamma_r_db_per_km - 3.27).abs() < 0.02, "{}", r.gamma_r_db_per_km);
        assert!((r.distance_factor - 0.519).abs() < 0.002, "{}", r.distance_factor);
        assert!((r.a001_db - 33.9).abs() < 0.2, "{}", r.a001_db);
        // v17 identical for this input (within limits, denominator > 0.4)
        let r17 = rain_attenuation_001_v17(20.0, 18.0, 42.0, 90.0, 0.0);
        assert!(rel(r17.a001_db, r.a001_db) < 1e-12);
        // inverse of eq. (34)
        for p in [0.001, 0.003, 0.01, 0.1, 1.0] {
            let a = rain_attenuation_db(r.a001_db, 18.0, p);
            let back = rain_exceedance_pct(r.a001_db, 18.0, a);
            assert!(rel(back, p) < 1e-9, "p={p}: {back}");
        }
        assert_eq!(rain_exceedance_pct(r.a001_db, 18.0, 0.0), 100.0);
        assert_eq!(rain_exceedance_pct(0.0, 18.0, 10.0), 0.0);
    }

    #[test]
    fn rain_r_limits_v19() {
        // long path: d limited to 30 km in eq. (32)
        assert!(
            rel(rain_distance_factor_r(60.0, 20.0, 50.0, 1.0), rain_distance_factor_r(30.0, 20.0, 50.0, 1.0)) < 1e-12
        );
        // low frequency limited to 6 GHz, low rain rate to 3 mm/h
        assert_eq!(rain_distance_factor_r(10.0, 2.0, 1.0, 1.0), rain_distance_factor_r(10.0, 6.0, 3.0, 1.0));
        assert_eq!(rain_distance_factor_r_v17(30.0, 6.0, 3.0, 1.0), 2.5);
        assert!(multipath_events_10s_per_year(0.01) > 0.0);
    }
}
