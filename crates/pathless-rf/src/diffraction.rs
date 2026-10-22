//! Diffraction loss (ITU-R P.526) and line-of-sight / Fresnel clearance analysis.
//!
//! # Profiles
//!
//! All profile functions take two equally long slices:
//!
//! * `distances_m` – distance of each sample from the transmitter (m), non-decreasing.
//!   The first sample is the transmitter site, the last one the receiver site.
//! * `heights_m` – **raw** terrain (+ clutter) height above mean sea level (m). Earth
//!   curvature is **not** included; it is applied internally from
//!   [`PathParams::k_factor`].
//!
//! Antenna heights are given **above mean sea level** in [`PathParams`]
//! (i.e. ground height at the site + antenna height above ground).
//!
//! # Methods
//!
//! * [`knife_edge_loss_db`] – P.526-16 eq. (31) approximation of J(ν).
//! * [`deygout_loss`] – three-edge Deygout construction (main edge plus one secondary edge
//!   on each side). [`DeygoutVariant::Classic`] sums the three knife-edge losses;
//!   [`DeygoutVariant::P452Empirical`] uses the ITU-R P.452-14 §4.2.1 slope corrections and
//!   empirical combination `Lm + (1 − e^(−Lm/6))·(Lt + Lr + 10 + 0.04·d)`.
//! * [`bullington_loss`] – P.526-16 §4.5.1 Bullington construction (equivalent single knife
//!   edge) including the tapered correction of eq. (57).
//! * [`delta_bullington_loss`] – P.526-16 §4.5.2 complete general-path method
//!   (Bullington + smooth spherical-earth correction, §3.2).
//! * [`spherical_earth_loss_db`] – P.526-16 §3.2 smooth-earth diffraction for any distance.
//! * [`los_clearance`] – per-sample LOS ray height, bulged terrain, first Fresnel radius and
//!   clearance ratio, for plotting.
//!
//! # Choosing a method
//!
//! P.526-16 recommends the §4.5 general method ([`delta_bullington_loss`]) for automatic
//! processing of arbitrary profiles. Deygout is provided for comparison with classic
//! planning tools; note that on finely sampled, rounded obstacles the classic construction
//! tends to pick neighbouring samples of the *same* hill as secondary edges and therefore
//! over-estimates the loss (P.452-14 accepted this and compensates empirically).

use serde::{Deserialize, Serialize};

use crate::basic::{earth_bulge_m, fresnel_radius_m, wavelength_m, EARTH_RADIUS_KM};
use crate::error::{require_positive, RfError};

/// Geometry and radio parameters shared by the profile based functions.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct PathParams {
    /// Transmitter antenna height above mean sea level (m), located at `distances_m[0]`.
    pub tx_antenna_amsl_m: f64,
    /// Receiver antenna height above mean sea level (m), located at the last sample.
    pub rx_antenna_amsl_m: f64,
    /// Frequency (MHz).
    pub frequency_mhz: f64,
    /// Effective earth radius factor k (4/3 for the standard atmosphere).
    pub k_factor: f64,
}

impl PathParams {
    /// Convenience constructor.
    pub fn new(tx_antenna_amsl_m: f64, rx_antenna_amsl_m: f64, frequency_mhz: f64, k_factor: f64) -> Self {
        Self { tx_antenna_amsl_m, rx_antenna_amsl_m, frequency_mhz, k_factor }
    }

    fn validate(&self) -> Result<(), RfError> {
        require_positive("frequency_mhz", self.frequency_mhz)?;
        require_positive("k_factor", self.k_factor)?;
        if !self.tx_antenna_amsl_m.is_finite() || !self.rx_antenna_amsl_m.is_finite() {
            return Err(RfError::invalid("antenna heights must be finite"));
        }
        Ok(())
    }
}

/// Which diffraction algorithm produced a [`DiffractionResult`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum DiffractionMethod {
    /// Classic Deygout (sum of up to three knife edges).
    DeygoutClassic,
    /// Deygout with ITU-R P.452-14 empirical correction.
    DeygoutP452,
    /// ITU-R P.526-16 §4.5.1 Bullington.
    Bullington,
    /// ITU-R P.526-16 §4.5.2 delta-Bullington (Bullington + spherical earth).
    DeltaBullington,
}

/// Variant of the Deygout construction.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum DeygoutVariant {
    /// Classic Deygout (1966): `L = J(νm) + J(νt) + J(νr)` where the secondary edges are
    /// searched on the sub-paths transmitter→main edge and main edge→receiver.
    #[default]
    Classic,
    /// ITU-R P.452-14 §4.2.1: path-slope corrections ζ and
    /// `L = Lm + (1 − exp(−Lm/6))·(Lt + Lr + 10 + 0.04·d_km)` (zero if Lm = 0).
    P452Empirical,
}

/// Role of an edge in a multiple-edge diffraction result.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum EdgeRole {
    /// Principal (dominant) Deygout edge, or the dominant profile point of a LOS Bullington path.
    Main,
    /// Secondary Deygout edge between transmitter and main edge.
    TxSide,
    /// Secondary Deygout edge between main edge and receiver.
    RxSide,
    /// Virtual Bullington edge (intersection of the two horizon rays).
    BullingtonVirtual,
    /// Transmitter horizon point (trans-horizon Bullington path).
    TxHorizon,
    /// Receiver horizon point (trans-horizon Bullington path).
    RxHorizon,
}

