//! Port of `src/itm_p2p.cpp`: point-to-point prediction mode.

use serde::{Deserialize, Serialize};

use crate::common::{ItmMode, Pfl};
use crate::enums::{Climate, Polarization};
use crate::errors::ItmError;
use crate::free_space_loss::free_space_loss_db;
use crate::initialize_point_to_point::initialize_point_to_point;
use crate::longley_rice::longley_rice;
use crate::quick_pfl::quick_pfl;
use crate::types::{IntermediateValues, ItmOutput};
use crate::validate_inputs::validate_inputs;
use crate::variability::variability;

/// Scalar (non-terrain) inputs of a point-to-point prediction. `Copy`, so a
/// coverage engine can build one per site and reuse it for every radial.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct P2pParams {
    /// Structural height of the TX antenna above ground, meters.
    /// Valid 0.5..=3000 (warning outside 1..=1000).
    pub h_tx_m: f64,
    /// Structural height of the RX antenna above ground, meters.
    /// Valid 0.5..=3000 (warning outside 1..=1000).
    pub h_rx_m: f64,
    /// Radio climate.
    pub climate: Climate,
    /// Surface refractivity reduced to sea level (minimum monthly mean),
    /// N-units. Valid 250..=400; 301 is the usual default.
    pub n_0: f64,
    /// Frequency, MHz. Valid 20..=20000 (warning outside 40..=10000).
    pub f_mhz: f64,
    /// Polarization.
    pub polarization: Polarization,
    /// Ground relative permittivity, >= 1 (see [`crate::ground`]).
    pub epsilon: f64,
    /// Ground conductivity, S/m, > 0 (see [`crate::ground`]).
    pub sigma: f64,
    /// Raw NTIA mode of variability: 0..=3 base mode
    /// ([`crate::ModeOfVariability`]) plus 10 to eliminate location
    /// variability and/or 20 to eliminate direct situation variability.
    pub mdvar: i32,
}

/// Owned point-to-point input (terrain profile + scalar parameters).
///
/// For hot loops prefer [`p2p_tls_profile`] with a borrowed slice and a
/// [`P2pParams`]; this struct is convenient for serialization and one-off
/// calls.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct P2pInput {
    /// Structural height of the TX antenna above ground, meters (0.5..=3000).
    pub h_tx_m: f64,
    /// Structural height of the RX antenna above ground, meters (0.5..=3000).
    pub h_rx_m: f64,
    /// Terrain elevations above mean sea level, meters, evenly spaced from
    /// the TX (first element) to the RX (last element), both endpoints
    /// included. At least 2 points are required.
    pub elevations_m: Vec<f64>,
    /// Spacing between consecutive profile points, meters (> 0).
    /// Path length is `(elevations_m.len() - 1) * step_m`.
    pub step_m: f64,
    /// Radio climate.
    pub climate: Climate,
    /// Surface refractivity reduced to sea level, N-units (250..=400).
    pub n_0: f64,
    /// Frequency, MHz (20..=20000).
    pub f_mhz: f64,
    /// Polarization.
    pub polarization: Polarization,
    /// Ground relative permittivity (>= 1).
    pub epsilon: f64,
    /// Ground conductivity, S/m (> 0).
    pub sigma: f64,
    /// Raw NTIA mode of variability (see [`P2pParams::mdvar`]).
    pub mdvar: i32,
}

impl P2pInput {
    /// The scalar parameters of this input.
    pub fn params(&self) -> P2pParams {
        P2pParams {
            h_tx_m: self.h_tx_m,
            h_rx_m: self.h_rx_m,
            climate: self.climate,
            n_0: self.n_0,
            f_mhz: self.f_mhz,
            polarization: self.polarization,
            epsilon: self.epsilon,
            sigma: self.sigma,
            mdvar: self.mdvar,
        }
    }

