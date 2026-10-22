//! Antenna patterns: data model, 3-D gain interpolation, MSI/Planet I/O, generators and
//! the built-in generic catalog.
//!
//! # Pattern conventions (MSI / Planet)
//!
//! * `horizontal[i]` – attenuation (dB, ≥ 0, relative to the peak gain) at azimuth `i`
//!   degrees **clockwise from boresight**, i = 0…359.
//! * `vertical[i]` – attenuation (dB) at `i` degrees **below the horizon** in the vertical
//!   plane through boresight: 0 = horizon in front, 90 = nadir (straight down),
//!   180 = horizon behind the antenna, 270 = zenith (straight up).
//! * Electrical downtilt is *included* in the vertical cut (as in manufacturer files);
//!   [`AntennaPattern::electrical_tilt_deg`] is informational.
//!
//! Function arguments use the more natural "elevation above horizon" (positive up); the
//! conversion to the MSI vertical index is done internally.

pub mod catalog;
pub mod generators;
pub mod msi;

use serde::{Deserialize, Serialize};

pub use catalog::{builtin_catalog, load_catalog_from_dir, AntennaKind, AntennaSpec, CatalogLoad};
pub use generators::{DishEnvelope, DishParams, OmniParams, SectorParams, SidelobeEnvelope, YagiParams};
pub use msi::{GainUnit, MsiParseOptions};

/// Number of samples in each pattern cut (1° resolution).
pub const PATTERN_SAMPLES: usize = 360;

/// An antenna radiation pattern described by a horizontal and a vertical cut.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AntennaPattern {
    /// Model / pattern name.
    pub name: String,
    /// Manufacturer (free text, "Generic" for generated patterns).
    pub make: String,
    /// Frequency the pattern applies to (MHz); 0 if unknown.
    pub frequency_mhz: f64,
    /// Peak gain (dBi).
    pub gain_dbi: f64,
    /// Horizontal 3 dB beamwidth (deg).
    pub h_beamwidth_deg: f64,
    /// Vertical 3 dB beamwidth (deg).
    pub v_beamwidth_deg: f64,
    /// Front-to-back ratio (dB).
    pub front_to_back_db: f64,
    /// Electrical downtilt included in the vertical cut (deg, positive = down).
    pub electrical_tilt_deg: f64,
    /// Polarization description (e.g. "Vertical", "Horizontal", "+45", "Dual slant").
    pub polarization: String,
    /// Free-text comment.
    pub comment: String,
    /// 360 attenuation values (dB re peak); index = azimuth clockwise from boresight (deg).
    pub horizontal: Vec<f32>,
    /// 360 attenuation values (dB re peak); index = degrees below horizon (MSI convention:
    /// 0 horizon, 90 nadir, 180 back horizon, 270 zenith).
    pub vertical: Vec<f32>,
}

/// Method used to combine the horizontal and vertical cuts into a 3-D gain estimate.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum InterpolationMethod {
    /// Summing method: `A(φ,θ) = H(φ) + V'(φ,θ)` where V' is the vertical cut (front half for
    /// |φ| ≤ 90°, blended linearly with the back half re-referenced to the back horizon for
    /// |φ| > 90° so that the back vertical cut is reproduced at φ = 180°). The total is
    /// clamped to the deepest attenuation present in either cut.
    Summing,
    /// Cross-weighted interpolation after F. Gil, A. R. Claro et al., "A 3-D interpolation
    /// method for base-station-antenna radiation patterns", IEEE AP Magazine 43(2), 2001:
    /// `A = (H·w1 + V·w2)/(w1 + w2)`, `w1 = v(1 − h)`, `w2 = h(1 − v)` with
    /// `h = 10^(−H/10)`, `v = 10^(−V/10)` the normalised linear cut values. Exact on both
    /// cuts. The front (|φ| ≤ 90°) uses the front half of the vertical cut; behind the antenna
    /// the results obtained with the front and back halves are blended linearly in |φ|
    /// (weight (|φ|−90)/90 on the back half) to keep the gain continuous.
    #[default]
    Weighted,
}

/// Linear interpolation on a circular table of `n` samples covering 360°.
fn interp_circular(v: &[f32], angle_deg: f64) -> f64 {
    let n = v.len();
    if n == 0 {
        return 0.0;
    }
    let x = angle_deg.rem_euclid(360.0) * n as f64 / 360.0;
    let i0 = (x.floor() as usize) % n;
    let i1 = (i0 + 1) % n;
    let t = x - x.floor();
    v[i0] as f64 * (1.0 - t) + v[i1] as f64 * t
}