/// One diffracting edge.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DiffractionEdge {
    /// Profile sample index (None for the virtual Bullington edge).
    pub index: Option<usize>,
    /// Distance from the transmitter (m).
    pub distance_m: f64,
    /// Height above the relevant ray line (m), including earth bulge; positive = obstructing.
    pub clearance_deficit_m: f64,
    /// Fresnel-Kirchhoff diffraction parameter ν.
    pub v: f64,
    /// Knife-edge loss J(ν) of this edge alone (dB).
    pub loss_db: f64,
    /// Role of the edge.
    pub role: EdgeRole,
}

/// Result of a profile diffraction calculation.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DiffractionResult {
    /// Algorithm used.
    pub method: DiffractionMethod,
    /// Diffraction loss relative to free space (dB, ≥ 0).
    pub loss_db: f64,
    /// True if no profile sample intersects the straight ray (with earth bulge).
    pub line_of_sight: bool,
    /// Index of the dominant obstruction (largest ν), if the path has intermediate samples.
    pub dominant_index: Option<usize>,
    /// ν of the dominant obstruction (or of the Bullington point); None without
    /// intermediate samples.
    pub dominant_v: Option<f64>,
    /// Contributing edges (may be empty).
    pub edges: Vec<DiffractionEdge>,
}

/// Single knife-edge diffraction loss J(ν) (dB), ITU-R P.526-16 eq. (31):
///
/// `J(ν) = 6.9 + 20·log10( sqrt((ν − 0.1)² + 1) + ν − 0.1 )` for ν > −0.78, else 0.
pub fn knife_edge_loss_db(v: f64) -> f64 {
    if v <= -0.78 {
        return 0.0;
    }
    let t = v - 0.1;
    (6.9 + 20.0 * ((t * t + 1.0).sqrt() + t).log10()).max(0.0)
}

/// Fresnel-Kirchhoff diffraction parameter ν, P.526-16 eq. (26):
/// `ν = h·sqrt( (2/λ)·(1/d1 + 1/d2) )`.
///
/// * `h_m` – height of the edge above the direct ray (m), negative if below.
/// * `d1_m`, `d2_m` – distances from the terminals to the edge (m).
/// * `f_mhz` – frequency (MHz).
pub fn fresnel_parameter_v(h_m: f64, d1_m: f64, d2_m: f64, f_mhz: f64) -> f64 {
    let lambda = wavelength_m(f_mhz);
    h_m * ((2.0 / lambda) * (1.0 / d1_m + 1.0 / d2_m)).sqrt()
}

/// Convenience: loss (dB) of a single knife edge `h_m` above the ray.
pub fn single_knife_edge_loss_db(h_m: f64, d1_m: f64, d2_m: f64, f_mhz: f64) -> f64 {
    knife_edge_loss_db(fresnel_parameter_v(h_m, d1_m, d2_m, f_mhz))
}

/// Validate a profile and return distances relative to the first sample, in km.
fn prepare_profile(distances_m: &[f64], heights_m: &[f64]) -> Result<Vec<f64>, RfError> {
    if distances_m.len() != heights_m.len() {
        return Err(RfError::InvalidProfile(format!(
            "distances ({}) and heights ({}) differ in length",
            distances_m.len(),
            heights_m.len()
        )));
    }
    if distances_m.len() < 2 {
        return Err(RfError::InvalidProfile("profile needs at least 2 samples".into()));
    }
    if distances_m.iter().chain(heights_m.iter()).any(|x| !x.is_finite()) {
        return Err(RfError::InvalidProfile("profile contains non-finite values".into()));
    }
    if distances_m.windows(2).any(|w| w[1] < w[0]) {
        return Err(RfError::InvalidProfile("distances must be non-decreasing".into()));
    }
    let d0 = distances_m[0];
    let total = distances_m[distances_m.len() - 1] - d0;
    if total <= 0.0 {
        return Err(RfError::InvalidProfile("path length must be > 0".into()));
    }
    Ok(distances_m.iter().map(|d| (d - d0) / 1000.0).collect())
}

/// Index range of intermediate samples strictly inside (0, d).
fn interior(dk: &[f64]) -> impl Iterator<Item = usize> + '_ {
    let d = dk[dk.len() - 1];
    (1..dk.len() - 1).filter(move |&i| dk[i] > 0.0 && dk[i] < d)
}

struct EdgeSearch {
    index: usize,
    v: f64,
    h: f64,
}

/// Find the edge with maximum ν on the sub-path between samples `a` and `b`
/// (P.452-14 eqs. (15), (18), (21) generalised). `zeta` applies the slope correction.
#[allow(clippy::too_many_arguments)]
fn max_v_subpath(
    dk: &[f64],
    h: &[f64],
    a: usize,
    b: usize,
    ha: f64,
    hb: f64,
    ae_km: f64,
    lambda: f64,
    zeta_correction: bool,
) -> Option<EdgeSearch> {
    let dab = dk[b] - dk[a];
    if b <= a + 1 || dab <= 0.0 {
        return None;
    }
    let zeta = if zeta_correction { (1e-3 * (hb - ha) / dab).atan().cos() } else { 1.0 };
    let mut best: Option<EdgeSearch> = None;
    for i in (a + 1)..b {
        let di = dk[i] - dk[a];
        let dj = dk[b] - dk[i];
        if di <= 0.0 || dj <= 0.0 {
            continue;
        }
        let hi = h[i] + 1e3 * di * dj / (2.0 * ae_km) - (ha * dj + hb * di) / dab;
        let v = zeta * hi * (2e-3 * dab / (lambda * di * dj)).sqrt();
        if best.as_ref().is_none_or(|e| v > e.v) {
            best = Some(EdgeSearch { index: i, v, h: hi });
        }
    }
    best
}

