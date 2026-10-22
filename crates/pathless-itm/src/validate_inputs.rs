//! Port of `src/ValidateInputs.cpp`.

use crate::errors::ItmError;
use crate::warnings::*;

/// `true` if `mdvar` is one of 0..=3, 10..=13, 20..=23, 30..=33.
#[inline]
pub(crate) fn mdvar_is_valid(mdvar: i32) -> bool {
    !((mdvar < 0)
        || (mdvar > 3 && mdvar < 10)
        || (mdvar > 13 && mdvar < 20)
        || (mdvar > 23 && mdvar < 30)
        || (mdvar > 33))
}

/// Input parameter validation common to point-to-point and area mode.
///
/// Takes the raw NTIA integer `climate` / `pol` so the checks (and their
/// order, which determines which error is reported first) match the C++.
#[allow(clippy::too_many_arguments)]
pub(crate) fn validate_inputs(
    h_tx_m: f64,
    h_rx_m: f64,
    climate: i32,
    time: f64,
    location: f64,
    situation: f64,
    n_0: f64,
    f_mhz: f64,
    pol: i32,
    epsilon: f64,
    sigma: f64,
    mdvar: i32,
    warnings: &mut u32,
) -> Result<(), ItmError> {
    if h_tx_m < 1.0 || h_tx_m > 1000.0 {
        *warnings |= TX_TERMINAL_HEIGHT;
    }

    if h_tx_m < 0.5 || h_tx_m > 3000.0 {
        return Err(ItmError::TxTerminalHeight);
    }

    if h_rx_m < 1.0 || h_rx_m > 1000.0 {
        *warnings |= RX_TERMINAL_HEIGHT;
    }

    if h_rx_m < 0.5 || h_rx_m > 3000.0 {
        return Err(ItmError::RxTerminalHeight);
    }

    if !(1..=7).contains(&climate) {
        return Err(ItmError::InvalidRadioClimate);
    }

    if n_0 < 250.0 || n_0 > 400.0 {
        return Err(ItmError::Refractivity);
    }

    if f_mhz < 40.0 || f_mhz > 10000.0 {
        *warnings |= FREQUENCY;
    }

    if f_mhz < 20.0 || f_mhz > 20000.0 {
        return Err(ItmError::Frequency);
    }

    if pol != 0 && pol != 1 {
        return Err(ItmError::Polarization);
    }

    if epsilon < 1.0 {
        return Err(ItmError::Epsilon);
    }

    if sigma <= 0.0 {
        return Err(ItmError::Sigma);
    }

    if !mdvar_is_valid(mdvar) {
        return Err(ItmError::Mdvar);
    }

    if situation <= 0.0 || situation >= 100.0 {
        return Err(ItmError::InvalidSituation);
    }

    if time <= 0.0 || time >= 100.0 {
        return Err(ItmError::InvalidTime);
    }

    if location <= 0.0 || location >= 100.0 {
        return Err(ItmError::InvalidLocation);
    }

    Ok(())
}
