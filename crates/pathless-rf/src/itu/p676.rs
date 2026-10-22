//! ITU-R P.676-13 (08/2022): attenuation by atmospheric gases.
//!
//! Since P.676-12 the Recommendation no longer contains the closed-form Annex 2
//! approximations for the specific attenuations γo and γw that older editions (≤ P.676-11)
//! had; Annex 2 now uses the Annex 1 specific attenuation directly. This module therefore
//! implements the **Annex 1 line-by-line method** (equations (1)–(8)) with the
//! spectroscopic data of Tables 1 (44 oxygen lines) and 2 (35 water-vapour lines incl. the
//! 1780 GHz pseudo-line), valid 1–1000 GHz. The tables were taken from the ITU-Rpy data
//! files `v13_lines_oxygen.txt` / `v13_lines_water_vapour.txt` and checked against the
//! Recommendation; results are validated against the ITU-R SG3 validation set
//! (`ITURP676-13_gamma.csv`).
//!
//! Terrestrial path attenuation is `A = γ·d` (P.530 eq. (1)).

use serde::{Deserialize, Serialize};

/// Surface atmospheric conditions.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Atmosphere {
    /// Dry-air pressure p (hPa). (Total barometric pressure = p + e.) Default 1013.25.
    pub pressure_hpa: f64,
    /// Temperature (°C). Default 15 °C.
    pub temperature_c: f64,
    /// Water-vapour density ρ (g/m³). Default 7.5 g/m³.
    pub water_vapour_density_g_m3: f64,
}

impl Default for Atmosphere {
    /// Reference atmosphere of P.676 Fig. 1: 1013.25 hPa, 15 °C, 7.5 g/m³.
    fn default() -> Self {
        Self { pressure_hpa: 1013.25, temperature_c: 15.0, water_vapour_density_g_m3: 7.5 }
    }
}

impl Atmosphere {
    /// Water-vapour partial pressure e = ρ·T / 216.7 (hPa), eq. (4).
    pub fn water_vapour_pressure_hpa(&self) -> f64 {
        self.water_vapour_density_g_m3 * self.temperature_k() / 216.7
    }

    /// Temperature in kelvin.
    pub fn temperature_k(&self) -> f64 {
        self.temperature_c + 273.15
    }
}

/// Specific attenuations (dB/km).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct GaseousAttenuation {
    /// Dry air (oxygen + pressure-induced nitrogen + Debye) γo (dB/km).
    pub oxygen_db_per_km: f64,
    /// Water vapour γw (dB/km).
    pub water_vapour_db_per_km: f64,
    /// Total γ = γo + γw (dB/km).
    pub total_db_per_km: f64,
}

