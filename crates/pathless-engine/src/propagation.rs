//! Shared propagation setup: maps project environment settings onto ITM and
//! the microwave (free space + P.526 diffraction + P.676 gases) method.

use pathless_itm::{Climate, ItmOutput, ModeOfVariability, P2pParams, Polarization};
use pathless_rf::diffraction::{delta_bullington_loss, GroundConstants, PathParams};
use pathless_rf::itu::p676::{self, Atmosphere};

use crate::project::{ClimateChoice, Environment, ModelChoice, Pol, Variability};

pub const ITM_MIN_MHZ: f64 = 20.0;
pub const ITM_MAX_MHZ: f64 = 20_000.0;
/// In `Auto`, point-to-point links at or above this use the microwave method.
pub const AUTO_LINK_MICROWAVE_MHZ: f64 = 3_000.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Method {
    Itm,
    Microwave,
}

impl Method {
    pub fn label(self) -> &'static str {
        match self {
            Method::Itm => "Longley-Rice ITM",
            Method::Microwave => "Free space + P.526 diffraction",
        }
    }
}

/// Pick the method for a link (`for_link`) or a coverage run.
pub fn choose_method(model: ModelChoice, f_mhz: f64, for_link: bool) -> Method {
    let itm_ok = (ITM_MIN_MHZ..=ITM_MAX_MHZ).contains(&f_mhz);
    match model {
        ModelChoice::Itm if itm_ok => Method::Itm,
        ModelChoice::Itm | ModelChoice::Microwave => Method::Microwave,
        ModelChoice::Auto => {
            if !itm_ok || (for_link && f_mhz >= AUTO_LINK_MICROWAVE_MHZ) {
                Method::Microwave
            } else {
                Method::Itm
            }
        }
    }
}

fn itm_climate(c: ClimateChoice) -> Climate {
    match c {
        ClimateChoice::Equatorial => Climate::Equatorial,
        ClimateChoice::ContinentalSubtropical => Climate::ContinentalSubtropical,
        ClimateChoice::MaritimeSubtropical => Climate::MaritimeSubtropical,
        ClimateChoice::Desert => Climate::Desert,
        ClimateChoice::ContinentalTemperate => Climate::ContinentalTemperate,
        ClimateChoice::MaritimeTemperateOverLand => Climate::MaritimeTemperateOverLand,
        ClimateChoice::MaritimeTemperateOverSea => Climate::MaritimeTemperateOverSea,
    }
}

fn itm_variability(v: Variability) -> ModeOfVariability {
    match v {
        Variability::SingleMessage => ModeOfVariability::SingleMessage,
        Variability::Accidental => ModeOfVariability::Accidental,
        Variability::Mobile => ModeOfVariability::Mobile,
        Variability::Broadcast => ModeOfVariability::Broadcast,
    }
}

/// ITM requires terminal heights of at least 0.5 m.
fn itm_height(h: f64) -> f64 {
    h.clamp(0.5, 3000.0)
}

/// Everything needed to evaluate path loss for one frequency / pair of
/// antenna heights; cheap to share across threads.
#[derive(Debug, Clone)]
pub struct PathModel {
    pub method: Method,
    pub f_mhz: f64,
    pub k_factor: f64,
    pub itm: P2pParams,
    pub time_pct: f64,
    pub location_pct: f64,
    pub situation_pct: f64,
    pub ground: GroundConstants,
    /// Specific gaseous attenuation (dB/km); 0 below 1 GHz.
    pub gas_db_per_km: f64,
}

#[derive(Debug, Clone)]
pub struct PathLoss {
    /// Loss used for the budget (method result + gases), dB.
    pub total_db: f64,
    pub fspl_db: f64,
    pub gas_db: f64,
    pub itm: Option<ItmOutput>,
    pub diffraction_db: Option<f64>,
}