/// Deygout multiple knife-edge diffraction loss over a terrain profile (up to 3 edges).
///
/// See the [module documentation](self) for profile conventions. The principal edge is
/// the sample with the largest ν relative to the transmitter–receiver ray (earth bulge
/// included); secondary edges are searched between the transmitter and the principal
/// edge, and between the principal edge and the receiver, using the *terrain height* of
/// the principal edge as the sub-path end point (as in P.452-14).
pub fn deygout_loss(
    distances_m: &[f64],
    heights_m: &[f64],
    params: &PathParams,
    variant: DeygoutVariant,
) -> Result<DiffractionResult, RfError> {
    params.validate()?;
    let dk = prepare_profile(distances_m, heights_m)?;
    let n = dk.len();
    let d = dk[n - 1];
    let ae = EARTH_RADIUS_KM * params.k_factor;
    let lambda = wavelength_m(params.frequency_mhz);
    let p452 = variant == DeygoutVariant::P452Empirical;
    let method = if p452 { DiffractionMethod::DeygoutP452 } else { DiffractionMethod::DeygoutClassic };
    let hts = params.tx_antenna_amsl_m;
    let hrs = params.rx_antenna_amsl_m;

    let Some(main) = max_v_subpath(&dk, heights_m, 0, n - 1, hts, hrs, ae, lambda, p452) else {
        return Ok(DiffractionResult {
            method,
            loss_db: 0.0,
            line_of_sight: true,
            dominant_index: None,
            dominant_v: None,
            edges: Vec::new(),
        });
    };
    let line_of_sight = main.h <= 0.0;
    let lm = knife_edge_loss_db(main.v);
    let mk = |e: &EdgeSearch, role: EdgeRole, loss: f64| DiffractionEdge {
        index: Some(e.index),
        distance_m: distances_m[e.index] - distances_m[0],
        clearance_deficit_m: e.h,
        v: e.v,
        loss_db: loss,
        role,
    };
    let mut edges = vec![mk(&main, EdgeRole::Main, lm)];
    if lm <= 0.0 {
        return Ok(DiffractionResult {
            method,
            loss_db: 0.0,
            line_of_sight,
            dominant_index: Some(main.index),
            dominant_v: Some(main.v),
            edges,
        });
    }
    let hm = heights_m[main.index];
    let mut lt = 0.0;
    if let Some(t) = max_v_subpath(&dk, heights_m, 0, main.index, hts, hm, ae, lambda, p452) {
        lt = knife_edge_loss_db(t.v);
        if lt > 0.0 {
            edges.push(mk(&t, EdgeRole::TxSide, lt));
        }
    }
    let mut lr = 0.0;
    if let Some(r) = max_v_subpath(&dk, heights_m, main.index, n - 1, hm, hrs, ae, lambda, p452) {
        lr = knife_edge_loss_db(r.v);
        if lr > 0.0 {
            edges.push(mk(&r, EdgeRole::RxSide, lr));
        }
    }
    let loss_db = if p452 { lm + (1.0 - (-lm / 6.0).exp()) * (lt + lr + 10.0 + 0.04 * d) } else { lm + lt + lr };
    Ok(DiffractionResult {
        method,
        loss_db,
        line_of_sight,
        dominant_index: Some(main.index),
        dominant_v: Some(main.v),
        edges,
    })
}

/// Internal Bullington evaluation on km/m units (P.526-16 §4.5.1).
struct BullingtonOut {
    luc: f64,
    lb: f64,
    v: Option<f64>,
    los: bool,
    edges: Vec<DiffractionEdge>,
    dominant_index: Option<usize>,
}