// Table 1: f0 (GHz), a1, a2, a3, a4, a5, a6
// Table 2: f0 (GHz), b1, b2, b3, b4, b5, b6
const OXYGEN_LINES: [[f64; 7]; 44] = [
    [50.474214, 0.975, 9.651, 6.69, 0.0, 2.566, 6.85],
    [50.987745, 2.529, 8.653, 7.17, 0.0, 2.246, 6.8],
    [51.50336, 6.193, 7.709, 7.64, 0.0, 1.947, 6.729],
    [52.021429, 14.32, 6.819, 8.11, 0.0, 1.667, 6.64],
    [52.542418, 31.24, 5.983, 8.58, 0.0, 1.388, 6.526],
    [53.066934, 64.29, 5.201, 9.06, 0.0, 1.349, 6.206],
    [53.595775, 124.6, 4.474, 9.55, 0.0, 2.227, 5.085],
    [54.130025, 227.3, 3.8, 9.96, 0.0, 3.17, 3.75],
    [54.67118, 389.7, 3.182, 10.37, 0.0, 3.558, 2.654],
    [55.221384, 627.1, 2.618, 10.89, 0.0, 2.56, 2.952],
    [55.783815, 945.3, 2.109, 11.34, 0.0, -1.172, 6.135],
    [56.264774, 543.4, 0.014, 17.03, 0.0, 3.525, -0.978],
    [56.363399, 1331.8, 1.654, 11.89, 0.0, -2.378, 6.547],
    [56.968211, 1746.6, 1.255, 12.23, 0.0, -3.545, 6.451],
    [57.612486, 2120.1, 0.91, 12.62, 0.0, -5.416, 6.056],
    [58.323877, 2363.7, 0.621, 12.95, 0.0, -1.932, 0.436],
    [58.446588, 1442.1, 0.083, 14.91, 0.0, 6.768, -1.273],
    [59.164204, 2379.9, 0.387, 13.53, 0.0, -6.561, 2.309],
    [59.590983, 2090.7, 0.207, 14.08, 0.0, 6.957, -0.776],
    [60.306056, 2103.4, 0.207, 14.15, 0.0, -6.395, 0.699],
    [60.434778, 2438.0, 0.386, 13.39, 0.0, 6.342, -2.825],
    [61.150562, 2479.5, 0.621, 12.92, 0.0, 1.014, -0.584],
    [61.800158, 2275.9, 0.91, 12.63, 0.0, 5.014, -6.619],
    [62.41122, 1915.4, 1.255, 12.17, 0.0, 3.029, -6.759],
    [62.486253, 1503.0, 0.083, 15.13, 0.0, -4.499, 0.844],
    [62.997984, 1490.2, 1.654, 11.74, 0.0, 1.856, -6.675],
    [63.568526, 1078.0, 2.108, 11.34, 0.0, 0.658, -6.139],
    [64.127775, 728.7, 2.617, 10.88, 0.0, -3.036, -2.895],
    [64.67891, 461.3, 3.181, 10.38, 0.0, -3.968, -2.59],
    [65.224078, 274.0, 3.8, 9.96, 0.0, -3.528, -3.68],
    [65.764779, 153.0, 4.473, 9.55, 0.0, -2.548, -5.002],
    [66.302096, 80.4, 5.2, 9.06, 0.0, -1.66, -6.091],
    [66.836834, 39.8, 5.982, 8.58, 0.0, -1.68, -6.393],
    [67.369601, 18.56, 6.818, 8.11, 0.0, -1.956, -6.475],
    [67.900868, 8.172, 7.708, 7.64, 0.0, -2.216, -6.545],
    [68.431006, 3.397, 8.652, 7.17, 0.0, -2.492, -6.6],
    [68.960312, 1.334, 9.65, 6.69, 0.0, -2.773, -6.65],
    [118.750334, 940.3, 0.01, 16.64, 0.0, -0.439, 0.079],
    [368.498246, 67.4, 0.048, 16.4, 0.0, 0.0, 0.0],
    [424.76302, 637.7, 0.044, 16.4, 0.0, 0.0, 0.0],
    [487.249273, 237.4, 0.049, 16.0, 0.0, 0.0, 0.0],
    [715.392902, 98.1, 0.145, 16.0, 0.0, 0.0, 0.0],
    [773.83949, 572.3, 0.141, 16.2, 0.0, 0.0, 0.0],
    [834.145546, 183.1, 0.145, 14.7, 0.0, 0.0, 0.0],
];
const WATER_LINES: [[f64; 7]; 35] = [
    [22.23508, 0.1079, 2.144, 26.38, 0.76, 5.087, 1.0],
    [67.80396, 0.0011, 8.732, 28.58, 0.69, 4.93, 0.82],
    [119.99594, 0.0007, 8.353, 29.48, 0.7, 4.78, 0.79],
    [183.310087, 2.273, 0.668, 29.06, 0.77, 5.022, 0.85],
    [321.22563, 0.047, 6.179, 24.04, 0.67, 4.398, 0.54],
    [325.152888, 1.514, 1.541, 28.23, 0.64, 4.893, 0.74],
    [336.227764, 0.001, 9.825, 26.93, 0.69, 4.74, 0.61],
    [380.197353, 11.67, 1.048, 28.11, 0.54, 5.063, 0.89],
    [390.134508, 0.0045, 7.347, 21.52, 0.63, 4.81, 0.55],
    [437.346667, 0.0632, 5.048, 18.45, 0.6, 4.23, 0.48],
    [439.150807, 0.9098, 3.595, 20.07, 0.63, 4.483, 0.52],
    [443.018343, 0.192, 5.048, 15.55, 0.6, 5.083, 0.5],
    [448.001085, 10.41, 1.405, 25.64, 0.66, 5.028, 0.67],
    [470.888999, 0.3254, 3.597, 21.34, 0.66, 4.506, 0.65],
    [474.689092, 1.26, 2.379, 23.2, 0.65, 4.804, 0.64],
    [488.490108, 0.2529, 2.852, 25.86, 0.69, 5.201, 0.72],
    [503.568532, 0.0372, 6.731, 16.12, 0.61, 3.98, 0.43],
    [504.482692, 0.0124, 6.731, 16.12, 0.61, 4.01, 0.45],
    [547.67644, 0.9785, 0.158, 26.0, 0.7, 4.5, 1.0],
    [552.02096, 0.184, 0.158, 26.0, 0.7, 4.5, 1.0],
    [556.935985, 497.0, 0.159, 30.86, 0.69, 4.552, 1.0],
    [620.700807, 5.015, 2.391, 24.38, 0.71, 4.856, 0.68],
    [645.766085, 0.0067, 8.633, 18.0, 0.6, 4.0, 0.5],
    [658.00528, 0.2732, 7.816, 32.1, 0.69, 4.14, 1.0],
    [752.033113, 243.4, 0.396, 30.86, 0.68, 4.352, 0.84],
    [841.051732, 0.0134, 8.177, 15.9, 0.33, 5.76, 0.45],
    [859.965698, 0.1325, 8.055, 30.6, 0.68, 4.09, 0.84],
    [899.303175, 0.0547, 7.914, 29.85, 0.68, 4.53, 0.9],
    [902.611085, 0.0386, 8.429, 28.65, 0.7, 5.1, 0.95],
    [906.205957, 0.1836, 5.11, 24.08, 0.7, 4.7, 0.53],
    [916.171582, 8.4, 1.441, 26.73, 0.7, 5.15, 0.78],
    [923.112692, 0.0079, 10.293, 29.0, 0.7, 5.0, 0.8],
    [970.315022, 9.009, 1.919, 25.5, 0.64, 4.94, 0.67],
    [987.926764, 134.6, 0.257, 29.85, 0.68, 4.55, 0.9],
    [1780.0, 17506.0, 0.952, 196.3, 2.0, 24.15, 5.0],
];

