//! Typical ground electrical constants (relative permittivity `epsilon`,
//! conductivity `sigma` in S/m), as commonly used with ITM / SPLAT!.

use serde::{Deserialize, Serialize};

/// Ground electrical constants.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct GroundParams {
    /// Relative permittivity (dimensionless, must be >= 1).
    pub epsilon: f64,
    /// Conductivity in S/m (must be > 0).
    pub sigma: f64,
}

/// Average ground: epsilon = 15, sigma = 0.005 S/m. Sensible default.
pub const AVERAGE: GroundParams = GroundParams { epsilon: 15.0, sigma: 0.005 };
/// Poor ground: epsilon = 4, sigma = 0.001 S/m.
pub const POOR: GroundParams = GroundParams { epsilon: 4.0, sigma: 0.001 };
/// Good ground: epsilon = 25, sigma = 0.02 S/m.
pub const GOOD: GroundParams = GroundParams { epsilon: 25.0, sigma: 0.02 };
/// Fresh water: epsilon = 81, sigma = 0.01 S/m.
pub const FRESH_WATER: GroundParams = GroundParams { epsilon: 81.0, sigma: 0.01 };
/// Sea water: epsilon = 81, sigma = 5.0 S/m.
pub const SEA_WATER: GroundParams = GroundParams { epsilon: 81.0, sigma: 5.0 };

impl Default for GroundParams {
    /// [`AVERAGE`] ground.
    fn default() -> Self {
        AVERAGE
    }
}