    /// Build from parameters and an owned profile.
    pub fn new(params: &P2pParams, elevations_m: Vec<f64>, step_m: f64) -> Self {
        Self {
            h_tx_m: params.h_tx_m,
            h_rx_m: params.h_rx_m,
            elevations_m,
            step_m,
            climate: params.climate,
            n_0: params.n_0,
            f_mhz: params.f_mhz,
            polarization: params.polarization,
            epsilon: params.epsilon,
            sigma: params.sigma,
            mdvar: params.mdvar,
        }
    }
}

/// Point-to-point prediction, variability as time / location / situation
/// (NTIA `ITM_P2P_TLS_Ex`).
///
/// Percentages must satisfy 0 < x < 100. Returns the basic transmission
/// loss not exceeded for `time_pct` % of the time, at `location_pct` % of
/// locations, in `situation_pct` % of situations.
pub fn p2p_tls(input: &P2pInput, time_pct: f64, location_pct: f64, situation_pct: f64) -> Result<ItmOutput, ItmError> {
    p2p_tls_profile(&input.elevations_m, input.step_m, &input.params(), time_pct, location_pct, situation_pct)
}

/// Point-to-point prediction, variability as confidence / reliability
/// (NTIA `ITM_P2P_CR_Ex`). Equivalent to TLS with time = reliability,
/// location = 50, situation = confidence; time/situation range errors are
/// reported as reliability/confidence errors.
pub fn p2p_cr(input: &P2pInput, confidence_pct: f64, reliability_pct: f64) -> Result<ItmOutput, ItmError> {
    p2p_cr_profile(&input.elevations_m, input.step_m, &input.params(), confidence_pct, reliability_pct)
}

/// Borrowing, allocation-free variant of [`p2p_tls`] for hot loops.
///
/// `elevations_m`: evenly spaced terrain elevations (meters AMSL) from TX to
/// RX inclusive, at least 2 points; `step_m`: spacing in meters (> 0).
pub fn p2p_tls_profile(
    elevations_m: &[f64],
    step_m: f64,
    params: &P2pParams,
    time_pct: f64,
    location_pct: f64,
    situation_pct: f64,
) -> Result<ItmOutput, ItmError> {
    p2p_tls_ex(pfl_from_profile(elevations_m, step_m), params, time_pct, location_pct, situation_pct)
}

/// Borrowing, allocation-free variant of [`p2p_cr`] for hot loops.
pub fn p2p_cr_profile(
    elevations_m: &[f64],
    step_m: f64,
    params: &P2pParams,
    confidence_pct: f64,
    reliability_pct: f64,
) -> Result<ItmOutput, ItmError> {
    p2p_tls_profile(elevations_m, step_m, params, reliability_pct, 50.0, confidence_pct).map_err(ItmError::tls_to_cr)
}

/// Point-to-point TLS prediction taking a terrain profile in the native NTIA
/// PFL format: `pfl = [num_points - 1, step_m, z_0, z_1, ..., z_np]`.
///
/// `pfl[0]` must be a positive integer and the slice must contain at least
/// `pfl[0] + 3` values (extra trailing values are ignored, as in C++).
pub fn p2p_tls_pfl(
    pfl: &[f64],
    params: &P2pParams,
    time_pct: f64,
    location_pct: f64,
    situation_pct: f64,
) -> Result<ItmOutput, ItmError> {
    p2p_tls_ex(pfl_from_ntia(pfl), params, time_pct, location_pct, situation_pct)
}

/// Point-to-point CR prediction taking an NTIA PFL-format profile
/// (see [`p2p_tls_pfl`]).
pub fn p2p_cr_pfl(
    pfl: &[f64],
    params: &P2pParams,
    confidence_pct: f64,
    reliability_pct: f64,
) -> Result<ItmOutput, ItmError> {
    p2p_tls_pfl(pfl, params, reliability_pct, 50.0, confidence_pct).map_err(ItmError::tls_to_cr)
}