/// Gil et al. cross-weighted combination of two attenuations (dB, ≥ 0).
fn cross_weighted(a_h: f64, a_v: f64) -> f64 {
    let a_h = a_h.max(0.0);
    let a_v = a_v.max(0.0);
    let h = 10f64.powf(-a_h / 10.0);
    let v = 10f64.powf(-a_v / 10.0);
    let w1 = v * (1.0 - h);
    let w2 = h * (1.0 - v);
    let s = w1 + w2;
    if s < 1e-15 {
        0.5 * (a_h + a_v)
    } else {
        (a_h * w1 + a_v * w2) / s
    }
}

/// Wrap an angle to (−180, 180].
fn wrap180(a: f64) -> f64 {
    let x = a.rem_euclid(360.0);
    if x > 180.0 {
        x - 360.0
    } else {
        x
    }
}

/// Direction of a target in the frame of a mechanically down-tilted antenna.
///
/// * `antenna_azimuth_deg` – boresight azimuth (deg true, clockwise from north)
/// * `mechanical_downtilt_deg` – mechanical downtilt β (deg, positive = boresight below horizon)
/// * `target_azimuth_deg`, `target_elevation_deg` – direction to the target as seen from the
///   antenna site (azimuth deg true, elevation above the local horizontal, deg)
///
/// Returns `(azimuth_off_boresight_deg, elevation_deg)` in the antenna's own frame
/// (azimuth clockwise in (−180, 180], elevation positive towards the antenna's "up"), using a
/// proper rotation about the antenna's horizontal axis (ITU-R F.1336-5 eqs. (3b), (3c)):
///
/// `θ = asin(sin θh·cos β + cos θh·cos φh·sin β)`,
/// `φ = atan2(cos θh·sin φh, −sin θh·sin β + cos θh·cos φh·cos β)`.
pub fn antenna_relative_direction(
    antenna_azimuth_deg: f64,
    mechanical_downtilt_deg: f64,
    target_azimuth_deg: f64,
    target_elevation_deg: f64,
) -> (f64, f64) {
    let phi_h = wrap180(target_azimuth_deg - antenna_azimuth_deg).to_radians();
    let th = target_elevation_deg.to_radians();
    let b = mechanical_downtilt_deg.to_radians();
    let x = th.cos() * phi_h.cos() * b.cos() - th.sin() * b.sin();
    let y = th.cos() * phi_h.sin();
    let z = (th.sin() * b.cos() + th.cos() * phi_h.cos() * b.sin()).clamp(-1.0, 1.0);
    let el = z.asin().to_degrees();
    let az = if x.abs() < 1e-15 && y.abs() < 1e-15 { 0.0 } else { y.atan2(x).to_degrees() };
    (az, el)
}

impl AntennaPattern {
    /// A pattern with flat (0 dB) cuts and the given gain.
    pub fn flat(name: impl Into<String>, frequency_mhz: f64, gain_dbi: f64) -> Self {
        Self {
            name: name.into(),
            make: "Generic".into(),
            frequency_mhz,
            gain_dbi,
            h_beamwidth_deg: 360.0,
            v_beamwidth_deg: 360.0,
            front_to_back_db: 0.0,
            electrical_tilt_deg: 0.0,
            polarization: String::new(),
            comment: String::new(),
            horizontal: vec![0.0; PATTERN_SAMPLES],
            vertical: vec![0.0; PATTERN_SAMPLES],
        }
    }

    /// Check that both cuts have 360 finite samples.
    pub fn validate(&self) -> Result<(), crate::RfError> {
        for (name, cut) in [("horizontal", &self.horizontal), ("vertical", &self.vertical)] {
            if cut.len() != PATTERN_SAMPLES {
                return Err(crate::RfError::invalid(format!("{name} cut has {} samples, expected 360", cut.len())));
            }
            if cut.iter().any(|x| !x.is_finite()) {
                return Err(crate::RfError::invalid(format!("{name} cut contains non-finite values")));
            }
        }
        Ok(())
    }

    /// Horizontal-cut attenuation (dB) at `az_deg` clockwise from boresight (interpolated).
    pub fn horizontal_attenuation_db(&self, az_deg: f64) -> f64 {
        interp_circular(&self.horizontal, az_deg)
    }

