//! # pathless-itm
//!
//! A faithful, pure-Rust port of the NTIA/ITS **Irregular Terrain Model**
//! (ITM, a.k.a. Longley-Rice) reference implementation v1.x
//! (<https://github.com/NTIA/itm>, public domain). Modules mirror the C++
//! source files one-to-one and preserve the constants, branch logic,
//! warning flags and error codes of the original.
//!
//! ## Model validity (from the NTIA README)
//!
//! | Input | Units | Valid range | Warning range |
//! |---|---|---|---|
//! | `h_tx_m`, `h_rx_m` | m above ground | 0.5 ..= 3000 | outside 1 ..= 1000 |
//! | `n_0` | N-units | 250 ..= 400 | |
//! | `f_mhz` | MHz | 20 ..= 20000 | outside 40 ..= 10000 |
//! | `epsilon` | - | >= 1 | |
//! | `sigma` | S/m | > 0 | |
//! | `mdvar` | - | 0..=3, +10, +20 | |
//! | time / location / situation / confidence / reliability | % | 0 < x < 100 | |z| > 3.10 (about <0.1 % or >99.9 %) |
//! | path distance | km | > 0 | < 1 km or > 1000 km |
//! | area `delta_h_m` | m | >= 0 | |
//!
//! ## Point-to-point profiles
//!
//! Terrain is passed as evenly spaced elevations (meters above mean sea
//! level) from the TX to the RX, both endpoints included, plus the spacing
//! in meters. The NTIA PFL array `[np, step, z0, ..., z_np]` is accepted by
//! the `*_pfl` variants. At least 2 points are required (otherwise
//! [`ItmError::InvalidProfile`]); ITM resamples the profile internally to at
//! most 245 points for the delta-h computation, so very dense profiles mostly
//! cost time in the horizon search, which is O(n). For meaningful results use
//! a spacing comparable to the DEM resolution (e.g. 30-100 m) and paths of at
//! least ~1 km.
//!
//! ## Threading / performance
//!
//! Everything is pure functions of their inputs: no global state, no
//! interior mutability, no heap allocation inside the model
//! ([`p2p_tls_profile`] borrows the elevation slice). All public types are
//! `Send + Sync`, so calls can be made freely from rayon worker threads.
//!
//! ## Example
//!
//! ```
//! use pathless_itm::*;
//! let params = P2pParams {
//!     h_tx_m: 30.0, h_rx_m: 2.0,
//!     climate: Climate::ContinentalTemperate, n_0: 301.0, f_mhz: 450.0,
//!     polarization: Polarization::Vertical,
//!     epsilon: ground::AVERAGE.epsilon, sigma: ground::AVERAGE.sigma,
//!     mdvar: ModeOfVariability::Broadcast.mdvar(false, false),
//! };
//! let terrain: Vec<f64> = (0..201).map(|i| 100.0 + 10.0 * ((i as f64) / 20.0).sin()).collect();
//! let out = p2p_tls_profile(&terrain, 50.0, &params, 50.0, 50.0, 50.0).unwrap();
//! assert!(out.loss_db > out.intermediate.a_fs_db - 10.0);
//! ```

// Range checks are written as in the C++ (`x < a || x > b`) rather than
// `!(a..=b).contains(&x)`, because the two differ for NaN inputs.
#![allow(clippy::manual_range_contains)]

mod common;
mod complex;
mod compute_delta_h;
mod diffraction_loss;
mod enums;
mod errors;
mod find_horizons;
mod free_space_loss;
mod fresnel_integral;
pub mod ground;
mod h0_function;
mod initialize_area;
mod initialize_point_to_point;
mod inverse_ccdf;
mod itm_area;
mod itm_p2p;
mod knife_edge_diffraction;
mod line_of_sight_loss;
mod linear_least_squares_fit;
mod longley_rice;
mod quick_pfl;
#[cfg(test)]
mod reference_tests;
mod sigma_h_function;
mod smooth_earth_diffraction;
mod terrain_roughness;
mod troposcatter_loss;
mod types;
mod validate_inputs;
mod variability;
pub mod warnings;

pub use enums::{Climate, ModeOfVariability, Polarization, PropMode, SitingCriteria};
pub use errors::{ItmError, SUCCESS, SUCCESS_WITH_WARNINGS};
pub use free_space_loss::free_space_loss_db;
pub use itm_area::{area_cr, area_tls, AreaInput};
pub use itm_p2p::{p2p_cr, p2p_cr_pfl, p2p_cr_profile, p2p_tls, p2p_tls_pfl, p2p_tls_profile, P2pInput, P2pParams};
pub use types::{IntermediateValues, ItmOutput};

/// Terrain irregularity parameter delta_h (meters) of a profile, computed
/// exactly as ITM does internally (NTIA `ComputeDeltaH`): the interdecile
/// range of terrain heights about a least-squares line, over the section
/// from `d_start_m` to `d_end_m` (distances from the first point, meters),
/// scaled to an "infinite path" value. Useful for feeding [`area_tls`].
///
/// Requires `0 <= d_start_m <= d_end_m <= (len - 1) * step_m`; sections
/// shorter than two profile intervals give 0.
pub fn compute_delta_h_m(elevations_m: &[f64], step_m: f64, d_start_m: f64, d_end_m: f64) -> Result<f64, ItmError> {
    let pfl = itm_p2p::pfl_from_profile(elevations_m, step_m)?;
    let d_m = pfl.np_f() * pfl.xi;
    if !(d_start_m >= 0.0 && d_start_m <= d_end_m && d_end_m <= d_m) {
        return Err(ItmError::InvalidProfile);
    }
    Ok(compute_delta_h::compute_delta_h(pfl, d_start_m, d_end_m))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn assert_send_sync<T: Send + Sync>() {}

    #[test]
    fn public_types_are_send_sync() {
        assert_send_sync::<P2pInput>();
        assert_send_sync::<P2pParams>();
        assert_send_sync::<AreaInput>();
        assert_send_sync::<ItmOutput>();
        assert_send_sync::<ItmError>();
    }

    #[test]
    fn free_space_loss_formula() {
        for &(d_km, f) in &[(1.0, 100.0), (10.0, 450.0), (37.5, 2400.0), (250.0, 10000.0), (0.5, 20.0)] {
            let expected = 32.45 + 20.0 * f64::log10(f) + 20.0 * f64::log10(d_km);
            let got = free_space_loss_db(d_km * 1000.0, f);
            assert!((got - expected).abs() < 1e-12, "{d_km} km {f} MHz: {got} vs {expected}");
        }
        // Sanity: 1 km at 1000 MHz is ~92.45 dB
        assert!((free_space_loss_db(1000.0, 1000.0) - 92.45).abs() < 1e-12);
    }

    #[test]
    fn mdvar_helpers() {
        assert_eq!(ModeOfVariability::Mobile.mdvar(true, true), 32);
        assert_eq!(ModeOfVariability::decompose(12).unwrap(), (ModeOfVariability::Mobile, true, false));
        assert_eq!(ModeOfVariability::decompose(23).unwrap(), (ModeOfVariability::Broadcast, false, true));
        assert_eq!(ModeOfVariability::decompose(4), Err(ItmError::Mdvar));
    }

    #[test]
    fn error_codes_roundtrip() {
        for code in 1000..=1100 {
            if let Some(e) = ItmError::from_code(code) {
                assert_eq!(e.code(), code);
            }
        }
        assert_eq!(ItmError::from_code(1015), None);
        assert_eq!(ItmError::GroundImpedance.code(), 1013);
    }
}