fn bullington_core(dk: &[f64], h: &[f64], hts: f64, hrs: f64, ae_km: f64, lambda: f64) -> BullingtonOut {
    let n = dk.len();
    let d = dk[n - 1];
    let ce = 1.0 / ae_km;
    let bulge = |i: usize| 500.0 * ce * dk[i] * (d - dk[i]);
    // eq. (49)
    let mut stim = f64::NEG_INFINITY;
    let mut itim = None;
    for i in interior(dk) {
        let s = (h[i] + bulge(i) - hts) / dk[i];
        if s > stim {
            stim = s;
            itim = Some(i);
        }
    }
    // eq. (50)
    let str_ = (hrs - hts) / d;
    let mut edges = Vec::new();
    let (luc, v, los, dominant_index);
    if itim.is_none() || stim < str_ {
        // Case 1: LoS, eq. (51)
        let mut vmax = f64::NEG_INFINITY;
        let mut imax = None;
        let mut hmax = 0.0;
        for i in interior(dk) {
            let hi = h[i] + bulge(i) - (hts * (d - dk[i]) + hrs * dk[i]) / d;
            let vi = hi * (0.002 * d / (lambda * dk[i] * (d - dk[i]))).sqrt();
            if vi > vmax {
                vmax = vi;
                imax = Some(i);
                hmax = hi;
            }
        }
        los = true;
        dominant_index = imax;
        if let Some(i) = imax {
            luc = knife_edge_loss_db(vmax);
            v = Some(vmax);
            edges.push(DiffractionEdge {
                index: Some(i),
                distance_m: dk[i] * 1000.0,
                clearance_deficit_m: hmax,
                v: vmax,
                loss_db: luc,
                role: EdgeRole::Main,
            });
        } else {
            luc = 0.0;
            v = None;
        }
    } else {
        // Case 2: trans-horizon, eq. (53)–(56)
        let mut srim = f64::NEG_INFINITY;
        let mut irim = None;
        for i in interior(dk) {
            let s = (h[i] + bulge(i) - hrs) / (d - dk[i]);
            if s > srim {
                srim = s;
                irim = Some(i);
            }
        }
        let db = ((hrs - hts + srim * d) / (stim + srim)).clamp(1e-9, d - 1e-9);
        let hb = hts + stim * db - (hts * (d - db) + hrs * db) / d;
        let vb = hb * (0.002 * d / (lambda * db * (d - db))).sqrt();
        v = Some(vb);
        luc = knife_edge_loss_db(vb);
        los = false;
        dominant_index = itim;
        edges.push(DiffractionEdge {
            index: None,
            distance_m: db * 1000.0,
            clearance_deficit_m: hb,
            v: vb,
            loss_db: luc,
            role: EdgeRole::BullingtonVirtual,
        });
        if let Some(i) = itim {
            edges.push(DiffractionEdge {
                index: Some(i),
                distance_m: dk[i] * 1000.0,
                clearance_deficit_m: h[i] + bulge(i) - (hts * (d - dk[i]) + hrs * dk[i]) / d,
                v: 0.0,
                loss_db: 0.0,
                role: EdgeRole::TxHorizon,
            });
        }
        if let Some(i) = irim {
            edges.push(DiffractionEdge {
                index: Some(i),
                distance_m: dk[i] * 1000.0,
                clearance_deficit_m: h[i] + bulge(i) - (hts * (d - dk[i]) + hrs * dk[i]) / d,
                v: 0.0,
                loss_db: 0.0,
                role: EdgeRole::RxHorizon,
            });
        }
    }
    // eq. (57)
    let lb = luc + (1.0 - (-luc / 6.0).exp()) * (10.0 + 0.02 * d);
    BullingtonOut { luc, lb, v, los, edges, dominant_index }
}

/// Which loss value the Bullington function reports.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum BullingtonOutput {
    /// `Lb` of P.526-16 eq. (57): knife-edge loss plus the tapered correction
    /// `(1 − exp(−Luc/6))·(10 + 0.02·d_km)`.
    #[default]
    WithCorrection,
    /// `Luc` only: the plain equivalent-knife-edge loss J(νb) (classic Bullington).
    KnifeEdgeOnly,
}

/// Bullington diffraction loss, ITU-R P.526-16 §4.5.1.
///
/// The path is classified as LoS (largest-ν sample used) or trans-horizon (virtual edge at
/// the intersection of the two horizon rays). The edge list contains the dominant sample /
/// virtual edge and, for trans-horizon paths, the two horizon samples (their `v` and
/// `loss_db` fields are 0; only the virtual edge carries the loss).
pub fn bullington_loss(
    distances_m: &[f64],
    heights_m: &[f64],
    params: &PathParams,
    output: BullingtonOutput,
) -> Result<DiffractionResult, RfError> {
    params.validate()?;
    let dk = prepare_profile(distances_m, heights_m)?;
    let ae = EARTH_RADIUS_KM * params.k_factor;
    let lambda = wavelength_m(params.frequency_mhz);
    let out = bullington_core(&dk, heights_m, params.tx_antenna_amsl_m, params.rx_antenna_amsl_m, ae, lambda);
    Ok(DiffractionResult {
        method: DiffractionMethod::Bullington,
        loss_db: match output {
            BullingtonOutput::WithCorrection => out.lb,
            BullingtonOutput::KnifeEdgeOnly => out.luc,
        },
        line_of_sight: out.los && out.edges.first().is_none_or(|e| e.clearance_deficit_m <= 0.0),
        dominant_index: out.dominant_index,
        dominant_v: out.v,
        edges: out.edges,
    })
}

/// Electrical characteristics of the ground used by the smooth-earth model
/// (ITU-R P.526-16 §3.1.1.1). Only matters at low frequencies / vertical polarization.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct GroundConstants {
    /// Effective relative permittivity ε.
    pub permittivity: f64,
    /// Effective conductivity σ (S/m).
    pub conductivity_s_m: f64,
    /// True for vertical polarization, false for horizontal.
    pub vertical_polarization: bool,
}

impl GroundConstants {
    /// Average land, ε = 22, σ = 0.003 S/m (values used in ITU-R P.452), vertical polarization.
    pub const LAND_VERTICAL: GroundConstants =
        GroundConstants { permittivity: 22.0, conductivity_s_m: 0.003, vertical_polarization: true };
    /// Sea water, ε = 80, σ = 5 S/m, vertical polarization.
    pub const SEA_VERTICAL: GroundConstants =
        GroundConstants { permittivity: 80.0, conductivity_s_m: 5.0, vertical_polarization: true };
}