/// Build the internal profile view from elevations + spacing, rejecting
/// profiles the C++ would mis-handle (not an NTIA check; see
/// [`ItmError::InvalidProfile`]).
#[inline]
pub(crate) fn pfl_from_profile(z: &[f64], xi: f64) -> Result<Pfl<'_>, ItmError> {
    if z.len() < 2 || z.len() > i32::MAX as usize || !(xi > 0.0 && xi.is_finite()) {
        return Err(ItmError::InvalidProfile);
    }
    Ok(Pfl { np: z.len() - 1, xi, z })
}

/// Build the internal profile view from an NTIA PFL array.
#[inline]
pub(crate) fn pfl_from_ntia(pfl: &[f64]) -> Result<Pfl<'_>, ItmError> {
    if pfl.len() < 4 {
        return Err(ItmError::InvalidProfile);
    }
    let np_f = pfl[0];
    if !(np_f >= 1.0 && np_f.fract() == 0.0 && np_f + 3.0 <= pfl.len() as f64) {
        return Err(ItmError::InvalidProfile);
    }
    let np = np_f as usize;
    pfl_from_profile(&pfl[2..np + 3], pfl[1])
}

/// Port of `ITM_P2P_TLS_Ex`.
fn p2p_tls_ex(
    pfl: Result<Pfl<'_>, ItmError>,
    p: &P2pParams,
    time: f64,
    location: f64,
    situation: f64,
) -> Result<ItmOutput, ItmError> {
    let mut warnings = 0u32; // Initialize to no warnings

    // initial input validation check - some validation occurs later in calculations
    validate_inputs(
        p.h_tx_m,
        p.h_rx_m,
        p.climate.as_i32(),
        time,
        location,
        situation,
        p.n_0,
        p.f_mhz,
        p.polarization.as_i32(),
        p.epsilon,
        p.sigma,
        p.mdvar,
        &mut warnings,
    )?;

    // pathless-itm addition: reject unusable profiles instead of reading out of bounds.
    // (checked after ValidateInputs so NTIA error precedence is preserved)
    let pfl = pfl?;

    let d_km = (pfl.np_f() * pfl.xi) / 1000.0;

    let np = pfl.np_i(); // number of points in the pfl

    // compute the average path height, ignoring first and last 10%
    let p10 = (0.1 * np as f64) as i32; // 10% of np
    let mut h_sys_m = 0.0; // Height of the system above mean sea level

    for &zi in &pfl.z[p10 as usize..=(np - p10) as usize] {
        h_sys_m += zi;
    }

    h_sys_m /= (np - 2 * p10 + 1) as f64;

    let init = initialize_point_to_point(p.f_mhz, h_sys_m, p.n_0, p.polarization, p.epsilon, p.sigma);

    let h_m = [p.h_tx_m, p.h_rx_m];
    let qp = quick_pfl(pfl, init.gamma_e, &h_m);

    // Reference attenuation, in dB
    let (a_ref_db, propmode) = longley_rice(
        &qp.theta_hzn,
        p.f_mhz,
        init.z_g,
        &qp.d_hzn_m,
        &qp.h_e_m,
        init.gamma_e,
        init.n_s,
        qp.delta_h_m,
        &h_m,
        qp.d_m,
        ItmMode::P2p,
        &mut warnings,
    )?;

    let a_fs_db = free_space_loss_db(qp.d_m, p.f_mhz);

    let loss_db = variability(
        time,
        location,
        situation,
        &qp.h_e_m,
        qp.delta_h_m,
        p.f_mhz,
        qp.d_m,
        a_ref_db,
        p.climate.as_i32(),
        p.mdvar,
        &mut warnings,
    ) + a_fs_db;

    Ok(ItmOutput {
        loss_db,
        warnings,
        intermediate: IntermediateValues {
            theta_hzn: qp.theta_hzn,
            d_hzn_m: qp.d_hzn_m,
            h_e_m: qp.h_e_m,
            n_s: init.n_s,
            delta_h_m: qp.delta_h_m,
            a_ref_db,
            a_fs_db,
            d_km,
            mode: propmode,
        },
    })
}
