//! ITU-R P.837-1 (1994) rain climatic zones.
//!
//! Table 1 of Rec. ITU-R PN.837-1 gives the rainfall intensity (mm/h, 1-minute integration)
//! exceeded for 1, 0.3, 0.1, 0.03, 0.01, 0.003 and 0.001 % of an average year for the 15
//! zones A–Q (letters I and O are not used). The zone maps (Figs. 1–3) are not included;
//! the gridded P.837-7 model is out of scope for now — use [`RainRate::Custom`] to pass a
//! locally known R0.01.
//!
//! Zone A at 1 % is given as "< 0.1" in the Recommendation; the value 0.1 is stored.

use serde::{Deserialize, Serialize};

use crate::error::RfError;

/// Time percentages (%) of the columns of P.837-1 Table 1.
pub const RAIN_ZONE_PERCENTAGES: [f64; 7] = [1.0, 0.3, 0.1, 0.03, 0.01, 0.003, 0.001];

/// Zone letters in table order.
pub const RAIN_ZONES: [char; 15] = ['A', 'B', 'C', 'D', 'E', 'F', 'G', 'H', 'J', 'K', 'L', 'M', 'N', 'P', 'Q'];

/// P.837-1 Table 1: rain rate (mm/h) for each zone (rows) at [`RAIN_ZONE_PERCENTAGES`].
const TABLE: [[f64; 7]; 15] = [
    //  1.0   0.3   0.1  0.03  0.01 0.003 0.001
    [0.1, 0.8, 2.0, 5.0, 8.0, 14.0, 22.0],          // A
    [0.5, 2.0, 3.0, 6.0, 12.0, 21.0, 32.0],         // B
    [0.7, 2.8, 5.0, 9.0, 15.0, 26.0, 42.0],         // C
    [2.1, 4.5, 8.0, 13.0, 19.0, 29.0, 42.0],        // D
    [0.6, 2.4, 6.0, 12.0, 22.0, 41.0, 70.0],        // E
    [1.7, 4.5, 8.0, 15.0, 28.0, 54.0, 78.0],        // F
    [3.0, 7.0, 12.0, 20.0, 30.0, 45.0, 65.0],       // G
    [2.0, 4.0, 10.0, 18.0, 32.0, 55.0, 83.0],       // H
    [8.0, 13.0, 20.0, 28.0, 35.0, 45.0, 55.0],      // J
    [1.5, 4.2, 12.0, 23.0, 42.0, 70.0, 100.0],      // K
    [2.0, 7.0, 15.0, 33.0, 60.0, 105.0, 150.0],     // L
    [4.0, 11.0, 22.0, 40.0, 63.0, 95.0, 120.0],     // M
    [5.0, 15.0, 35.0, 65.0, 95.0, 140.0, 180.0],    // N
    [12.0, 34.0, 65.0, 105.0, 145.0, 200.0, 250.0], // P
    [24.0, 49.0, 72.0, 96.0, 115.0, 142.0, 170.0],  // Q
];

/// Rain rate exceeded for 1, 0.3, 0.1, 0.03, 0.01, 0.003, 0.001 % of the year (mm/h)
/// for a P.837-1 zone letter (case-insensitive). None for unknown letters.
pub fn zone_table(zone: char) -> Option<[f64; 7]> {
    let z = zone.to_ascii_uppercase();
    RAIN_ZONES.iter().position(|c| *c == z).map(|i| TABLE[i])
}

/// R0.01 (mm/h) for a P.837-1 zone.
pub fn r001_for_zone(zone: char) -> Option<f64> {
    zone_table(zone).map(|r| r[4])
}

/// Rain rate (mm/h) exceeded for `p_percent` of an average year in a P.837-1 zone,
/// log–log interpolated between table columns and clamped to 0.001–1 %.
pub fn rain_rate_for_zone(zone: char, p_percent: f64) -> Option<f64> {
    let t = zone_table(zone)?;
    let p = p_percent.clamp(0.001, 1.0);
    if let Some(i) = RAIN_ZONE_PERCENTAGES.iter().position(|c| ((c - p) / c).abs() < 1e-12) {
        return Some(t[i]);
    }
    let lp = p.log10();
    for i in 0..6 {
        let (p0, p1) = (RAIN_ZONE_PERCENTAGES[i], RAIN_ZONE_PERCENTAGES[i + 1]);
        if p <= p0 && p >= p1 {
            let w = (lp - p0.log10()) / (p1.log10() - p0.log10());
            let lr = t[i].log10() + w * (t[i + 1].log10() - t[i].log10());
            return Some(10f64.powf(lr));
        }
    }
    None
}

/// Source of the rain rate R0.01 used by rain attenuation predictions.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum RainRate {
    /// ITU-R P.837-1 rain climate zone letter (A–Q).
    Zone(char),
    /// User supplied rain rate exceeded for 0.01 % of an average year (mm/h).
    Custom {
        /// R0.01 (mm/h).
        r001: f64,
    },
}

impl RainRate {
    /// R0.01 (mm/h).
    pub fn r001(&self) -> Result<f64, RfError> {
        match *self {
            RainRate::Zone(z) => r001_for_zone(z).ok_or(RfError::UnknownRainZone(z)),
            RainRate::Custom { r001 } => {
                if r001.is_finite() && r001 >= 0.0 {
                    Ok(r001)
                } else {
                    Err(RfError::invalid(format!("R0.01 must be ≥ 0 (got {r001})")))
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn r001_values() {
        let expected = [
            ('A', 8.0),
            ('B', 12.0),
            ('C', 15.0),
            ('D', 19.0),
            ('E', 22.0),
            ('F', 28.0),
            ('G', 30.0),
            ('H', 32.0),
            ('J', 35.0),
            ('K', 42.0),
            ('L', 60.0),
            ('M', 63.0),
            ('N', 95.0),
            ('P', 145.0),
            ('Q', 115.0),
        ];
        for (z, r) in expected {
            assert_eq!(r001_for_zone(z), Some(r));
            assert_eq!(r001_for_zone(z.to_ascii_lowercase()), Some(r));
        }
        assert_eq!(r001_for_zone('I'), None);
        assert_eq!(r001_for_zone('O'), None);
    }

    #[test]
    fn interpolation() {
        assert_eq!(rain_rate_for_zone('K', 0.01), Some(42.0));
        assert_eq!(rain_rate_for_zone('K', 0.001), Some(100.0));
        let r = rain_rate_for_zone('K', 0.02).unwrap();
        assert!(r > 23.0 && r < 42.0);
        // log-log midpoint between 0.01 (42) and 0.003 (70) at p=sqrt(0.01*0.003)
        let pm = (0.01f64 * 0.003).sqrt();
        let rm = rain_rate_for_zone('K', pm).unwrap();
        assert!((rm - (42.0f64 * 70.0).sqrt()).abs() < 1e-9);
        // monotonic in p for all zones
        for z in RAIN_ZONES {
            let t = zone_table(z).unwrap();
            assert!(t.windows(2).all(|w| w[1] >= w[0]), "zone {z}");
        }
    }

    #[test]
    fn rain_rate_enum() {
        assert_eq!(RainRate::Zone('N').r001().unwrap(), 95.0);
        assert_eq!(RainRate::Custom { r001: 50.0 }.r001().unwrap(), 50.0);
        assert!(RainRate::Zone('Z').r001().is_err());
        assert!(RainRate::Custom { r001: -1.0 }.r001().is_err());
        let j = serde_json::to_string(&RainRate::Zone('K')).unwrap();
        assert_eq!(serde_json::from_str::<RainRate>(&j).unwrap(), RainRate::Zone('K'));
    }
}