impl Default for GroundConstants {
    fn default() -> Self {
        Self::LAND_VERTICAL
    }
}

/// Normalized surface admittance K (P.526-16 eq. 11a/12a), `ae_km` effective radius, f MHz.
fn surface_admittance_k(ae_km: f64, f_mhz: f64, g: &GroundConstants) -> f64 {
    let x = 18_000.0 * g.conductivity_s_m / f_mhz;
    let kh = 0.36 * (ae_km * f_mhz).powf(-1.0 / 3.0) * ((g.permittivity - 1.0).powi(2) + x * x).powf(-0.25);
    if g.vertical_polarization {
        kh * (g.permittivity.powi(2) + x * x).sqrt()
    } else {
        kh
    }
}

/// First-term residue-series diffraction field relative to free space, P.526-16 §3.1.1.2,
/// returned as a loss (dB) = −20·log10(E/E0). d km, heights m, f MHz, ae km.
fn residue_series_loss_db(d_km: f64, h1_m: f64, h2_m: f64, f_mhz: f64, ae_km: f64, g: &GroundConstants) -> f64 {
    let k = surface_admittance_k(ae_km, f_mhz, g);
    let k2 = k * k;
    let k4 = k2 * k2;
    let beta = (1.0 + 1.6 * k2 + 0.67 * k4) / (1.0 + 4.5 * k2 + 1.53 * k4);
    let x = 2.188 * beta * f_mhz.powf(1.0 / 3.0) * ae_km.powf(-2.0 / 3.0) * d_km;
    let fx = if x >= 1.6 { 11.0 + 10.0 * x.log10() - 17.6 * x } else { -20.0 * x.log10() - 5.6488 * x.powf(1.425) };
    let gy = |h: f64| {
        let y = 9.575e-3 * beta * f_mhz.powf(2.0 / 3.0) * ae_km.powf(-1.0 / 3.0) * h;
        let b = beta * y;
        let g = if b > 2.0 {
            17.6 * (b - 1.1).sqrt() - 5.0 * (b - 1.1).log10() - 8.0
        } else if b > 0.0 {
            20.0 * (b + 0.1 * b * b * b).log10()
        } else {
            f64::NEG_INFINITY
        };
        g.max(2.0 + 20.0 * k.log10())
    };
    -(fx + gy(h1_m) + gy(h2_m))
}

/// Smooth spherical-earth diffraction loss (dB) for any distance, ITU-R P.526-16 §3.2.
///
/// * `d_km` – path length (km)
/// * `h1_m`, `h2_m` – antenna heights above the smooth earth surface (m)
/// * `f_mhz` – frequency (MHz), valid ≥ 10 MHz
/// * `k_factor` – effective earth radius factor
pub fn spherical_earth_loss_db(
    d_km: f64,
    h1_m: f64,
    h2_m: f64,
    f_mhz: f64,
    k_factor: f64,
    ground: &GroundConstants,
) -> f64 {
    let ae_km = EARTH_RADIUS_KM * k_factor;
    let ae_m = ae_km * 1000.0;
    let h1 = h1_m.max(0.0);
    let h2 = h2_m.max(0.0);
    let d_m = d_km * 1000.0;
    // eq. (21)
    let dlos = (2.0 * ae_m).sqrt() * (h1.sqrt() + h2.sqrt());
    if d_m >= dlos {
        return residue_series_loss_db(d_km, h1, h2, f_mhz, ae_km, ground).max(0.0);
    }
    // eq. (22)
    let m = d_m * d_m / (4.0 * ae_m * (h1 + h2));
    let c = (h1 - h2) / (h1 + h2);
    let arg = (1.5 * c * (3.0 * m / (m + 1.0).powi(3)).sqrt()).clamp(-1.0, 1.0);
    let b = 2.0 * ((m + 1.0) / (3.0 * m)).sqrt() * (std::f64::consts::PI / 3.0 + arg.acos() / 3.0).cos();
    let d1 = d_m / 2.0 * (1.0 + b);
    let d2 = d_m - d1;
    let h = ((h1 - d1 * d1 / (2.0 * ae_m)) * d2 + (h2 - d2 * d2 / (2.0 * ae_m)) * d1) / d_m;
    // eq. (23)
    let hreq = 0.552 * (d1 * d2 * wavelength_m(f_mhz) / d_m).sqrt();
    if h > hreq {
        return 0.0;
    }
    // eq. (24)
    let aem_m = 0.5 * (d_m / (h1.sqrt() + h2.sqrt())).powi(2);
    let ah = residue_series_loss_db(d_km, h1, h2, f_mhz, aem_m / 1000.0, ground);
    if ah < 0.0 {
        return 0.0;
    }
    // eq. (25)
    ((1.0 - h / hreq) * ah).max(0.0)
}

