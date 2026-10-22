//! Frequency-aware antenna pattern generators.
//!
//! | Generator | Model |
//! |---|---|
//! | [`isotropic`] | 0 dBi everywhere |
//! | [`half_wave_dipole`] | vertical λ/2 dipole, 2.15 dBi, `E(θ) = cos(π/2·cos θ)/sin θ` |
//! | [`omni_collinear`] | ITU-R F.1336-5 recommends 2.1 (peak) / 2.2 (average) omnidirectional elevation pattern, θ3 = 107.6·10^(−0.1·G0) |
//! | [`sector`] | ITU-R F.1336-5 recommends 3.1.1 (peak) / 3.1.2 (average) sectoral pattern (400 MHz–6 GHz model, also used above 6 GHz) with a front-to-back floor |
//! | [`yagi`] | empirical: Gaussian main lobe `12(ψ/θ3)²`, side-lobe plateau, linear ramp to F/B behind (see [`YagiParams`]) |
//! | [`parabolic_dish`] | Gmax = 10·log10(η(πD/λ)²); ITU-R F.699-9 peak envelope or ITU-R F.1245-3 average pattern, symmetric in both planes |
//!
//! Electrical downtilt (omni, sector) is applied with F.1336-5 eq. (1e).

use serde::{Deserialize, Serialize};

use super::{AntennaPattern, PATTERN_SAMPLES};
use crate::basic::{wavelength_m, DIPOLE_GAIN_DBI};

/// Side-lobe envelope used by the F.1336 based generators.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum SidelobeEnvelope {
    /// Peak side-lobe pattern (F.1336 recommends 2.1 / 3.1.1).
    #[default]
    Peak,
    /// Average side-lobe pattern (F.1336 recommends 2.2 / 3.1.2).
    Average,
}

/// Envelope used for parabolic dishes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum DishEnvelope {
    /// ITU-R F.699-9 reference (peak side-lobe envelope), 100 MHz–174.8 GHz.
    #[default]
    F699Peak,
    /// ITU-R F.1245-3 average side-lobe pattern, 1–86 GHz (F.699 is used below 1 GHz).
    F1245Average,
}

/// Parameters of [`omni_collinear`].
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct OmniParams {
    /// Peak gain G0 (dBi).
    pub gain_dbi: f64,
    /// F.1336 side-lobe parameter k. Default: 0.7 below 3 GHz (typical antennas),
    /// 0 from 3 GHz (recommends 2.3/2.4).
    #[serde(default)]
    pub k: Option<f64>,
    /// Peak or average side-lobe envelope.
    #[serde(default)]
    pub sidelobe: SidelobeEnvelope,
    /// Electrical downtilt (deg, positive down).
    #[serde(default)]
    pub electrical_tilt_deg: f64,
}

impl Default for OmniParams {
    fn default() -> Self {
        Self { gain_dbi: 6.0, k: None, sidelobe: SidelobeEnvelope::Peak, electrical_tilt_deg: 0.0 }
    }
}

fn default_fb_sector() -> f64 {
    25.0
}

/// Parameters of [`sector`].
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct SectorParams {
    /// Peak gain G0 (dBi).
    pub gain_dbi: f64,
    /// Horizontal (azimuth) 3 dB beamwidth φ3 (deg).
    pub h_beamwidth_deg: f64,
    /// Vertical 3 dB beamwidth θ3 (deg). If None, F.1336 eq. (3a):
    /// `θ3 = 31000·10^(−0.1·G0)/φ3`.
    #[serde(default)]
    pub v_beamwidth_deg: Option<f64>,
    /// Front-to-back floor: maximum attenuation of the pattern (dB). It only caps the
    /// attenuation; for wide sectors F.1336 itself gives a modest rear attenuation
    /// (`−Ghr(180/φ3)`, ≈ 17 dB for φ3 = 120°), which is not raised by this value.
    #[serde(default = "default_fb_sector")]
    pub front_to_back_db: f64,
    /// Use the "improved side-lobe" F.1336 parameters (kh = 0.7, kv = 0.3) instead of
    /// the typical ones (kh = 0.8, kv = 0.7). kp = ka = 0.7 in both cases.
    #[serde(default)]
    pub improved_sidelobes: bool,
    /// Peak or average side-lobe envelope.
    #[serde(default)]
    pub sidelobe: SidelobeEnvelope,
    /// Electrical downtilt (deg, positive down).
    #[serde(default)]
    pub electrical_tilt_deg: f64,
}