impl PathModel {
    pub fn new(env: &Environment, method: Method, f_mhz: f64, pol: Pol, h_tx_agl: f64, h_rx_agl: f64) -> Self {
        let vertical = pol == Pol::Vertical;
        let mdvar = itm_variability(env.variability).mdvar(false, false);
        let gas_db_per_km =
            if f_mhz >= 1_000.0 { p676::gamma_total_db_per_km(f_mhz / 1000.0, &Atmosphere::default()) } else { 0.0 };
        PathModel {
            method,
            f_mhz,
            k_factor: env.k_factor.clamp(0.3, 10.0),
            itm: P2pParams {
                h_tx_m: itm_height(h_tx_agl),
                h_rx_m: itm_height(h_rx_agl),
                climate: itm_climate(env.climate),
                n_0: env.n0,
                f_mhz,
                polarization: if vertical { Polarization::Vertical } else { Polarization::Horizontal },
                epsilon: env.epsilon,
                sigma: env.sigma,
                mdvar,
            },
            time_pct: env.time_pct.clamp(0.01, 99.99),
            location_pct: env.location_pct.clamp(0.01, 99.99),
            situation_pct: env.situation_pct.clamp(0.01, 99.99),
            ground: GroundConstants {
                permittivity: env.epsilon,
                conductivity_s_m: env.sigma,
                vertical_polarization: vertical,
            },
            gas_db_per_km,
        }
    }

    pub fn fspl_db(&self, d_m: f64) -> f64 {
        pathless_rf::basic::fspl_db((d_m / 1000.0).max(1e-3), self.f_mhz)
    }

    pub fn itm_loss(&self, heights: &[f64], step_m: f64) -> Result<ItmOutput, pathless_itm::ItmError> {
        pathless_itm::p2p_tls_profile(heights, step_m, &self.itm, self.time_pct, self.location_pct, self.situation_pct)
    }

    /// Delta-Bullington diffraction loss (dB) over `heights` (AMSL incl. clutter).
    pub fn diffraction_db(&self, distances: &[f64], heights: &[f64], tx_amsl: f64, rx_amsl: f64) -> Option<f64> {
        let p = PathParams::new(tx_amsl, rx_amsl, self.f_mhz, self.k_factor);
        delta_bullington_loss(distances, heights, &p, &self.ground).ok().map(|r| r.loss_db)
    }

    /// Path loss with the model's method. `distances` must match `heights`
    /// (pass an empty slice for ITM to skip building it).
    pub fn loss(
        &self,
        heights: &[f64],
        distances: &[f64],
        step_m: f64,
        tx_amsl: f64,
        rx_amsl: f64,
    ) -> Option<PathLoss> {
        let d_m = step_m * (heights.len().saturating_sub(1)) as f64;
        let fspl_db = self.fspl_db(d_m);
        let gas_db = self.gas_db_per_km * d_m / 1000.0;
        match self.method {
            Method::Itm => {
                let out = self.itm_loss(heights, step_m).ok()?;
                Some(PathLoss { total_db: out.loss_db + gas_db, fspl_db, gas_db, itm: Some(out), diffraction_db: None })
            }
            Method::Microwave => {
                let diff = self.diffraction_db(distances, heights, tx_amsl, rx_amsl).unwrap_or(0.0);
                Some(PathLoss {
                    total_db: fspl_db + diff + gas_db,
                    fspl_db,
                    gas_db,
                    itm: None,
                    diffraction_db: Some(diff),
                })
            }
        }
    }
}

/// Elevation angle (deg) from a point at `h1_amsl` to one at `h2_amsl`
/// `d_m` away, including effective-earth curvature.
pub fn elevation_angle_deg(h1_amsl: f64, h2_amsl: f64, d_m: f64, k_factor: f64) -> f64 {
    if d_m <= 0.0 {
        return 0.0;
    }
    let re = pathless_rf::basic::effective_earth_radius_m(k_factor);
    ((h2_amsl - h1_amsl) / d_m - d_m / (2.0 * re)).atan().to_degrees()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn auto_method_selection() {
        assert_eq!(choose_method(ModelChoice::Auto, 450.0, true), Method::Itm);
        assert_eq!(choose_method(ModelChoice::Auto, 5800.0, true), Method::Microwave);
        assert_eq!(choose_method(ModelChoice::Auto, 3550.0, false), Method::Itm);
        assert_eq!(choose_method(ModelChoice::Auto, 23000.0, false), Method::Microwave);
        assert_eq!(choose_method(ModelChoice::Itm, 38000.0, true), Method::Microwave);
    }

    #[test]
    fn elevation_angle_includes_curvature() {
        let flat = elevation_angle_deg(100.0, 100.0, 10_000.0, 4.0 / 3.0);
        assert!(flat < 0.0 && flat > -0.1, "{flat}");
        assert!((elevation_angle_deg(0.0, 1000.0, 1000.0, 1e9) - 45.0).abs() < 1e-3);
    }
}
