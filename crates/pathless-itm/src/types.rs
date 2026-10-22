//! Output types (port of `struct IntermediateValues` in `include/itm.h`).

use serde::{Deserialize, Serialize};

use crate::enums::PropMode;

/// Internal values computed by ITM (the C++ `_Ex` functions' output).
/// Index 0 is the TX terminal, index 1 the RX terminal.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct IntermediateValues {
    /// Terminal horizon angles, radians (positive = horizon above horizontal).
    pub theta_hzn: [f64; 2],
    /// Terminal horizon distances, meters.
    pub d_hzn_m: [f64; 2],
    /// Effective terminal heights, meters.
    pub h_e_m: [f64; 2],
    /// Surface refractivity N_s, N-units (N_0 scaled to the mean path elevation).
    pub n_s: f64,
    /// Terrain irregularity parameter delta_h, meters.
    pub delta_h_m: f64,
    /// Reference attenuation (median loss relative to free space, before
    /// variability), dB.
    pub a_ref_db: f64,
    /// Free space basic transmission loss, dB.
    pub a_fs_db: f64,
    /// Path distance, km.
    pub d_km: f64,
    /// Selected mode of propagation.
    pub mode: PropMode,
}

/// Result of an ITM prediction.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct ItmOutput {
    /// Basic transmission loss, dB (free space loss + ITM excess loss +
    /// variability). This is the NTIA `A__db` output.
    pub loss_db: f64,
    /// NTIA warning bit flags (see [`crate::warnings`]). Zero means the
    /// NTIA return code would have been `SUCCESS`; non-zero means
    /// `SUCCESS_WITH_WARNINGS`.
    pub warnings: u32,
    /// Intermediate values.
    pub intermediate: IntermediateValues,
}

impl ItmOutput {
    /// `true` if any warning flag is set.
    #[inline]
    pub fn has_warnings(&self) -> bool {
        self.warnings != 0
    }

    /// The NTIA integer return code: 0 (`SUCCESS`) or 1 (`SUCCESS_WITH_WARNINGS`).
    #[inline]
    pub fn return_code(&self) -> i32 {
        if self.warnings != 0 {
            crate::errors::SUCCESS_WITH_WARNINGS
        } else {
            crate::errors::SUCCESS
        }
    }
}
