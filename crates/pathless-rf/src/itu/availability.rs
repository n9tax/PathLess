//! Convenience microwave link availability calculation (multipath + rain), combining
//! P.530, P.838, P.837 and P.841.
//!
//! Following P.530 §2.4, the annual exceedance percentages of the fade margin due to
//! multipath and due to rain are **added** to obtain the combined figure. Note that in
//! ITU-T G.82x terms clear-air multipath outages count towards *performance* (SES) and rain
//! outages towards *unavailability*; both are reported separately so the caller can present
//! either convention.

use serde::{Deserialize, Serialize};

use super::p530::{
    fade_exceedance_annual_pct, fade_exceedance_worst_month_pct, multipath_occurrence_factor_p0, path_inclination_mrad,
    rain_attenuation_001, rain_attenuation_001_v17, rain_exceedance_pct, MultipathGeometry, MultipathModel,
    WorstMonthConversion,
};
use super::p837::RainRate;
use super::p841::{worst_month_from_annual, P841Params};
use super::{percent_to_seconds_per_year, Polarization};
use crate::error::{require_finite, require_positive, RfError};

/// Edition of the P.530 rain distance factor to use.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum RainMethod {
    /// P.530-19 (input limits in eq. (32)).
    #[default]
    P530v19,
    /// P.530-17 (r ≤ 2.5, no input limits).
    P530v17,
}

/// Inputs of [`microwave_availability`].
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct MicrowaveLinkInput {
    /// Frequency (GHz).
    pub frequency_ghz: f64,
    /// Path length (km).
    pub path_length_km: f64,
    /// Antenna height at end A above mean sea level (m).
    pub tx_antenna_amsl_m: f64,
    /// Antenna height at end B above mean sea level (m).
    pub rx_antenna_amsl_m: f64,
    /// Flat fade margin (dB): received level minus receiver threshold.
    pub fade_margin_db: f64,
    /// Polarization (affects rain attenuation).
    pub polarization: Polarization,
    /// Rain climate.
    pub rain: RainRate,
    /// Rain distance factor edition.
    #[serde(default)]
    pub rain_method: RainMethod,
    /// Multipath model and its climatic inputs.
    pub multipath: MultipathModel,
    /// Worst-month → annual conversion for multipath.
    #[serde(default)]
    pub worst_month_conversion: WorstMonthConversion,
    /// Apply the P.530 §2.3.1 rule that multipath can be neglected on paths shorter than
    /// 5 km (default true).
    #[serde(default = "default_true")]
    pub ignore_multipath_below_5km: bool,
}

fn default_true() -> bool {
    true
}

/// Result of [`microwave_availability`]. Percentages are % of time.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct MicrowaveAvailability {
    /// Path inclination |εp| (mrad).
    pub path_inclination_mrad: f64,
    /// Path elevation angle used for P.838 (deg).
    pub path_elevation_deg: f64,
    /// Multipath occurrence factor p0 (%).
    pub multipath_p0_pct: f64,
    /// % of the average worst month the fade margin is exceeded by multipath.
    pub multipath_worst_month_pct: f64,
    /// % of the average year the fade margin is exceeded by multipath.
    pub multipath_annual_pct: f64,
    /// R0.01 used (mm/h).
    pub rain_r001_mm_h: f64,
    /// Rain specific attenuation at R0.01 (dB/km).
    pub rain_gamma_r_db_per_km: f64,
    /// Effective rain path length (km).
    pub rain_effective_path_km: f64,
    /// Rain attenuation exceeded 0.01 % of the year (dB).
    pub rain_a001_db: f64,
    /// % of the average year the fade margin is exceeded by rain.
    pub rain_annual_pct: f64,
    /// Rain outage as worst-month % (P.841 global parameters).
    pub rain_worst_month_pct: f64,
    /// Multipath + rain, % of the average year.
    pub total_annual_outage_pct: f64,
    /// `100 − total_annual_outage_pct`.
    pub availability_pct: f64,
    /// Multipath outage, seconds per average year.
    pub multipath_seconds_per_year: f64,
    /// Rain outage, seconds per average year.
    pub rain_seconds_per_year: f64,
    /// Combined outage, seconds per average year.
    pub total_seconds_per_year: f64,
    /// Combined outage, minutes per average year.
    pub total_minutes_per_year: f64,
}