impl Default for SectorParams {
    fn default() -> Self {
        Self {
            gain_dbi: 17.0,
            h_beamwidth_deg: 65.0,
            v_beamwidth_deg: None,
            front_to_back_db: 25.0,
            improved_sidelobes: false,
            sidelobe: SidelobeEnvelope::Peak,
            electrical_tilt_deg: 0.0,
        }
    }
}

fn default_fb_yagi() -> f64 {
    18.0
}

fn default_sl_yagi() -> f64 {
    15.0
}

/// Parameters of [`yagi`].
///
/// Approximation: main lobe attenuation `12·(ψ/θ3)²` (same parabola as F.1336), capped at
/// `sidelobe_db` up to 90° off axis, then a linear ramp from `sidelobe_db` at 90° to
/// `front_to_back_db` at 180°. Beamwidths default to `θ = sqrt(31000·10^(−G/10))` in both
/// planes (if only one is given the other follows from the same product).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct YagiParams {
    /// Peak gain (dBi).
    pub gain_dbi: f64,
    /// Horizontal 3 dB beamwidth (deg), optional.
    #[serde(default)]
    pub h_beamwidth_deg: Option<f64>,
    /// Vertical 3 dB beamwidth (deg), optional.
    #[serde(default)]
    pub v_beamwidth_deg: Option<f64>,
    /// Front-to-back ratio (dB). Default 18.
    #[serde(default = "default_fb_yagi")]
    pub front_to_back_db: f64,
    /// Side-lobe plateau attenuation (dB). Default 15.
    #[serde(default = "default_sl_yagi")]
    pub sidelobe_db: f64,
}

impl Default for YagiParams {
    fn default() -> Self {
        Self { gain_dbi: 10.0, h_beamwidth_deg: None, v_beamwidth_deg: None, front_to_back_db: 18.0, sidelobe_db: 15.0 }
    }
}

fn default_efficiency() -> f64 {
    0.55
}

/// Parameters of [`parabolic_dish`].
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct DishParams {
    /// Reflector diameter D (m).
    pub diameter_m: f64,
    /// Aperture efficiency η (default 0.55).
    #[serde(default = "default_efficiency")]
    pub efficiency: f64,
    /// Radiation pattern envelope.
    #[serde(default)]
    pub envelope: DishEnvelope,
}

impl Default for DishParams {
    fn default() -> Self {
        Self { diameter_m: 0.6, efficiency: 0.55, envelope: DishEnvelope::F699Peak }
    }
}

/// Elevation above the horizon (deg) and front/back flag for an MSI vertical index.
fn vertical_index_geometry(i: usize) -> (f64, bool) {
    let i = i as f64;
    if i <= 90.0 {
        (-i, true)
    } else if i >= 270.0 {
        (360.0 - i, true)
    } else {
        (i - 180.0, false)
    }
}

/// F.1336-5 eq. (1e): elevation in the tilted pattern frame for an electrical downtilt β.
fn electrically_tilted_elevation(theta_h: f64, beta: f64) -> f64 {
    if beta == 0.0 {
        return theta_h;
    }
    let x = theta_h + beta;
    if x >= 0.0 {
        90.0 * x / (90.0 + beta)
    } else {
        90.0 * x / (90.0 - beta)
    }
}

fn base_pattern(name: String, f_mhz: f64, gain_dbi: f64) -> AntennaPattern {
    let mut p = AntennaPattern::flat(name, f_mhz, gain_dbi);
    p.make = "Generic".into();
    p
}

/// Isotropic radiator (0 dBi, flat cuts).
pub fn isotropic(f_mhz: f64) -> AntennaPattern {
    let mut p = base_pattern("Isotropic".into(), f_mhz, 0.0);
    p.comment = "Ideal isotropic radiator".into();
    p
}

/// Vertical half-wave dipole: 2.15 dBi, omnidirectional in azimuth, elevation pattern
/// `|cos(π/2·cos θ')/sin θ'|` with θ' the angle from the dipole axis (attenuation capped at
/// 40 dB at zenith/nadir). 3 dB vertical beamwidth ≈ 78°.
pub fn half_wave_dipole(f_mhz: f64) -> AntennaPattern {
    let mut p = base_pattern("Half-wave dipole".into(), f_mhz, DIPOLE_GAIN_DBI);
    p.polarization = "Vertical".into();
    p.comment = "Vertical lambda/2 dipole".into();
    for i in 0..PATTERN_SAMPLES {
        let (el, _) = vertical_index_geometry(i);
        let th = (90.0 - el).to_radians();
        let s = th.sin();
        let e = if s.abs() < 1e-9 { 0.0 } else { ((std::f64::consts::FRAC_PI_2 * th.cos()).cos() / s).abs() };
        p.vertical[i] = if e <= 1e-2 { 40.0 } else { (-20.0 * e.log10()).clamp(0.0, 40.0) as f32 };
    }
    p.v_beamwidth_deg = p.measured_v_beamwidth_deg();
    p
}

