//! Port of `src/itm_area.cpp`: area prediction mode.

use serde::{Deserialize, Serialize};

use crate::common::ItmMode;
use crate::enums::{Climate, Polarization, SitingCriteria};
use crate::errors::ItmError;
use crate::free_space_loss::free_space_loss_db;
use crate::initialize_area::initialize_area;
use crate::initialize_point_to_point::initialize_point_to_point;
use crate::longley_rice::longley_rice;
use crate::types::{IntermediateValues, ItmOutput};
use crate::validate_inputs::validate_inputs;
use crate::variability::variability;

/// Inputs of an area-mode prediction (no terrain profile; terrain is
/// summarised by `delta_h_m` and the siting criteria).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct AreaInput {
    /// Structural height of the TX antenna above ground, meters (0.5..=3000).
    pub h_tx_m: f64,
    /// Structural height of the RX antenna above ground, meters (0.5..=3000).
    pub h_rx_m: f64,
    /// TX siting criteria.
    pub tx_siting: SitingCriteria,
    /// RX siting criteria.
    pub rx_siting: SitingCriteria,
    /// Path distance, km (> 0; warnings below 1 km and above 1000 km).
    pub d_km: f64,
    /// Terrain irregularity parameter delta_h, meters (>= 0). Typical:
    /// 0 water/very smooth, 30 plains, 90 hills, 200 mountains, 500 rugged
    /// mountains.
    pub delta_h_m: f64,
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
    /// Raw NTIA mode of variability (see [`crate::P2pParams::mdvar`]).
    pub mdvar: i32,
}

/// Area-mode prediction, variability as time / location / situation
/// (NTIA `ITM_AREA_TLS_Ex`). Percentages must satisfy 0 < x < 100.
pub fn area_tls(
    input: &AreaInput,
    time_pct: f64,
    location_pct: f64,
    situation_pct: f64,
) -> Result<ItmOutput, ItmError> {
    let p = input;
    let mut warnings = 0u32;

    // inital input validation check - some validation occurs later in calculations
    validate_inputs(
        p.h_tx_m,
        p.h_rx_m,
        p.climate.as_i32(),
        time_pct,
        location_pct,
        situation_pct,
        p.n_0,
        p.f_mhz,
        p.polarization.as_i32(),
        p.epsilon,
        p.sigma,
        p.mdvar,
        &mut warnings,
    )?;

    // additional area mode parameter validation checks
    if p.d_km <= 0.0 {
        return Err(ItmError::PathDistance);
    }
    if p.delta_h_m < 0.0 {
        return Err(ItmError::DeltaH);
    }
    // (siting criteria are enum-typed, so ERROR__TX/RX_SITING_CRITERIA cannot occur here)

    let site_criteria = [p.tx_siting, p.rx_siting];
    let h_m = [p.h_tx_m, p.h_rx_m];

    let init = initialize_point_to_point(p.f_mhz, 0.0, p.n_0, p.polarization, p.epsilon, p.sigma);

    let (h_e_m, d_hzn_m, theta_hzn) = initialize_area(site_criteria, init.gamma_e, p.delta_h_m, &h_m);

    let d_m = p.d_km * 1000.0;
    let (a_ref_db, propmode) = longley_rice(
        &theta_hzn,
        p.f_mhz,
        init.z_g,
        &d_hzn_m,
        &h_e_m,
        init.gamma_e,
        init.n_s,
        p.delta_h_m,
        &h_m,
        d_m,
        ItmMode::Area,
        &mut warnings,
    )?;

    let a_fs_db = free_space_loss_db(d_m, p.f_mhz);

    let loss_db = a_fs_db
        + variability(
            time_pct,
            location_pct,
            situation_pct,
            &h_e_m,
            p.delta_h_m,
            p.f_mhz,
            d_m,
            a_ref_db,
            p.climate.as_i32(),
            p.mdvar,
            &mut warnings,
        );

    Ok(ItmOutput {
        loss_db,
        warnings,
        intermediate: IntermediateValues {
            theta_hzn,
            d_hzn_m,
            h_e_m,
            n_s: init.n_s,
            delta_h_m: p.delta_h_m,
            a_ref_db,
            a_fs_db,
            d_km: p.d_km,
            mode: propmode,
        },
    })
}

/// Area-mode prediction, variability as confidence / reliability
/// (NTIA `ITM_AREA_CR_Ex`). Equivalent to TLS with time = reliability,
/// location = 50, situation = confidence.
pub fn area_cr(input: &AreaInput, confidence_pct: f64, reliability_pct: f64) -> Result<ItmOutput, ItmError> {
    area_tls(input, reliability_pct, 50.0, confidence_pct).map_err(ItmError::tls_to_cr)
}