/// Dry-air specific attenuation γo (dB/km), P.676-13 Annex 1 eqs. (1)–(8).
///
/// `f_ghz` 1–1000 GHz.
pub fn gamma_oxygen_db_per_km(f_ghz: f64, atm: &Atmosphere) -> f64 {
    let f = f_ghz;
    let t = atm.temperature_k();
    let theta = 300.0 / t;
    let p = atm.pressure_hpa;
    let e = atm.water_vapour_pressure_hpa();
    let mut n_pp = 0.0;
    for l in OXYGEN_LINES.iter() {
        let (fi, a1, a2, a3, a4, a5, a6) = (l[0], l[1], l[2], l[3], l[4], l[5], l[6]);
        let si = a1 * 1e-7 * p * theta.powi(3) * (a2 * (1.0 - theta)).exp();
        let mut df = a3 * 1e-4 * (p * theta.powf(0.8 - a4) + 1.1 * e * theta);
        df = (df * df + 2.25e-6).sqrt();
        let delta = (a5 + a6 * theta) * 1e-4 * (p + e) * theta.powf(0.8);
        let fi_shape = f / fi
            * ((df - delta * (fi - f)) / ((fi - f).powi(2) + df * df)
                + (df - delta * (fi + f)) / ((fi + f).powi(2) + df * df));
        n_pp += si * fi_shape;
    }
    // eq. (8): dry continuum
    let d = 5.6e-4 * (p + e) * theta.powf(0.8);
    let nd = f
        * p
        * theta
        * theta
        * (6.14e-5 / (d * (1.0 + (f / d).powi(2))) + 1.4e-12 * p * theta.powf(1.5) / (1.0 + 1.9e-5 * f.powf(1.5)));
    0.1820 * f * (n_pp + nd)
}

/// Water-vapour specific attenuation γw (dB/km), P.676-13 Annex 1 eqs. (1)–(7).
pub fn gamma_water_vapour_db_per_km(f_ghz: f64, atm: &Atmosphere) -> f64 {
    let f = f_ghz;
    let t = atm.temperature_k();
    let theta = 300.0 / t;
    let p = atm.pressure_hpa;
    let e = atm.water_vapour_pressure_hpa();
    if e <= 0.0 {
        return 0.0;
    }
    let mut n_pp = 0.0;
    for l in WATER_LINES.iter() {
        let (fi, b1, b2, b3, b4, b5, b6) = (l[0], l[1], l[2], l[3], l[4], l[5], l[6]);
        let si = b1 * 1e-1 * e * theta.powf(3.5) * (b2 * (1.0 - theta)).exp();
        let mut df = b3 * 1e-4 * (p * theta.powf(b4) + b5 * e * theta.powf(b6));
        df = 0.535 * df + (0.217 * df * df + 2.1316e-12 * fi * fi / theta).sqrt();
        let fi_shape = f / fi * (df / ((fi - f).powi(2) + df * df) + df / ((fi + f).powi(2) + df * df));
        n_pp += si * fi_shape;
    }
    0.1820 * f * n_pp
}