/// F.1336 relative gain (dB, ≤ 0) of an omnidirectional antenna at elevation θ (deg)
/// relative to the beam peak.
fn f1336_omni_relative(theta: f64, theta3: f64, k: f64, env: SidelobeEnvelope) -> f64 {
    let t = theta.abs().min(90.0);
    match env {
        SidelobeEnvelope::Peak => {
            let theta4 = theta3 * (1.0 - (k + 1.0).log10() / 1.2).max(0.0).sqrt();
            if t < theta4 {
                -12.0 * (t / theta3).powi(2)
            } else if t < theta3 {
                -12.0 + 10.0 * (k + 1.0).log10()
            } else {
                -12.0 + 10.0 * ((t / theta3).powf(-1.5) + k).log10()
            }
        }
        SidelobeEnvelope::Average => {
            let theta5 = theta3 * (1.25 - (k + 1.0).log10() / 1.2).max(0.0).sqrt();
            if t < theta3 {
                -12.0 * (t / theta3).powi(2)
            } else if t < theta5 {
                -15.0 + 10.0 * (k + 1.0).log10()
            } else {
                -15.0 + 10.0 * ((t / theta3).powf(-1.5) + k).log10()
            }
        }
    }
}

/// Elevation 3 dB beamwidth (deg) of an omni of gain `g0_dbi`: F.1336 eq. (1b),
/// `θ3 = 107.6·10^(−0.1·G0)` (limited to 90°).
pub fn omni_vertical_beamwidth_deg(g0_dbi: f64) -> f64 {
    (107.6 * 10f64.powf(-0.1 * g0_dbi)).min(90.0)
}

/// Omnidirectional collinear antenna, ITU-R F.1336-5 recommends 2.1/2.2.
pub fn omni_collinear(f_mhz: f64, params: &OmniParams) -> AntennaPattern {
    let g0 = params.gain_dbi;
    let theta3 = omni_vertical_beamwidth_deg(g0);
    let k = params.k.unwrap_or(if f_mhz < 3000.0 { 0.7 } else { 0.0 });
    let mut p = base_pattern(format!("Generic omni {} dBi", trim(g0)), f_mhz, g0);
    p.polarization = "Vertical".into();
    p.electrical_tilt_deg = params.electrical_tilt_deg;
    p.comment = format!("ITU-R F.1336-5 omni ({:?} side-lobes), k={k}, theta3={theta3:.1} deg", params.sidelobe);
    for i in 0..PATTERN_SAMPLES {
        let (el, _) = vertical_index_geometry(i);
        let te = electrically_tilted_elevation(el, params.electrical_tilt_deg);
        p.vertical[i] = (-f1336_omni_relative(te, theta3, k, params.sidelobe)).max(0.0) as f32;
    }
    p.h_beamwidth_deg = 360.0;
    p.v_beamwidth_deg = theta3;
    p
}

struct SectorModel {
    phi3: f64,
    theta3: f64,
    kh: f64,
    kv: f64,
    env: SidelobeEnvelope,
    g180: f64,
    c: f64,
    lambda_kh: f64,
    lambda_kv: f64,
    xk: f64,
}

impl SectorModel {
    fn new(phi3: f64, theta3: f64, improved: bool, env: SidelobeEnvelope) -> Self {
        let kh = if improved { 0.7 } else { 0.8 };
        let kv = if improved { 0.3 } else { 0.7 };
        let kpa: f64 = 0.7;
        let base = match env {
            SidelobeEnvelope::Peak => -12.0,
            SidelobeEnvelope::Average => -15.0,
        };
        let g180 = base + 10.0 * (1.0 + 8.0 * kpa).log10() - 15.0 * (180.0 / theta3).log10();
        let k4 = 4f64.powf(-1.5) + kv;
        let c = if theta3 < 22.5 {
            10.0 * ((180.0 / theta3).powf(1.5) * k4 / (1.0 + 8.0 * kpa)).log10() / (22.5 / theta3).log10()
        } else {
            0.0
        };
        let lambda_kv = 12.0 - c * 4f64.log10() - 10.0 * k4.log10();
        let lambda_kh = 3.0 * (1.0 - 0.5f64.powf(-kh));
        let xk = match env {
            SidelobeEnvelope::Peak => (1.0 - 0.36 * kv).sqrt(),
            SidelobeEnvelope::Average => (1.33 - 0.33 * kv).sqrt(),
        };
        Self { phi3, theta3, kh, kv, env, g180, c, lambda_kh, lambda_kv, xk }
    }