    /// Vertical-cut attenuation (dB) at MSI angle `deg_below_horizon` (0 horizon,
    /// 90 nadir, 180 back horizon, 270 zenith), interpolated.
    pub fn vertical_attenuation_db(&self, deg_below_horizon: f64) -> f64 {
        interp_circular(&self.vertical, deg_below_horizon)
    }

    /// Front half of the vertical cut at elevation `elev_deg` above the horizon.
    pub fn vertical_front_db(&self, elev_deg: f64) -> f64 {
        self.vertical_attenuation_db(-elev_deg.clamp(-90.0, 90.0))
    }

    /// Back half of the vertical cut at elevation `elev_deg` above the (rear) horizon.
    pub fn vertical_back_db(&self, elev_deg: f64) -> f64 {
        self.vertical_attenuation_db(180.0 + elev_deg.clamp(-90.0, 90.0))
    }

    fn max_attenuation(&self) -> f64 {
        self.horizontal.iter().chain(self.vertical.iter()).fold(0.0f64, |m, x| m.max(*x as f64))
    }

    /// Attenuation (dB re peak) toward `(az_off_boresight_deg, elev_deg)` in the antenna
    /// frame. `elev_deg` is positive above the horizon.
    pub fn attenuation_db(&self, az_off_boresight_deg: f64, elev_deg: f64, method: InterpolationMethod) -> f64 {
        let az = wrap180(az_off_boresight_deg);
        let el = elev_deg.clamp(-90.0, 90.0);
        let a_h = self.horizontal_attenuation_db(az);
        let abs_az = az.abs();
        let s = ((abs_az - 90.0) / 90.0).clamp(0.0, 1.0);
        let vf = self.vertical_front_db(el);
        match method {
            InterpolationMethod::Summing => {
                let v = if s > 0.0 {
                    let vb = self.vertical_back_db(el) - self.vertical_back_db(0.0) + self.vertical_front_db(0.0);
                    (1.0 - s) * vf + s * vb
                } else {
                    vf
                };
                (a_h + v).clamp(0.0, self.max_attenuation())
            }
            InterpolationMethod::Weighted => {
                let front = cross_weighted(a_h, vf);
                if s > 0.0 {
                    let back = cross_weighted(a_h, self.vertical_back_db(el));
                    (1.0 - s) * front + s * back
                } else {
                    front
                }
            }
        }
    }

    /// Gain (dBi) toward `(az_off_boresight_deg, elev_deg_above_horizon)` in the antenna frame
    /// using the default [`InterpolationMethod::Weighted`].
    pub fn gain_dbi(&self, az_off_boresight_deg: f64, elev_deg_above_horizon: f64) -> f64 {
        self.gain_dbi_with(az_off_boresight_deg, elev_deg_above_horizon, InterpolationMethod::Weighted)
    }

    /// Gain (dBi) with an explicit interpolation method.
    pub fn gain_dbi_with(
        &self,
        az_off_boresight_deg: f64,
        elev_deg_above_horizon: f64,
        method: InterpolationMethod,
    ) -> f64 {
        self.gain_dbi - self.attenuation_db(az_off_boresight_deg, elev_deg_above_horizon, method)
    }

    /// Gain (dBi) toward a target given the antenna's azimuth (deg true), mechanical downtilt
    /// (deg, positive down) and the target azimuth (deg true) / elevation (deg above horizon).
    /// See [`antenna_relative_direction`].
    pub fn gain_toward(
        &self,
        antenna_azimuth_deg: f64,
        mechanical_downtilt_deg: f64,
        target_azimuth_deg: f64,
        target_elevation_deg: f64,
        method: InterpolationMethod,
    ) -> f64 {
        let (az, el) = antenna_relative_direction(
            antenna_azimuth_deg,
            mechanical_downtilt_deg,
            target_azimuth_deg,
            target_elevation_deg,
        );
        self.gain_dbi_with(az, el, method)
    }

    /// Horizontal cut for polar plots: `(azimuth_deg_clockwise_from_boresight, gain_dbi)`.
    pub fn horizontal_cut(&self) -> Vec<(f64, f64)> {
        self.horizontal.iter().enumerate().map(|(i, a)| (i as f64, self.gain_dbi - *a as f64)).collect()
    }