/// γo, γw and γ (dB/km) at `f_ghz`.
pub fn specific_attenuation(f_ghz: f64, atm: &Atmosphere) -> GaseousAttenuation {
    let o = gamma_oxygen_db_per_km(f_ghz, atm);
    let w = gamma_water_vapour_db_per_km(f_ghz, atm);
    GaseousAttenuation { oxygen_db_per_km: o, water_vapour_db_per_km: w, total_db_per_km: o + w }
}

/// Total specific attenuation γ = γo + γw (dB/km).
pub fn gamma_total_db_per_km(f_ghz: f64, atm: &Atmosphere) -> f64 {
    gamma_oxygen_db_per_km(f_ghz, atm) + gamma_water_vapour_db_per_km(f_ghz, atm)
}

/// Gaseous attenuation (dB) on a horizontal terrestrial path of `d_km`: A = γ·d.
pub fn terrestrial_path_attenuation_db(f_ghz: f64, d_km: f64, atm: &Atmosphere) -> f64 {
    gamma_total_db_per_km(f_ghz, atm) * d_km
}

#[cfg(test)]
mod tests {
    use super::*;

    /// ITU-R SG3 validation data, ITURP676-13_gamma.csv (p = 1013.25 hPa, T = 288.15 K,
    /// ρ = 7.5 g/m³): f, γo, γw.
    const VALIDATION: [(f64, f64, f64); 20] = [
        (1.0, 0.00538865816790655, 5.09046173249644e-05),
        (5.0, 0.00740042625629061, 0.00131345892117269),
        (10.0, 0.00822441670270988, 0.00597412524547672),
        (12.0, 0.00869826406877357, 0.00953538822024593),
        (20.0, 0.0118835504778076, 0.0970473048151117),
        (22.0, 0.0131302229653917, 0.17420703333692),
        (30.0, 0.0214496842395166, 0.0723748630251885),
        (50.0, 0.277268638530969, 0.111158547270094),
        (57.0, 10.0652376721386, 0.140613831253581),
        (60.0, 14.6234747964861, 0.154841840636247),
        (70.0, 0.304105097147173, 0.209887930364388),
        (80.0, 0.0723074953269855, 0.270060121575656),
        (100.0, 0.033625442078135, 0.424433522672505),
        (118.0, 1.13486620187051, 0.605921952594212),
        (150.0, 0.0143469132743067, 1.10921762592395),
        (183.0, 0.0127339088358709, 27.6650083141665),
        (200.0, 0.0137235611329728, 2.87482447791876),
        (300.0, 0.0257595762819792, 5.22132904110494),
        (325.0, 0.0300989585941093, 37.8621105311468),
        (350.0, 0.0368056270504571, 10.1226731395126),
    ];

    #[test]
    fn itu_validation_set() {
        let atm = Atmosphere::default();
        assert!((atm.temperature_k() - 288.15).abs() < 1e-12);
        for (f, go, gw) in VALIDATION {
            let r = specific_attenuation(f, &atm);
            assert!(((r.oxygen_db_per_km - go) / go).abs() < 1e-6, "γo {f}: {} vs {go}", r.oxygen_db_per_km);
            assert!(
                ((r.water_vapour_db_per_km - gw) / gw).abs() < 1e-6,
                "γw {f}: {} vs {gw}",
                r.water_vapour_db_per_km
            );
            assert!((r.total_db_per_km - (go + gw)).abs() < 1e-6 * (go + gw));
        }
    }

    #[test]
    fn path_attenuation_and_dry_air() {
        let atm = Atmosphere::default();
        let a = terrestrial_path_attenuation_db(23.0, 10.0, &atm);
        assert!((a - 10.0 * gamma_total_db_per_km(23.0, &atm)).abs() < 1e-12);
        let dry = Atmosphere { water_vapour_density_g_m3: 0.0, ..atm };
        assert_eq!(gamma_water_vapour_db_per_km(22.235, &dry), 0.0);
        // more humidity → more attenuation at the 22 GHz line
        let wet = Atmosphere { water_vapour_density_g_m3: 15.0, ..atm };
        assert!(gamma_total_db_per_km(22.235, &wet) > gamma_total_db_per_km(22.235, &atm));
    }
}