    /// Ghr(xh), eq. (2b2)/(2c2).
    fn ghr(&self, xh: f64) -> f64 {
        let g = if xh <= 0.5 { -12.0 * xh * xh } else { -12.0 * xh.powf(2.0 - self.kh) - self.lambda_kh };
        g.max(self.g180)
    }

    /// Gvr(xv), eq. (2b3)/(2c3).
    fn gvr(&self, xv: f64) -> f64 {
        let xlim = 90.0 / self.theta3;
        let g = if xv < self.xk {
            -12.0 * xv * xv
        } else if xv >= xlim && xlim > self.xk {
            self.g180
        } else if xv < 4.0 {
            let base = match self.env {
                SidelobeEnvelope::Peak => -12.0,
                SidelobeEnvelope::Average => -15.0,
            };
            base + 10.0 * (xv.powf(-1.5) + self.kv).log10()
        } else {
            let extra = match self.env {
                SidelobeEnvelope::Peak => 0.0,
                SidelobeEnvelope::Average => -3.0,
            };
            -self.lambda_kv + extra - self.c * xv.log10()
        };
        g.max(self.g180)
    }

    /// Relative gain (dB, ≤ 0) at azimuth φ and elevation θ (deg, beam frame), eq. (2a1).
    fn relative(&self, phi: f64, theta: f64) -> f64 {
        let xh = phi.abs() / self.phi3;
        let xv = theta.abs() / self.theta3;
        let gh180 = self.ghr(180.0 / self.phi3);
        let gh = self.ghr(xh);
        let r = if gh180.abs() < 1e-12 { 1.0 } else { (gh - gh180) / (0.0 - gh180) };
        gh + r * self.gvr(xv)
    }
}

/// Vertical beamwidth (deg) of a sector from F.1336 eq. (3a): `θ3 = 31000·10^(−0.1·G0)/φ3`.
pub fn sector_vertical_beamwidth_deg(g0_dbi: f64, h_beamwidth_deg: f64) -> f64 {
    31_000.0 * 10f64.powf(-0.1 * g0_dbi) / h_beamwidth_deg
}

/// Sector / panel antenna, ITU-R F.1336-5 recommends 3.1 (peak 3.1.1 or average 3.1.2).
///
/// The horizontal cut is `Ghr(φ/φ3)` (beam axis plane); the vertical cut is evaluated at
/// φ = 0 in front and φ = 180° behind, with the electrical tilt applied through eq. (1e).
/// All attenuations are capped at `front_to_back_db`.
pub fn sector(f_mhz: f64, params: &SectorParams) -> AntennaPattern {
    let g0 = params.gain_dbi;
    let phi3 = params.h_beamwidth_deg.clamp(1.0, 360.0);
    let theta3 = params.v_beamwidth_deg.unwrap_or_else(|| sector_vertical_beamwidth_deg(g0, phi3)).clamp(0.5, 90.0);
    let m = SectorModel::new(phi3, theta3, params.improved_sidelobes, params.sidelobe);
    let fb = params.front_to_back_db.max(0.0);
    let mut p = base_pattern(format!("Generic sector {}deg {} dBi", trim(phi3), trim(g0)), f_mhz, g0);
    p.polarization = "Slant +/-45".into();
    p.electrical_tilt_deg = params.electrical_tilt_deg;
    p.comment = format!(
        "ITU-R F.1336-5 sectoral ({:?} side-lobes{}), theta3={theta3:.1} deg",
        params.sidelobe,
        if params.improved_sidelobes { ", improved" } else { "" }
    );
    for i in 0..PATTERN_SAMPLES {
        let phi = if i <= 180 { i as f64 } else { i as f64 - 360.0 };
        p.horizontal[i] = (-m.relative(phi, 0.0)).clamp(0.0, fb) as f32;
        let (el, front) = vertical_index_geometry(i);
        let te = electrically_tilted_elevation(el, params.electrical_tilt_deg);
        let rel = m.relative(if front { 0.0 } else { 180.0 }, te);
        p.vertical[i] = (-rel).clamp(0.0, fb) as f32;
    }
    p.h_beamwidth_deg = phi3;
    p.v_beamwidth_deg = theta3;
    p.front_to_back_db = (p.horizontal[180] - p.horizontal[0]) as f64;
    p
}

