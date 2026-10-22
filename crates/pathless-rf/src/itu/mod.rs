//! ITU-R propagation recommendations used for microwave / millimetre-wave link design.
//!
//! | Module | Recommendation | Edition implemented |
//! |---|---|---|
//! | [`p838`] | Specific attenuation model for rain | P.838-3 (03/2005), current |
//! | [`p837`] | Rain climate zones (legacy table) | P.837-1 (1994) Table 1 |
//! | [`p530`] | Terrestrial LOS design: multipath, rain | P.530-17 multipath (K from dN1, sa), P.530-18/19 multipath (K, dN75, hc), P.530-19 rain (§2.4.1) |
//! | [`p676`] | Gaseous attenuation | P.676-13 (08/2022) Annex 1 line-by-line |
//! | [`p841`] | Annual ↔ worst-month conversion | P.841-7 (08/2022) |
//! | [`availability`] | Convenience availability calculation | combines the above |

pub mod availability;
pub mod p530;
pub mod p676;
pub mod p837;
pub mod p838;
pub mod p841;

use serde::{Deserialize, Serialize};

/// Antenna polarization for rain attenuation (P.838 polarization tilt angle τ).
#[derive(Debug, Clone, Copy, PartialEq, Default, Serialize, Deserialize)]
pub enum Polarization {
    /// Horizontal linear (τ = 0°).
    Horizontal,
    /// Vertical linear (τ = 90°).
    #[default]
    Vertical,
    /// Circular (τ = 45°).
    Circular,
    /// Linear with arbitrary tilt τ (degrees from horizontal).
    TiltDeg(f64),
}

impl Polarization {
    /// Polarization tilt angle τ relative to horizontal (degrees).
    pub fn tilt_deg(&self) -> f64 {
        match *self {
            Polarization::Horizontal => 0.0,
            Polarization::Vertical => 90.0,
            Polarization::Circular => 45.0,
            Polarization::TiltDeg(t) => t,
        }
    }
}

/// Seconds in an average (Julian) year, 365.25 days.
pub const SECONDS_PER_YEAR: f64 = 365.25 * 86_400.0;

/// Convert an annual percentage of time to seconds per year.
pub fn percent_to_seconds_per_year(p_percent: f64) -> f64 {
    p_percent / 100.0 * SECONDS_PER_YEAR
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn polarization_tilt() {
        assert_eq!(Polarization::Horizontal.tilt_deg(), 0.0);
        assert_eq!(Polarization::Vertical.tilt_deg(), 90.0);
        assert_eq!(Polarization::Circular.tilt_deg(), 45.0);
        assert_eq!(Polarization::TiltDeg(30.0).tilt_deg(), 30.0);
        // 99.999 % → 0.001 % → 315.576 s/yr (≈ 5.26 min)
        assert!((percent_to_seconds_per_year(0.001) - 315.576).abs() < 1e-9);
    }
}