/// Compute multipath and rain outage of a microwave hop for a given flat fade margin.
pub fn microwave_availability(input: &MicrowaveLinkInput) -> Result<MicrowaveAvailability, RfError> {
    require_positive("frequency_ghz", input.frequency_ghz)?;
    require_positive("path_length_km", input.path_length_km)?;
    require_finite("fade_margin_db", input.fade_margin_db)?;
    require_finite("tx_antenna_amsl_m", input.tx_antenna_amsl_m)?;
    require_finite("rx_antenna_amsl_m", input.rx_antenna_amsl_m)?;
    let d = input.path_length_km;
    let f = input.frequency_ghz;
    let eps = path_inclination_mrad(input.tx_antenna_amsl_m, input.rx_antenna_amsl_m, d);
    let elev = ((input.rx_antenna_amsl_m - input.tx_antenna_amsl_m) / (d * 1000.0)).atan().to_degrees().abs();

    let geom = MultipathGeometry {
        path_length_km: d,
        frequency_ghz: f,
        he_amsl_m: input.tx_antenna_amsl_m,
        hr_amsl_m: input.rx_antenna_amsl_m,
    };
    let (p0, pw, pa) = if input.ignore_multipath_below_5km && d < 5.0 {
        (0.0, 0.0, 0.0)
    } else {
        let p0 = multipath_occurrence_factor_p0(&input.multipath, &geom);
        let pw = fade_exceedance_worst_month_pct(p0, input.fade_margin_db);
        let pa = fade_exceedance_annual_pct(p0, input.fade_margin_db, &input.worst_month_conversion, d, eps);
        (p0, pw, pa)
    };

    let r001 = input.rain.r001()?;
    let tilt = input.polarization.tilt_deg();
    let rain = match input.rain_method {
        RainMethod::P530v19 => rain_attenuation_001(d, f, r001, tilt, elev),
        RainMethod::P530v17 => rain_attenuation_001_v17(d, f, r001, tilt, elev),
    };
    let rain_pct = rain_exceedance_pct(rain.a001_db, f, input.fade_margin_db);
    let total = (pa + rain_pct).min(100.0);
    let ms = percent_to_seconds_per_year(pa);
    let rs = percent_to_seconds_per_year(rain_pct);
    let ts = percent_to_seconds_per_year(total);
    Ok(MicrowaveAvailability {
        path_inclination_mrad: eps,
        path_elevation_deg: elev,
        multipath_p0_pct: p0,
        multipath_worst_month_pct: pw,
        multipath_annual_pct: pa,
        rain_r001_mm_h: r001,
        rain_gamma_r_db_per_km: rain.gamma_r_db_per_km,
        rain_effective_path_km: rain.effective_path_km,
        rain_a001_db: rain.a001_db,
        rain_annual_pct: rain_pct,
        rain_worst_month_pct: worst_month_from_annual(rain_pct, &P841Params::GLOBAL),
        total_annual_outage_pct: total,
        availability_pct: 100.0 - total,
        multipath_seconds_per_year: ms,
        rain_seconds_per_year: rs,
        total_seconds_per_year: ts,
        total_minutes_per_year: ts / 60.0,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::itu::p530::rain_attenuation_db;

    fn base() -> MicrowaveLinkInput {
        MicrowaveLinkInput {
            frequency_ghz: 18.0,
            path_length_km: 20.0,
            tx_antenna_amsl_m: 300.0,
            rx_antenna_amsl_m: 350.0,
            fade_margin_db: 40.0,
            polarization: Polarization::Vertical,
            rain: RainRate::Zone('K'),
            rain_method: RainMethod::P530v19,
            multipath: MultipathModel::P530v17 { dn1: -400.0, s_a_m: 50.0 },
            worst_month_conversion: WorstMonthConversion::default(),
            ignore_multipath_below_5km: true,
        }
    }

    #[test]
    fn combined_availability() {
        let r = microwave_availability(&base()).unwrap();
        assert_eq!(r.rain_r001_mm_h, 42.0);
        // rain outage at FM equals inverse of Ap
        let a = rain_attenuation_db(r.rain_a001_db, 18.0, r.rain_annual_pct);
        assert!((a - 40.0).abs() < 1e-6);
        assert!((r.total_annual_outage_pct - (r.multipath_annual_pct + r.rain_annual_pct)).abs() < 1e-12);
        assert!((r.availability_pct + r.total_annual_outage_pct - 100.0).abs() < 1e-12);
        assert!((r.total_minutes_per_year * 60.0 - r.total_seconds_per_year).abs() < 1e-9);
        assert!(r.rain_annual_pct > 0.001 && r.rain_annual_pct < 0.1, "{}", r.rain_annual_pct);
        assert!(r.multipath_annual_pct > 0.0 && r.multipath_annual_pct < r.multipath_worst_month_pct);
        // more margin → better availability
        let mut b = base();
        b.fade_margin_db = 50.0;
        let r2 = microwave_availability(&b).unwrap();
        assert!(r2.availability_pct > r.availability_pct);
        let j = serde_json::to_string(&b).unwrap();
        let back: MicrowaveLinkInput = serde_json::from_str(&j).unwrap();
        assert_eq!(back, b);
    }

    #[test]
    fn short_path_no_multipath_and_errors() {
        let mut b = base();
        b.path_length_km = 3.0;
        let r = microwave_availability(&b).unwrap();
        assert_eq!(r.multipath_annual_pct, 0.0);
        b.rain = RainRate::Zone('X');
        assert!(microwave_availability(&b).is_err());
        b.rain = RainRate::Zone('K');
        b.path_length_km = -1.0;
        assert!(microwave_availability(&b).is_err());
    }
}