/// Yagi-Uda antenna (empirical approximation, see [`YagiParams`]).
pub fn yagi(f_mhz: f64, params: &YagiParams) -> AntennaPattern {
    let g = params.gain_dbi;
    let product = 31_000.0 * 10f64.powf(-0.1 * g);
    let (hbw, vbw) = match (params.h_beamwidth_deg, params.v_beamwidth_deg) {
        (Some(h), Some(v)) => (h, v),
        (Some(h), None) => (h, product / h),
        (None, Some(v)) => (product / v, v),
        (None, None) => (product.sqrt(), product.sqrt()),
    };
    let (hbw, vbw) = (hbw.clamp(1.0, 180.0), vbw.clamp(1.0, 180.0));
    let sl = params.sidelobe_db.max(0.0);
    let fb = params.front_to_back_db.max(0.0);
    let att = |psi: f64, bw: f64| -> f32 {
        let psi = psi.abs().min(180.0);
        let main = 12.0 * (psi / bw).powi(2);
        let cap = if psi <= 90.0 { sl } else { sl + (fb - sl) * (psi - 90.0) / 90.0 };
        main.min(cap).max(0.0) as f32
    };
    let mut p = base_pattern(format!("Generic yagi {} dBi", trim(g)), f_mhz, g);
    p.polarization = "Linear".into();
    p.comment = format!("Empirical yagi: 12(psi/bw)^2 main lobe, {sl} dB side-lobe plateau, {fb} dB F/B");
    for i in 0..PATTERN_SAMPLES {
        let phi = if i <= 180 { i as f64 } else { 360.0 - i as f64 };
        p.horizontal[i] = att(phi, hbw);
        let (el, front) = vertical_index_geometry(i);
        let psi = if front { el.abs() } else { 180.0 - el.abs() };
        p.vertical[i] = att(psi, vbw);
    }
    p.h_beamwidth_deg = hbw;
    p.v_beamwidth_deg = vbw;
    p.front_to_back_db = (p.horizontal[180] - p.horizontal[0]) as f64;
    p
}

/// Peak gain (dBi) of a parabolic reflector: `10·log10(η·(π·D/λ)²)`.
pub fn dish_gain_dbi(diameter_m: f64, f_mhz: f64, efficiency: f64) -> f64 {
    let dl = diameter_m / wavelength_m(f_mhz);
    10.0 * (efficiency * (std::f64::consts::PI * dl).powi(2)).log10()
}

/// ITU-R F.699-9 reference gain (dBi) at off-axis angle `phi` (deg).
pub fn f699_gain_dbi(phi: f64, gmax: f64, d_over_lambda: f64, f_mhz: f64) -> f64 {
    let dl = d_over_lambda;
    let phi = phi.abs().min(180.0);
    let g1 = 2.0 + 15.0 * dl.log10();
    let phim = 20.0 / dl * (gmax - g1).max(0.0).sqrt();
    let main = gmax - 2.5e-3 * (dl * phi).powi(2);
    if phi < phim {
        return main;
    }
    let lphi = phi.max(1e-6).log10();
    let high = f_mhz >= 70_000.0;
    let g = if f_mhz < 1000.0 {
        // recommends 2.3 (100 MHz – 1 GHz, D/λ > 0.63)
        let phis = 144.5 * dl.powf(-0.2);
        if phi < 100.0 / dl {
            g1
        } else if phi < phis {
            52.0 - 10.0 * dl.log10() - 25.0 * lphi
        } else {
            -2.0 - 5.0 * dl.log10()
        }
    } else if dl > 100.0 {
        let phir = 15.85 * dl.powf(-0.6);
        let edge = if high { 120.0 } else { 48.0 };
        if phi < phir {
            g1
        } else if phi < edge {
            32.0 - 25.0 * lphi
        } else if high {
            -20.0
        } else {
            -10.0
        }
    } else {
        let edge = if high { 120.0 } else { 48.0 };
        if phi < 100.0 / dl {
            g1
        } else if phi < edge {
            52.0 - 10.0 * dl.log10() - 25.0 * lphi
        } else if high {
            -10.0 * dl.log10()
        } else {
            10.0 - 10.0 * dl.log10()
        }
    };
    g.min(gmax)
}

