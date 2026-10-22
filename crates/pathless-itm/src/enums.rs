//! Port of `include/Enums.h`.

use serde::{Deserialize, Serialize};

use crate::errors::ItmError;

/// Radio climate of the path (NTIA `climate`, values 1..=7).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[repr(i32)]
pub enum Climate {
    /// 1 - Equatorial (Congo).
    Equatorial = 1,
    /// 2 - Continental subtropical (Sudan).
    ContinentalSubtropical = 2,
    /// 3 - Maritime subtropical (West coast of Africa).
    MaritimeSubtropical = 3,
    /// 4 - Desert (Sahara).
    Desert = 4,
    /// 5 - Continental temperate (the usual choice for mid-latitude land paths).
    ContinentalTemperate = 5,
    /// 6 - Maritime temperate, over land (United Kingdom, continental west coasts).
    MaritimeTemperateOverLand = 6,
    /// 7 - Maritime temperate, over sea.
    MaritimeTemperateOverSea = 7,
}

impl Climate {
    /// Raw NTIA integer value.
    #[inline]
    pub fn as_i32(self) -> i32 {
        self as i32
    }
}

impl TryFrom<i32> for Climate {
    type Error = ItmError;
    fn try_from(v: i32) -> Result<Self, ItmError> {
        Ok(match v {
            1 => Climate::Equatorial,
            2 => Climate::ContinentalSubtropical,
            3 => Climate::MaritimeSubtropical,
            4 => Climate::Desert,
            5 => Climate::ContinentalTemperate,
            6 => Climate::MaritimeTemperateOverLand,
            7 => Climate::MaritimeTemperateOverSea,
            _ => return Err(ItmError::InvalidRadioClimate),
        })
    }
}

/// Antenna polarization (NTIA `pol`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[repr(i32)]
pub enum Polarization {
    /// 0 - Horizontal.
    Horizontal = 0,
    /// 1 - Vertical.
    Vertical = 1,
}

impl Polarization {
    /// Raw NTIA integer value.
    #[inline]
    pub fn as_i32(self) -> i32 {
        self as i32
    }
}

impl TryFrom<i32> for Polarization {
    type Error = ItmError;
    fn try_from(v: i32) -> Result<Self, ItmError> {
        match v {
            0 => Ok(Polarization::Horizontal),
            1 => Ok(Polarization::Vertical),
            _ => Err(ItmError::Polarization),
        }
    }
}

/// Terminal siting criteria, area mode only (NTIA `tx/rx_site_criteria`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[repr(i32)]
pub enum SitingCriteria {
    /// 0 - Random siting.
    Random = 0,
    /// 1 - Careful siting (antenna placed to take advantage of local terrain).
    Careful = 1,
    /// 2 - Very careful siting.
    VeryCareful = 2,
}

impl SitingCriteria {
    /// Raw NTIA integer value.
    #[inline]
    pub fn as_i32(self) -> i32 {
        self as i32
    }
}

impl TryFrom<i32> for SitingCriteria {
    type Error = ItmError;
    /// Note: an invalid value maps to [`ItmError::TxSitingCriteria`]; the
    /// caller decides whether TX or RX was at fault.
    fn try_from(v: i32) -> Result<Self, ItmError> {
        match v {
            0 => Ok(SitingCriteria::Random),
            1 => Ok(SitingCriteria::Careful),
            2 => Ok(SitingCriteria::VeryCareful),
            _ => Err(ItmError::TxSitingCriteria),
        }
    }
}

/// Base mode of variability (the units digit of NTIA `mdvar`).
///
/// The public entry points take the raw NTIA `mdvar: i32`, which is
/// `base + 10 * eliminate_location + 20 * eliminate_situation`:
///
/// * `+10`: location variability is eliminated (sigma_L = 0).
/// * `+20`: direct situation variability is eliminated (sigma_S = 0), as is
///   appropriate for interference problems.
///
/// Valid raw values are therefore 0..=3, 10..=13, 20..=23 and 30..=33.
/// Use [`ModeOfVariability::mdvar`] to build one.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[repr(i32)]
pub enum ModeOfVariability {
    /// 0 - Single message mode: time, location and situation all folded
    /// into one confidence.
    SingleMessage = 0,
    /// 1 - Accidental mode: time variability is reliability, location and
    /// situation folded into confidence.
    Accidental = 1,
    /// 2 - Mobile mode: time and location are reliability, situation is
    /// confidence.
    Mobile = 2,
    /// 3 - Broadcast mode: time and location are independent.
    Broadcast = 3,
}

impl ModeOfVariability {
    /// Build a raw NTIA `mdvar` value from the base mode plus the two
    /// elimination flags.
    #[inline]
    pub fn mdvar(self, eliminate_location: bool, eliminate_situation: bool) -> i32 {
        self as i32 + if eliminate_location { 10 } else { 0 } + if eliminate_situation { 20 } else { 0 }
    }

    /// Decompose a raw `mdvar` into `(mode, eliminate_location, eliminate_situation)`.
    /// Returns [`ItmError::Mdvar`] for invalid values.
    pub fn decompose(mdvar: i32) -> Result<(Self, bool, bool), ItmError> {
        if !crate::validate_inputs::mdvar_is_valid(mdvar) {
            return Err(ItmError::Mdvar);
        }
        let mut m = mdvar;
        let plus20 = m >= 20;
        if plus20 {
            m -= 20;
        }
        let plus10 = m >= 10;
        if plus10 {
            m -= 10;
        }
        let mode = match m {
            0 => ModeOfVariability::SingleMessage,
            1 => ModeOfVariability::Accidental,
            2 => ModeOfVariability::Mobile,
            _ => ModeOfVariability::Broadcast,
        };
        Ok((mode, plus10, plus20))
    }
}

/// Mode of propagation selected by the model (NTIA `propmode`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[repr(i32)]
pub enum PropMode {
    /// 0 - Not set (never returned on success).
    NotSet = 0,
    /// 1 - Line of sight.
    LineOfSight = 1,
    /// 2 - Diffraction.
    Diffraction = 2,
    /// 3 - Troposcatter.
    Troposcatter = 3,
}

impl PropMode {
    /// Raw NTIA integer value.
    #[inline]
    pub fn as_i32(self) -> i32 {
        self as i32
    }
}