/// Delta-Bullington general terrestrial path diffraction loss, ITU-R P.526-16 §4.5.2:
/// `L = Lba + max(Lsph − Lbs, 0)`.
///
/// `Lba` is the Bullington loss (eq. 57) of the actual profile, `Lbs` the Bullington loss
/// of a smooth (zero-height) profile with antenna heights relative to a least-squares
/// smooth surface, and `Lsph` the §3.2 spherical-earth loss for those heights.
pub fn delta_bullington_loss(
    distances_m: &[f64],
    heights_m: &[f64],
    params: &PathParams,
    ground: &GroundConstants,
) -> Result<DiffractionResult, RfError> {
    params.validate()?;
    let dk = prepare_profile(distances_m, heights_m)?;
    let n = dk.len();
    let d = dk[n - 1];
    let ae = EARTH_RADIUS_KM * params.k_factor;
    let lambda = wavelength_m(params.frequency_mhz);
    let hts = params.tx_antenna_amsl_m;
    let hrs = params.rx_antenna_amsl_m;
    let actual = bullington_core(&dk, heights_m, hts, hrs, ae, lambda);

    // eq. (58)–(60)
    let mut v1 = 0.0;
    let mut v2 = 0.0;
    for i in 1..n {
        let dd = dk[i] - dk[i - 1];
        v1 += dd * (heights_m[i] + heights_m[i - 1]);
        v2 += dd * (heights_m[i] * (2.0 * dk[i] + dk[i - 1]) + heights_m[i - 1] * (dk[i] + 2.0 * dk[i - 1]));
    }
    let hstip = (2.0 * v1 * d - v2) / (d * d);
    let hsrip = (v2 - v1 * d) / (d * d);
    // eq. (61)
    let mut hobs = f64::NEG_INFINITY;
    let mut aobt = f64::NEG_INFINITY;
    let mut aobr = f64::NEG_INFINITY;
    for i in interior(&dk) {
        let hob = heights_m[i] - (hts * (d - dk[i]) + hrs * dk[i]) / d;
        hobs = hobs.max(hob);
        aobt = aobt.max(hob / dk[i]);
        aobr = aobr.max(hob / (d - dk[i]));
    }
    // eq. (62)
    let (hstp, hsrp) = if hobs <= 0.0 {
        (hstip, hsrip)
    } else {
        let gt = aobt / (aobt + aobr);
        let gr = aobr / (aobt + aobr);
        (hstip - hobs * gt, hsrip - hobs * gr)
    };
    // eq. (63)
    let hst = hstp.min(heights_m[0]);
    let hsr = hsrp.min(heights_m[n - 1]);
    // eq. (64)
    let hts_p = hts - hst;
    let hrs_p = hrs - hsr;
    let zeros = vec![0.0; n];
    let smooth = bullington_core(&dk, &zeros, hts_p, hrs_p, ae, lambda);
    let lsph = spherical_earth_loss_db(d, hts_p, hrs_p, params.frequency_mhz, params.k_factor, ground);
    let loss_db = actual.lb + (lsph - smooth.lb).max(0.0);
    Ok(DiffractionResult {
        method: DiffractionMethod::DeltaBullington,
        loss_db,
        line_of_sight: actual.los && actual.edges.first().is_none_or(|e| e.clearance_deficit_m <= 0.0),
        dominant_index: actual.dominant_index,
        dominant_v: actual.v,
        edges: actual.edges,
    })
}

/// One sample of a [`LosClearance`] analysis.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ClearancePoint {
    /// Distance from the transmitter (m).
    pub distance_m: f64,
    /// Raw terrain (+clutter) height AMSL (m), as supplied.
    pub terrain_m: f64,
    /// Earth bulge at this point for the path k-factor (m).
    pub bulge_m: f64,
    /// Terrain height including earth bulge (m) – plot this under a straight LOS ray.
    pub effective_terrain_m: f64,
    /// Height of the straight transmitter→receiver ray (m AMSL).
    pub los_height_m: f64,
    /// First Fresnel zone radius (m), 0 at the terminals.
    pub fresnel_radius_m: f64,
    /// `los_height_m − effective_terrain_m` (m); negative = ray obstructed.
    pub clearance_m: f64,
    /// `clearance_m / fresnel_radius_m` (None at the terminals where F1 = 0).
    pub clearance_ratio: Option<f64>,
}

/// Line-of-sight and first-Fresnel-zone clearance along a profile.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LosClearance {
    /// Per-sample data, same order as the input profile.
    pub points: Vec<ClearancePoint>,
    /// Minimum clearance ratio (clearance / F1) over intermediate samples.
    pub min_clearance_ratio: Option<f64>,
    /// Index of the sample with the minimum clearance ratio.
    pub min_index: Option<usize>,
    /// Distance (m) of the sample with the minimum clearance ratio.
    pub min_distance_m: Option<f64>,
    /// Clearance (m) at the minimum-ratio sample.
    pub min_clearance_m: Option<f64>,
    /// True if no intermediate sample blocks the straight ray (clearance ≥ 0 everywhere).
    pub line_of_sight: bool,
    /// True if the minimum clearance ratio is ≥ 0.6 (customary "free-space" criterion).
    pub fresnel_60_clear: bool,
}