/// ITU-R F.1245-3 average gain (dBi) at off-axis angle `phi` (deg), 1–86 GHz.
/// Below 1 GHz the F.699-9 envelope is returned.
pub fn f1245_gain_dbi(phi: f64, gmax: f64, d_over_lambda: f64, f_mhz: f64) -> f64 {
    if f_mhz < 1000.0 {
        return f699_gain_dbi(phi, gmax, d_over_lambda, f_mhz);
    }
    let dl = d_over_lambda;
    let phi = phi.abs().min(180.0);
    let g1 = 2.0 + 15.0 * dl.log10();
    let phim = 20.0 / dl * (gmax - g1).max(0.0).sqrt();
    if phi < phim {
        return gmax - 2.5e-3 * (dl * phi).powi(2);
    }
    let lphi = phi.max(1e-6).log10();
    let high = f_mhz >= 70_000.0;
    let edge = if high { 120.0 } else { 48.0 };
    let g = if dl > 100.0 {
        let phir = 12.02 * dl.powf(-0.6);
        if phi < phim.max(phir) {
            g1
        } else if phi < edge {
            29.0 - 25.0 * lphi
        } else if high {
            -23.0
        } else {
            -13.0
        }
    } else if phi < edge {
        39.0 - 5.0 * dl.log10() - 25.0 * lphi
    } else if high {
        -13.0 - 5.0 * dl.log10()
    } else {
        -3.0 - 5.0 * dl.log10()
    };
    g.min(gmax)
}

/// Parabolic dish antenna, pattern per ITU-R F.699-9 (default) or F.1245-3, identical in the
/// horizontal and vertical planes.
pub fn parabolic_dish(f_mhz: f64, params: &DishParams) -> AntennaPattern {
    let dl = params.diameter_m / wavelength_m(f_mhz);
    let gmax = dish_gain_dbi(params.diameter_m, f_mhz, params.efficiency);
    let g_at = |phi: f64| match params.envelope {
        DishEnvelope::F699Peak => f699_gain_dbi(phi, gmax, dl, f_mhz),
        DishEnvelope::F1245Average => f1245_gain_dbi(phi, gmax, dl, f_mhz),
    };
    let mut p = base_pattern(
        format!("Generic dish {} m ({:.1} dBi @ {} MHz)", trim(params.diameter_m), gmax, trim(f_mhz)),
        f_mhz,
        gmax,
    );
    p.polarization = "Linear".into();
    p.comment = format!(
        "{} envelope, D/lambda={dl:.2}, efficiency={}",
        match params.envelope {
            DishEnvelope::F699Peak => "ITU-R F.699-9",
            DishEnvelope::F1245Average => "ITU-R F.1245-3",
        },
        params.efficiency
    );
    let mut cut = vec![0f32; PATTERN_SAMPLES];
    for (i, c) in cut.iter_mut().enumerate() {
        let psi = if i <= 180 { i as f64 } else { 360.0 - i as f64 };
        *c = (gmax - g_at(psi)).max(0.0) as f32;
    }
    p.horizontal = cut.clone();
    // vertical index i: off-axis angle is the same mapping (0 front, 180 back)
    p.vertical = cut;
    let bw = (2.0 * (3.0f64 / 2.5e-3).sqrt() / dl).min(360.0);
    p.h_beamwidth_deg = bw;
    p.v_beamwidth_deg = bw;
    p.front_to_back_db = (p.horizontal[180] - p.horizontal[0]) as f64;
    p
}

