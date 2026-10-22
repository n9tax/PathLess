//! Port of `include/Errors.h`.

use serde::{Deserialize, Serialize};
use thiserror::Error;

/// NTIA return code for success without warnings (`SUCCESS`).
pub const SUCCESS: i32 = 0;
/// NTIA return code for success with warning flags set (`SUCCESS_WITH_WARNINGS`).
pub const SUCCESS_WITH_WARNINGS: i32 = 1;

/// ITM error. Each variant corresponds to one NTIA error code
/// (`ERROR__*` in `Errors.h`), available via [`ItmError::code`].
///
/// [`ItmError::InvalidProfile`] is the only variant that is *not* an NTIA
/// code: the C++ library performs no checks on the terrain array, while this
/// port refuses profiles that would make it index out of bounds or divide
/// by zero.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Error, Serialize, Deserialize)]
pub enum ItmError {
    /// 1000 - TX terminal height is out of range (0.5..=3000 m).
    #[error("TX terminal height is out of range (code 1000)")]
    TxTerminalHeight,
    /// 1001 - RX terminal height is out of range (0.5..=3000 m).
    #[error("RX terminal height is out of range (code 1001)")]
    RxTerminalHeight,
    /// 1002 - Invalid value for radio climate.
    #[error("invalid value for radio climate (code 1002)")]
    InvalidRadioClimate,
    /// 1003 - Time percentage is out of range (0 < time < 100).
    #[error("time percentage is out of range (code 1003)")]
    InvalidTime,
    /// 1004 - Location percentage is out of range (0 < location < 100).
    #[error("location percentage is out of range (code 1004)")]
    InvalidLocation,
    /// 1005 - Situation percentage is out of range (0 < situation < 100).
    #[error("situation percentage is out of range (code 1005)")]
    InvalidSituation,
    /// 1006 - Confidence percentage is out of range (0 < confidence < 100).
    #[error("confidence percentage is out of range (code 1006)")]
    InvalidConfidence,
    /// 1007 - Reliability percentage is out of range (0 < reliability < 100).
    #[error("reliability percentage is out of range (code 1007)")]
    InvalidReliability,
    /// 1008 - Refractivity N_0 is out of range (250..=400 N-units).
    #[error("refractivity is out of range (code 1008)")]
    Refractivity,
    /// 1009 - Frequency is out of range (20..=20000 MHz).
    #[error("frequency is out of range (code 1009)")]
    Frequency,
    /// 1010 - Invalid value for polarization.
    #[error("invalid value for polarization (code 1010)")]
    Polarization,
    /// 1011 - Epsilon is out of range (must be >= 1).
    #[error("epsilon is out of range (code 1011)")]
    Epsilon,
    /// 1012 - Sigma is out of range (must be > 0).
    #[error("sigma is out of range (code 1012)")]
    Sigma,
    /// 1013 - The imaginary portion of the complex ground impedance is
    /// larger than the real portion.
    #[error("imaginary part of the ground impedance exceeds the real part (code 1013)")]
    GroundImpedance,
    /// 1014 - Invalid value for mode of variability.
    #[error("invalid value for mode of variability (code 1014)")]
    Mdvar,
    /// 1016 - Internally computed effective earth radius is invalid.
    #[error("internally computed effective earth radius is invalid (code 1016)")]
    EffectiveEarth,
    /// 1017 - Path distance is out of range (area mode: d_km > 0).
    #[error("path distance is out of range (code 1017)")]
    PathDistance,
    /// 1018 - Delta H (terrain irregularity parameter) is out of range (>= 0).
    #[error("delta H (terrain irregularity parameter) is out of range (code 1018)")]
    DeltaH,
    /// 1019 - Invalid value for TX siting criteria.
    #[error("invalid value for TX siting criteria (code 1019)")]
    TxSitingCriteria,
    /// 1020 - Invalid value for RX siting criteria.
    #[error("invalid value for RX siting criteria (code 1020)")]
    RxSitingCriteria,
    /// 1021 - Internally computed surface refractivity value is too small (< 150).
    #[error("internally computed surface refractivity is too small (code 1021)")]
    SurfaceRefractivitySmall,
    /// 1022 - Internally computed surface refractivity value is too large (> 400).
    #[error("internally computed surface refractivity is too large (code 1022)")]
    SurfaceRefractivityLarge,
    /// 1100 - (pathless-itm only, not an NTIA code) the terrain profile is
    /// unusable: fewer than 2 points, non-positive / non-finite spacing, or a
    /// malformed NTIA `pfl` array.
    #[error("invalid terrain profile (code 1100, pathless-itm specific)")]
    InvalidProfile,
}

impl ItmError {
    /// The NTIA numeric error code (1000..=1022), or 1100 for
    /// [`ItmError::InvalidProfile`].
    pub fn code(self) -> i32 {
        match self {
            ItmError::TxTerminalHeight => 1000,
            ItmError::RxTerminalHeight => 1001,
            ItmError::InvalidRadioClimate => 1002,
            ItmError::InvalidTime => 1003,
            ItmError::InvalidLocation => 1004,
            ItmError::InvalidSituation => 1005,
            ItmError::InvalidConfidence => 1006,
            ItmError::InvalidReliability => 1007,
            ItmError::Refractivity => 1008,
            ItmError::Frequency => 1009,
            ItmError::Polarization => 1010,
            ItmError::Epsilon => 1011,
            ItmError::Sigma => 1012,
            ItmError::GroundImpedance => 1013,
            ItmError::Mdvar => 1014,
            ItmError::EffectiveEarth => 1016,
            ItmError::PathDistance => 1017,
            ItmError::DeltaH => 1018,
            ItmError::TxSitingCriteria => 1019,
            ItmError::RxSitingCriteria => 1020,
            ItmError::SurfaceRefractivitySmall => 1021,
            ItmError::SurfaceRefractivityLarge => 1022,
            ItmError::InvalidProfile => 1100,
        }
    }

    /// Inverse of [`ItmError::code`].
    pub fn from_code(code: i32) -> Option<Self> {
        Some(match code {
            1000 => ItmError::TxTerminalHeight,
            1001 => ItmError::RxTerminalHeight,
            1002 => ItmError::InvalidRadioClimate,
            1003 => ItmError::InvalidTime,
            1004 => ItmError::InvalidLocation,
            1005 => ItmError::InvalidSituation,
            1006 => ItmError::InvalidConfidence,
            1007 => ItmError::InvalidReliability,
            1008 => ItmError::Refractivity,
            1009 => ItmError::Frequency,
            1010 => ItmError::Polarization,
            1011 => ItmError::Epsilon,
            1012 => ItmError::Sigma,
            1013 => ItmError::GroundImpedance,
            1014 => ItmError::Mdvar,
            1016 => ItmError::EffectiveEarth,
            1017 => ItmError::PathDistance,
            1018 => ItmError::DeltaH,
            1019 => ItmError::TxSitingCriteria,
            1020 => ItmError::RxSitingCriteria,
            1021 => ItmError::SurfaceRefractivitySmall,
            1022 => ItmError::SurfaceRefractivityLarge,
            1100 => ItmError::InvalidProfile,
            _ => return None,
        })
    }

    /// Map the TLS time/situation errors onto the CR reliability/confidence
    /// errors, exactly as `ITM_P2P_CR` / `ITM_AREA_CR` do.
    pub(crate) fn tls_to_cr(self) -> Self {
        match self {
            ItmError::InvalidTime => ItmError::InvalidReliability,
            ItmError::InvalidSituation => ItmError::InvalidConfidence,
            e => e,
        }
    }
}