/// Compute LOS ray height, bulged terrain, first Fresnel radius and clearance ratio for each
/// profile sample. The ray is a straight line between the two antennas; earth curvature is
/// represented by adding the bulge `d1·d2/(2·k·a)` to the terrain.
pub fn los_clearance(distances_m: &[f64], heights_m: &[f64], params: &PathParams) -> Result<LosClearance, RfError> {
    params.validate()?;
    let dk = prepare_profile(distances_m, heights_m)?;
    let n = dk.len();
    let d_m = dk[n - 1] * 1000.0;
    let mut points = Vec::with_capacity(n);
    let mut min_ratio: Option<f64> = None;
    let mut min_index = None;
    let mut los = true;
    for i in 0..n {
        let d1 = dk[i] * 1000.0;
        let d2 = d_m - d1;
        let bulge = if d1 > 0.0 && d2 > 0.0 { earth_bulge_m(d1, d2, params.k_factor) } else { 0.0 };
        let eff = heights_m[i] + bulge;
        let ray = params.tx_antenna_amsl_m + (params.rx_antenna_amsl_m - params.tx_antenna_amsl_m) * d1 / d_m;
        let f1 = fresnel_radius_m(1, params.frequency_mhz, d1, d2);
        let clearance = ray - eff;
        let ratio = if f1 > 0.0 { Some(clearance / f1) } else { None };
        if f1 > 0.0 {
            if clearance < 0.0 {
                los = false;
            }
            if let Some(r) = ratio {
                if min_ratio.is_none_or(|m| r < m) {
                    min_ratio = Some(r);
                    min_index = Some(i);
                }
            }
        }
        points.push(ClearancePoint {
            distance_m: distances_m[i] - distances_m[0],
            terrain_m: heights_m[i],
            bulge_m: bulge,
            effective_terrain_m: eff,
            los_height_m: ray,
            fresnel_radius_m: f1,
            clearance_m: clearance,
            clearance_ratio: ratio,
        });
    }
    Ok(LosClearance {
        min_distance_m: min_index.map(|i| points[i].distance_m),
        min_clearance_m: min_index.map(|i| points[i].clearance_m),
        fresnel_60_clear: min_ratio.is_none_or(|r| r >= 0.6),
        points,
        min_clearance_ratio: min_ratio,
        min_index,
        line_of_sight: los,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn close(a: f64, b: f64, tol: f64) -> bool {
        (a - b).abs() <= tol
    }

    #[test]
    fn j_of_v_known_values() {
        // P.526 Fig. 9 / eq. (31) values
        assert!(close(knife_edge_loss_db(0.0), 6.0328, 1e-3));
        assert!(close(knife_edge_loss_db(1.0), 13.9258, 1e-3));
        assert!(close(knife_edge_loss_db(2.4), 20.6, 0.1));
        assert!(knife_edge_loss_db(-0.78) < 0.01);
        assert_eq!(knife_edge_loss_db(-1.0), 0.0);
        // asymptote J ≈ 13 + 20 log v for large v
        assert!(close(knife_edge_loss_db(10.0), 13.0 + 20.0, 0.2));
    }

    #[test]
    fn v_parameter() {
        // h = F1 → ν = √2
        let f = 5800.0;
        let f1 = fresnel_radius_m(1, f, 4000.0, 6000.0);
        assert!(close(fresnel_parameter_v(f1, 4000.0, 6000.0, f), 2f64.sqrt(), 1e-9));
        // −0.6 F1 clearance → ν ≈ −0.849 → J = 0
        assert_eq!(single_knife_edge_loss_db(-0.6 * f1, 4000.0, 6000.0, f), 0.0);
    }

    fn flat_profile_with_edge(edge_h: f64) -> (Vec<f64>, Vec<f64>) {
        let d: Vec<f64> = (0..=100).map(|i| i as f64 * 100.0).collect();
        let mut h = vec![0.0; d.len()];
        h[50] = edge_h;
        (d, h)
    }

    #[test]
    fn single_edge_matches_knife_edge() {
        // 10 km, edge at midpoint; k huge → no bulge
        let (d, h) = flat_profile_with_edge(60.0);
        let p = PathParams::new(50.0, 50.0, 1000.0, 1e9);
        let expected = single_knife_edge_loss_db(10.0, 5000.0, 5000.0, 1000.0);
        let dg = deygout_loss(&d, &h, &p, DeygoutVariant::Classic).unwrap();
        assert!(close(dg.loss_db, expected, 1e-6), "{} vs {expected}", dg.loss_db);
        assert_eq!(dg.dominant_index, Some(50));
        assert!(!dg.line_of_sight);
        let bl = bullington_loss(&d, &h, &p, BullingtonOutput::KnifeEdgeOnly).unwrap();
        assert!(close(bl.loss_db, expected, 1e-6), "{} vs {expected}", bl.loss_db);
        // with correction: Luc + (1-exp(-Luc/6))*(10+0.02*10)
        let blc = bullington_loss(&d, &h, &p, BullingtonOutput::WithCorrection).unwrap();
        let corr = expected + (1.0 - (-expected / 6.0).exp()) * 10.2;
        assert!(close(blc.loss_db, corr, 1e-6));
    }

    #[test]
    fn clear_path_zero_loss() {
        let d: Vec<f64> = (0..=50).map(|i| i as f64 * 200.0).collect();
        let h = vec![0.0; d.len()];
        let p = PathParams::new(100.0, 100.0, 5800.0, 4.0 / 3.0);
        assert_eq!(deygout_loss(&d, &h, &p, DeygoutVariant::Classic).unwrap().loss_db, 0.0);
        assert_eq!(deygout_loss(&d, &h, &p, DeygoutVariant::P452Empirical).unwrap().loss_db, 0.0);
        let b = bullington_loss(&d, &h, &p, BullingtonOutput::WithCorrection).unwrap();
        assert!(b.loss_db < 1e-9 && b.line_of_sight);
        let db = delta_bullington_loss(&d, &h, &p, &GroundConstants::default()).unwrap();
        assert!(db.loss_db < 1e-9, "{}", db.loss_db);
        let c = los_clearance(&d, &h, &p).unwrap();
        assert!(c.line_of_sight && c.fresnel_60_clear);
    }

    #[test]
    fn deygout_three_edges() {
        // Two equal edges at 1/3 and 2/3: Deygout picks one as main and the other as secondary.
        let d: Vec<f64> = (0..=90).map(|i| i as f64 * 100.0).collect();
        let mut h = vec![0.0; d.len()];
        h[30] = 30.0;
        h[60] = 30.0;
        let p = PathParams::new(20.0, 20.0, 900.0, 1e9);
        let r = deygout_loss(&d, &h, &p, DeygoutVariant::Classic).unwrap();
        assert_eq!(r.edges.len(), 2);
        // main edge: h=10 m above ray at d1=3000,d2=6000
        let lm = single_knife_edge_loss_db(10.0, 3000.0, 6000.0, 900.0);
        assert!(close(r.edges[0].loss_db, lm, 1e-9));
        // secondary: sub-path from main edge (30 m terrain) to rx (20 m): at 6000 the line is
        // 30 - 10*(3000/6000)... computed generically:
        let line = 30.0 + (20.0 - 30.0) * 3000.0 / 6000.0;
        let ls = single_knife_edge_loss_db(30.0 - line, 3000.0, 3000.0, 900.0);
        assert!(close(r.loss_db, lm + ls, 1e-9), "{} vs {}", r.loss_db, lm + ls);
        let p452 = deygout_loss(&d, &h, &p, DeygoutVariant::P452Empirical).unwrap();
        assert!(p452.loss_db > lm);
    }

    #[test]
    fn earth_bulge_applied_internally() {
        // Flat sea-level 60 km path, 30 m antennas, k=4/3: bulge at midpoint = 52.98 m → obstructed
        let d: Vec<f64> = (0..=120).map(|i| i as f64 * 500.0).collect();
        let h = vec![0.0; d.len()];
        let p = PathParams::new(30.0, 30.0, 6000.0, 4.0 / 3.0);
        let c = los_clearance(&d, &h, &p).unwrap();
        assert!(!c.line_of_sight);
        assert_eq!(c.min_index, Some(60));
        assert!(close(c.points[60].bulge_m, 52.98, 0.01), "{}", c.points[60].bulge_m);
        let r = bullington_loss(&d, &h, &p, BullingtonOutput::WithCorrection).unwrap();
        assert!(!r.line_of_sight);
        assert!(r.loss_db > 20.0);
        // delta-Bullington ≥ Bullington for smooth earth
        let db = delta_bullington_loss(&d, &h, &p, &GroundConstants::default()).unwrap();
        assert!(db.loss_db >= r.loss_db - 1e-9);
    }

    #[test]
    fn spherical_earth_beyond_horizon_monotonic() {
        let g = GroundConstants::default();
        let l50 = spherical_earth_loss_db(50.0, 10.0, 10.0, 1000.0, 4.0 / 3.0, &g);
        let l80 = spherical_earth_loss_db(80.0, 10.0, 10.0, 1000.0, 4.0 / 3.0, &g);
        assert!(l80 > l50 && l50 > 0.0);
        // Short path well within LoS: zero loss
        assert_eq!(spherical_earth_loss_db(5.0, 50.0, 50.0, 1000.0, 4.0 / 3.0, &g), 0.0);
    }

    #[test]
    fn clearance_ratio_values() {
        let d = vec![0.0, 5000.0, 10_000.0];
        let f = 5800.0;
        let f1 = fresnel_radius_m(1, f, 5000.0, 5000.0);
        let p = PathParams::new(100.0, 100.0, f, 1e12);
        let c = los_clearance(&d, &[0.0, 100.0 - 0.5 * f1, 0.0], &p).unwrap();
        assert!(close(c.min_clearance_ratio.unwrap(), 0.5, 1e-6));
        assert!(!c.fresnel_60_clear && c.line_of_sight);
        assert_eq!(c.points[0].clearance_ratio, None);
        let json = serde_json::to_string(&c).unwrap();
        let back: LosClearance = serde_json::from_str(&json).unwrap();
        assert_eq!(back.min_index, Some(1));
    }

    #[test]
    fn profile_validation() {
        let p = PathParams::new(10.0, 10.0, 100.0, 4.0 / 3.0);
        assert!(deygout_loss(&[0.0, 1.0], &[0.0], &p, DeygoutVariant::Classic).is_err());
        assert!(deygout_loss(&[0.0], &[0.0], &p, DeygoutVariant::Classic).is_err());
        assert!(deygout_loss(&[0.0, 2.0, 1.0], &[0.0; 3], &p, DeygoutVariant::Classic).is_err());
        let bad = PathParams::new(10.0, 10.0, -1.0, 4.0 / 3.0);
        assert!(los_clearance(&[0.0, 1.0], &[0.0, 0.0], &bad).is_err());
    }
}