    /// Vertical cut for polar plots: `(deg_below_horizon, gain_dbi)` (MSI angle convention).
    pub fn vertical_cut(&self) -> Vec<(f64, f64)> {
        self.vertical.iter().enumerate().map(|(i, a)| (i as f64, self.gain_dbi - *a as f64)).collect()
    }

    /// −3 dB beamwidth (deg) measured on the horizontal cut (360 for omnis).
    pub fn measured_h_beamwidth_deg(&self) -> f64 {
        measure_beamwidth(&self.horizontal)
    }

    /// −3 dB beamwidth (deg) measured on the vertical cut.
    pub fn measured_v_beamwidth_deg(&self) -> f64 {
        measure_beamwidth(&self.vertical)
    }

    /// Front-to-back ratio (dB) measured on the horizontal cut: attenuation at 180° minus
    /// attenuation at 0°.
    pub fn measured_front_to_back_db(&self) -> f64 {
        let h = &self.horizontal;
        if h.len() < 360 {
            return 0.0;
        }
        (h[180] - h[0]) as f64
    }

    /// Parse an MSI / Planet pattern file contents with default options.
    pub fn from_msi_str(text: &str) -> Result<Self, crate::RfError> {
        msi::parse_msi(text, &MsiParseOptions::default())
    }

    /// Serialize to MSI / Planet text (gain written in dBi).
    pub fn to_msi_string(&self) -> String {
        msi::write_msi(self, GainUnit::Dbi)
    }
}