fn trim(x: f64) -> String {
    let s = format!("{x:.2}");
    s.trim_end_matches('0').trim_end_matches('.').to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::antenna::InterpolationMethod;

    #[test]
    fn dipole_pattern() {
        let p = half_wave_dipole(150.0);
        assert_eq!(p.gain_dbi, 2.15);
        assert_eq!(p.vertical[0], 0.0);
        // 45° elevation: |cos(π/2·cos45°)/sin45°| = 0.6274 → 4.05 dB
        assert!((p.vertical[45] - 4.05).abs() < 0.01, "{}", p.vertical[45]);
        assert!((p.vertical[315] - 4.05).abs() < 0.01);
        assert_eq!(p.vertical[90], 40.0);
        assert!((p.v_beamwidth_deg - 78.0).abs() < 1.0, "{}", p.v_beamwidth_deg);
        assert!(p.horizontal.iter().all(|x| *x == 0.0));
        assert_eq!(isotropic(100.0).gain_dbi, 0.0);
    }

    #[test]
    fn omni_f1336() {
        // 9 dBi → θ3 = 107.6·10^-0.9 = 13.546°
        let p = omni_collinear(450.0, &OmniParams { gain_dbi: 9.0, ..Default::default() });
        let t3 = 107.6 * 10f64.powf(-0.9);
        assert!((p.v_beamwidth_deg - 13.546).abs() < 0.001);
        // −3 dB at θ3/2 (−12·0.25 = −3)
        let half = p.v_beamwidth_deg / 2.0;
        assert!((p.vertical_front_db(half) - 3.0).abs() < 0.05, "{}", p.vertical_front_db(half));
        // k = 0.7 below 3 GHz: side-lobe plateau −12 + 10log(1.7) = −9.70 between θ4 and θ3
        let theta4 = t3 * (1.0 - 1.7f64.log10() / 1.2).sqrt();
        let idx = ((theta4 + t3) / 2.0).round() as usize;
        assert!((p.vertical[idx] as f64 - (12.0 - 10.0 * 1.7f64.log10())).abs() < 1e-4);
        // far: θ=90: −12 + 10 log((90/13.517)^-1.5 + 0.7)
        let far = 12.0 - 10.0 * ((90.0 / t3).powf(-1.5) + 0.7).log10();
        assert!((p.vertical[90] as f64 - far).abs() < 1e-3);
        // measured beamwidth ≈ θ3
        assert!((p.measured_v_beamwidth_deg() - t3).abs() < 0.2);
        // electrical tilt moves the peak down
        let t = omni_collinear(450.0, &OmniParams { gain_dbi: 9.0, electrical_tilt_deg: 4.0, ..Default::default() });
        assert_eq!(t.vertical[4], 0.0);
        assert!(t.vertical[0] > 0.0);
        // k = 0 above 3 GHz
        let h = omni_collinear(5800.0, &OmniParams { gain_dbi: 12.0, ..Default::default() });
        assert!(h.comment.contains("k=0"));
    }

    #[test]
    fn sector_f1336() {
        let sp = SectorParams { gain_dbi: 18.0, h_beamwidth_deg: 65.0, ..Default::default() };
        let p = sector(1950.0, &sp);
        // θ3 = 31000·10^-1.8/65 = 7.559°
        let t3 = 31000.0 * 10f64.powf(-1.8) / 65.0;
        assert!((p.v_beamwidth_deg - t3).abs() < 1e-9 && (t3 - 7.559).abs() < 1e-3);
        // −3 dB at φ = 32.5° (xh = 0.5 → −12·0.25)
        assert!((p.horizontal_attenuation_db(32.5) - 3.0).abs() < 0.02);
        assert!((p.measured_h_beamwidth_deg() - 65.0).abs() < 0.5);
        // vertical −3 dB at θ3/2
        assert!((p.vertical_front_db(-p.v_beamwidth_deg / 2.0) - 3.0).abs() < 0.1);
        // G180 = −12 + 10log(6.6) − 15log(180/7.56) = −24.46 → limited by F/B 25 → 24.46
        let g180 = -12.0 + 10.0 * 6.6f64.log10() - 15.0 * (180.0 / t3).log10();
        assert!((p.horizontal[180] as f64 + g180).abs() < 1e-3, "{} vs {}", p.horizontal[180], g180);
        // F/B floor
        let p2 = sector(1950.0, &SectorParams { front_to_back_db: 20.0, ..sp });
        assert!(p2.horizontal.iter().all(|x| *x <= 20.0));
        // continuity of Ghr at xh = 0.5
        let m = SectorModel::new(65.0, 7.56, false, SidelobeEnvelope::Peak);
        assert!((m.ghr(0.5 - 1e-9) - m.ghr(0.5 + 1e-9)).abs() < 1e-6);
        // continuity of Gvr at 4 and at 90/θ3
        assert!((m.gvr(4.0 - 1e-9) - m.gvr(4.0 + 1e-9)).abs() < 1e-6);
        let xl = 90.0 / 7.56;
        assert!((m.gvr(xl - 1e-9) - m.gvr(xl + 1e-9)).abs() < 1e-6);
        let ma = SectorModel::new(65.0, 7.56, true, SidelobeEnvelope::Average);
        assert!((ma.gvr(4.0 - 1e-9) - ma.gvr(4.0 + 1e-9)).abs() < 1e-6);
        assert!((ma.gvr(xl - 1e-9) - ma.gvr(xl + 1e-9)).abs() < 1e-6);
        // 3-D gain at boresight
        assert!((p.gain_dbi_with(0.0, 0.0, InterpolationMethod::Weighted) - 18.0).abs() < 1e-9);
        // electrical tilt
        let t = sector(1950.0, &SectorParams { electrical_tilt_deg: 6.0, ..sp });
        assert_eq!(t.vertical[6], 0.0);
    }

    #[test]
    fn yagi_approx() {
        let p = yagi(150.0, &YagiParams { gain_dbi: 10.0, ..Default::default() });
        let bw = (31000.0f64 / 10.0).sqrt();
        assert!((p.h_beamwidth_deg - bw).abs() < 1e-9);
        assert!((p.horizontal_attenuation_db(bw / 2.0) - 3.0).abs() < 0.05);
        assert!((p.horizontal[180] - 18.0).abs() < 1e-6);
        assert!((p.horizontal[90] - 15.0).abs() < 1e-6);
        assert!((p.vertical[180] - 18.0).abs() < 1e-6);
    }

    #[test]
    fn dish_gain_and_f699() {
        // 0.6 m at 23 GHz, η=0.55: D/λ = 46.03 → G = 10log(0.55·(π·46.03)²) = 40.6 dBi
        let g = dish_gain_dbi(0.6, 23_000.0, 0.55);
        assert!((g - 40.6).abs() < 0.1, "{g}");
        // 1.8 m at 6 GHz: D/λ = 36.0, G ≈ 38.5
        assert!((dish_gain_dbi(1.8, 6000.0, 0.55) - 38.48).abs() < 0.05);
        let p = parabolic_dish(23_000.0, &DishParams { diameter_m: 0.6, ..Default::default() });
        // half-power beamwidth = 69.28/(D/λ)
        let dl = 0.6 / wavelength_m(23_000.0);
        assert!((p.h_beamwidth_deg - 69.282 / dl).abs() < 1e-3);
        // 1° samples are exact; (note: a 1.5° beam is under-sampled by the 1° MSI grid)
        assert!((p.horizontal[1] as f64 - 2.5e-3 * dl * dl).abs() < 1e-3);
        // F.699 D/λ ≤ 100 far region: 10 − 10log(D/λ) dBi
        let far = 10.0 - 10.0 * dl.log10();
        assert!((p.horizontal[120] as f64 - (g - far)).abs() < 1e-3);
        // first side-lobe level G1 = 2 + 15log(D/λ)
        let g1 = 2.0 + 15.0 * dl.log10();
        let phim = 20.0 / dl * (g - g1).sqrt();
        let between = (phim + 100.0 / dl) / 2.0;
        assert!((f699_gain_dbi(between, g, dl, 23_000.0) - g1).abs() < 1e-9);
        // continuity at φm
        assert!((f699_gain_dbi(phim - 1e-9, g, dl, 23e3) - f699_gain_dbi(phim + 1e-9, g, dl, 23e3)).abs() < 1e-6);
        // D/λ > 100: 3.0 m @ 23 GHz: far = −10 dBi; 32 − 25 log 10 = 7 dBi at 10°
        let dl2 = 3.0 / wavelength_m(23_000.0);
        let g2 = dish_gain_dbi(3.0, 23_000.0, 0.55);
        assert_eq!(f699_gain_dbi(100.0, g2, dl2, 23e3), -10.0);
        assert!((f699_gain_dbi(10.0, g2, dl2, 23e3) - 7.0).abs() < 1e-9);
        // 80 GHz: far lobe −20 dBi beyond 120°
        let dl3 = 0.6 / wavelength_m(80_000.0);
        let g3 = dish_gain_dbi(0.6, 80_000.0, 0.55);
        assert_eq!(f699_gain_dbi(150.0, g3, dl3, 80e3), -20.0);
        // F.1245 average is ≤ F.699 envelope in the side-lobe region
        for phi in [5.0, 10.0, 30.0, 60.0, 150.0] {
            assert!(f1245_gain_dbi(phi, g, dl, 23e3) <= f699_gain_dbi(phi, g, dl, 23e3) + 1e-9, "{phi}");
        }
        // UHF (< 1 GHz) case
        let pu = parabolic_dish(900.0, &DishParams { diameter_m: 1.2, ..Default::default() });
        assert!(pu.gain_dbi > 15.0 && pu.horizontal[180] > 10.0);
        let pa = parabolic_dish(
            23e3,
            &DishParams { diameter_m: 0.6, envelope: DishEnvelope::F1245Average, ..Default::default() },
        );
        assert!(pa.horizontal[90] >= p.horizontal[90]);
    }
}
