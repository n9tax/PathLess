//! # pathless-rf
//!
//! Pure-Rust RF math for the PathLess path-analysis application:
//!
//! * [`basic`] (also re-exported as [`units`]) – wavelength, free-space loss, dB/power
//!   conversions, Fresnel zones, earth bulge, EIRP, thermal noise.
//! * [`diffraction`] – ITU-R P.526 knife-edge loss, Deygout (3-edge) and Bullington
//!   multiple-edge methods over a terrain profile, the P.526 §4.5 "delta-Bullington"
//!   general-path method, and a line-of-sight / Fresnel clearance analysis.
//! * [`itu`] – microwave propagation recommendations: P.838-3 rain specific attenuation,
//!   P.837-1 rain climate zones, P.530 multipath and rain fading/availability, P.676-13
//!   gaseous attenuation, P.841-7 worst-month conversion.
//! * [`antenna`] – antenna pattern model, MSI/Planet file reader/writer, 3-D gain
//!   interpolation, ITU-R F.1336/F.699/F.1245 based pattern generators and a built-in
//!   generic antenna catalog.
//! * [`budget`] – link budget and field-strength (dBµV/m) conversions.
//!
//! All functions are pure, all types are `Send + Sync`, and there is no global mutable
//! state. Units are always spelled out in the function name or documentation:
//! frequencies are MHz unless the name says `_ghz`, distances are metres unless the name
//! says `_km`, angles are degrees.
//!
//! Longley-Rice / ITM is intentionally **not** part of this crate (see `pathless-itm`).

#![forbid(unsafe_code)]
#![warn(missing_docs)]

pub mod antenna;
pub mod basic;
pub mod budget;
pub mod diffraction;
pub mod error;
pub mod itu;

/// Alias of [`basic`]: unit conversions and elementary RF formulas.
pub use basic as units;
pub use error::RfError;

#[cfg(test)]
mod send_sync_tests {
    fn assert_send_sync<T: Send + Sync>() {}

    #[test]
    fn public_types_are_send_sync() {
        assert_send_sync::<crate::antenna::AntennaPattern>();
        assert_send_sync::<crate::antenna::AntennaSpec>();
        assert_send_sync::<crate::diffraction::DiffractionResult>();
        assert_send_sync::<crate::diffraction::LosClearance>();
        assert_send_sync::<crate::itu::availability::MicrowaveAvailability>();
        assert_send_sync::<crate::budget::LinkBudgetResult>();
        assert_send_sync::<crate::RfError>();
    }
}