/// Width (deg) of the region around the minimum attenuation sample within which the
/// attenuation stays below min + 3 dB (linear interpolation of the crossings).
fn measure_beamwidth(cut: &[f32]) -> f64 {
    let n = cut.len();
    if n == 0 {
        return 0.0;
    }
    let step = 360.0 / n as f64;
    let (i0, amin) = cut
        .iter()
        .enumerate()
        .fold((0usize, f32::INFINITY), |(bi, bv), (i, v)| if *v < bv { (i, *v) } else { (bi, bv) });
    let thr = amin as f64 + 3.0;
    let mut half = [f64::NAN; 2];
    for (k, dir) in [1isize, -1isize].iter().enumerate() {
        let mut prev = amin as f64;
        for s in 1..n {
            let idx = (i0 as isize + dir * s as isize).rem_euclid(n as isize) as usize;
            let a = cut[idx] as f64;
            if a >= thr {
                let t = (thr - prev) / (a - prev);
                half[k] = ((s - 1) as f64 + t) * step;
                break;
            }
            prev = a;
        }
    }
    if half[0].is_nan() || half[1].is_nan() {
        360.0
    } else {
        (half[0] + half[1]).min(360.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_pattern() -> AntennaPattern {
        // simple sector-like pattern: H = 12(φ/65)² capped 25, V = 12(θ/10)² capped 20 front,
        // back = 25
        let mut p = AntennaPattern::flat("test", 2000.0, 18.0);
        for i in 0..360 {
            let az = wrap180(i as f64).abs();
            p.horizontal[i] = (12.0 * (az / 65.0).powi(2)).min(25.0) as f32;
            let el = if i <= 90 {
                i as f64
            } else if i >= 270 {
                360.0 - i as f64
            } else {
                999.0
            };
            p.vertical[i] = if el < 999.0 { (12.0 * (el / 10.0).powi(2)).min(20.0) as f32 } else { 25.0 };
        }
        p
    }

    #[test]
    fn cuts_reproduced_exactly() {
        let p = test_pattern();
        for m in [InterpolationMethod::Weighted, InterpolationMethod::Summing] {
            // boresight
            assert!((p.gain_dbi_with(0.0, 0.0, m) - 18.0).abs() < 1e-9);
            // horizontal plane, front
            assert!((p.attenuation_db(30.0, 0.0, m) - 12.0 * (30.0f64 / 65.0).powi(2)).abs() < 1e-5, "{m:?}");
            assert!((p.attenuation_db(-30.0, 0.0, m) - 12.0 * (30.0f64 / 65.0).powi(2)).abs() < 1e-5);
            // vertical plane: 5° below horizon → MSI index 5
            assert!((p.attenuation_db(0.0, -5.0, m) - 3.0).abs() < 1e-5, "{m:?}");
            assert!((p.attenuation_db(0.0, 5.0, m) - 3.0).abs() < 1e-5);
            // back
            assert!((p.attenuation_db(180.0, 0.0, m) - 25.0).abs() < 1e-5);
        }
    }

    #[test]
    fn weighted_is_between_cuts_and_summing_adds() {
        let p = test_pattern();
        let h = p.horizontal_attenuation_db(40.0);
        let v = p.vertical_front_db(-8.0);
        let w = p.attenuation_db(40.0, -8.0, InterpolationMethod::Weighted);
        assert!(w >= h.min(v) - 1e-9 && w <= h.max(v) + 1e-9);
        let s = p.attenuation_db(40.0, -8.0, InterpolationMethod::Summing);
        assert!((s - (h + v)).abs() < 1e-9);
        // continuity across ±90°
        let a = p.attenuation_db(89.999, 10.0, InterpolationMethod::Weighted);
        let b = p.attenuation_db(90.001, 10.0, InterpolationMethod::Weighted);
        assert!((a - b).abs() < 1e-2);
        // interpolation between samples
        let mid = p.horizontal_attenuation_db(30.5);
        assert!((mid - 0.5 * (p.horizontal[30] + p.horizontal[31]) as f64).abs() < 1e-6);
    }

    #[test]
    fn elevation_sign_convention() {
        // Pattern with a deep null only *below* the horizon at 20° (index 20)
        let mut p = AntennaPattern::flat("x", 100.0, 0.0);
        p.vertical[20] = 30.0;
        assert!((p.gain_dbi(0.0, -20.0) + 30.0).abs() < 1e-9);
        assert!(p.gain_dbi(0.0, 20.0).abs() < 1e-9);
        // zenith = index 270
        p.vertical[270] = 10.0;
        assert!((p.gain_dbi_with(0.0, 90.0, InterpolationMethod::Summing) + 10.0).abs() < 1e-9);
    }

    #[test]
    fn mechanical_tilt_rotation() {
        // Boresight azimuth 90°, downtilt 5°: a target on the horizon at boresight azimuth is 5°
        // *above* the antenna axis.
        let (az, el) = antenna_relative_direction(90.0, 5.0, 90.0, 0.0);
        assert!(az.abs() < 1e-9 && (el - 5.0).abs() < 1e-9);
        // Target 5° below horizon at boresight → on axis
        let (az, el) = antenna_relative_direction(90.0, 5.0, 90.0, -5.0);
        assert!(az.abs() < 1e-9 && el.abs() < 1e-9);
        // Target at 90° off boresight on the horizon: tilt has no effect on elevation
        let (az, el) = antenna_relative_direction(0.0, 10.0, 90.0, 0.0);
        assert!((az - 90.0).abs() < 1e-9 && el.abs() < 1e-9);
        // Behind the antenna on the horizon: tilted antenna's back looks *up*, so the target
        // appears below the axis plane
        let (az, el) = antenna_relative_direction(0.0, 10.0, 180.0, 0.0);
        assert!((az.abs() - 180.0).abs() < 1e-9 && (el + 10.0).abs() < 1e-9);
        // 45° off boresight: elevation shift less than the full tilt
        let (_, el) = antenna_relative_direction(0.0, 10.0, 45.0, 0.0);
        assert!(
            el > 0.0
                && el < 10.0
                && (el - (10f64.to_radians().sin() * 45f64.to_radians().cos()).asin().to_degrees()).abs() < 1e-9
        );
        let p = test_pattern();
        let g = p.gain_toward(0.0, 10.0, 0.0, -10.0, InterpolationMethod::Weighted);
        assert!((g - 18.0).abs() < 1e-9);
    }

    #[test]
    fn beamwidth_measurement() {
        let p = test_pattern();
        // 12(φ/65)² = 3 → φ = 32.5 → bw 65 (sampled at 1° with linear interpolation)
        assert!((p.measured_h_beamwidth_deg() - 65.0).abs() < 0.2, "{}", p.measured_h_beamwidth_deg());
        assert!((p.measured_v_beamwidth_deg() - 10.0).abs() < 0.2, "{}", p.measured_v_beamwidth_deg());
        assert!((p.measured_front_to_back_db() - 25.0).abs() < 1e-6);
        assert_eq!(AntennaPattern::flat("o", 1.0, 0.0).measured_h_beamwidth_deg(), 360.0);
        assert!(p.validate().is_ok());
        let cut = p.horizontal_cut();
        assert_eq!(cut.len(), 360);
        assert_eq!(cut[0], (0.0, 18.0));
        assert_eq!(p.vertical_cut()[180].1, 18.0 - 25.0);
    }
}
