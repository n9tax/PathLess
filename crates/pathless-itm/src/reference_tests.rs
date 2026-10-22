//! Function-level unit tests ported from the NTIA/ITS GoogleTest suite on the
//! `CrossPlatform-and-PropLib2.0` branch of https://github.com/NTIA/itm
//! (`tests/Test*.cpp`). That branch's model logic is identical to `master`
//! (only cosmetic/namespace changes), so its full-precision expected values
//! apply to this port. `EXPECT_DOUBLE_EQ` (4 ULP) is relaxed to a relative
//! tolerance of 1e-12 to allow for libm differences between platforms.

// Expected values are copied verbatim from the upstream tests.
#![allow(clippy::excessive_precision)]

use crate::common::{ItmMode, Pfl, PI};
use crate::complex::Complex;
use crate::compute_delta_h::compute_delta_h;
use crate::diffraction_loss::diffraction_loss;
use crate::enums::PropMode;
use crate::find_horizons::find_horizons;
use crate::inverse_ccdf::inverse_ccdf;
use crate::line_of_sight_loss::line_of_sight_loss;
use crate::linear_least_squares_fit::linear_least_squares_fit;
use crate::longley_rice::longley_rice;
use crate::quick_pfl::quick_pfl;
use crate::smooth_earth_diffraction::{height_function, smooth_earth_diffraction};
use crate::troposcatter_loss::{f_function, troposcatter_loss};
use crate::validate_inputs::validate_inputs;
use crate::variability::variability;
use crate::warnings::*;
use crate::ItmError;

const REL_TOL: f64 = 1e-12;

#[track_caller]
fn assert_close(actual: f64, expected: f64) {
    let tol = REL_TOL * expected.abs().max(1e-300);
    assert!(
        (actual - expected).abs() <= tol,
        "got {actual:.17e}, expected {expected:.17e} (rel err {:.3e})",
        ((actual - expected) / expected).abs()
    );
}

/// View an NTIA pfl array as the internal profile type.
fn pfl(a: &[f64]) -> Pfl<'_> {
    let np = a[0] as usize;
    Pfl { np, xi: a[1], z: &a[2..np + 3] }
}

// ---------------------------------------------------------------- ComputeDeltaH

#[test]
fn compute_delta_h_interpolate_exact_indices() {
    let a = [9.0, 10.0, 100.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 100.0];
    assert_close(compute_delta_h(pfl(&a), 10.0, 90.0), 130.80086741994160);
}

#[test]
fn compute_delta_h_interpolate_inexact_indices() {
    let a = [9.0, 10.0, 100.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 100.0];
    assert_close(compute_delta_h(pfl(&a), 15.0, 85.0), 41.639683828192240);
}

const LARGE_PFL: [f64; 36] = [
    33.0, 10.0, 100.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 2.0, 2.0, 2.0, 1.0, 1.0, 1.0, 1.0, 2.0, 2.0, 2.0, 1.0,
    1.0, 1.0, 2.0, 2.0, 2.0, 1.0, 1.0, 1.0, 1.0, 1.0, 2.0, 2.0, 2.0, 100.0,
];

#[test]
fn compute_delta_h_sample_exact_indices() {
    assert_close(compute_delta_h(pfl(&LARGE_PFL), 10.0, 320.0), 5.5481498660992807);
}

#[test]
fn compute_delta_h_sample_inexact_indices() {
    assert_close(compute_delta_h(pfl(&LARGE_PFL), 15.0, 315.0), 5.4395216034312366);
}

#[test]
fn compute_delta_h_short_circuit() {
    let a = [4.0, 10.0, 1.0, 2.0, 3.0, 4.0, 5.0];
    assert_eq!(compute_delta_h(pfl(&a), 40.0, 50.0), 0.0);
}

// ---------------------------------------------------------------- FindHorizons

#[test]
fn find_horizons_symmetric() {
    let a = [4.0, 10.0, 0.0, 10.0, 20.0, 10.0, 0.0];
    let (theta, d) = find_horizons(pfl(&a), 6371000.0, &[5.0, 5.0]);
    assert_close(theta[0], theta[1]);
    assert_close(d[0], d[1]);
}

#[test]
fn find_horizons_initialization() {
    let a = [1.0, 10.0, 2.0, 4.0];
    let h = [1.0, 5.0];
    let a_e = 6371000.0;
    let d_km = a[1] * a[0] * 1e-3;
    let z_tx = a[2] + h[0];
    let z_rx = a[3] + h[1];
    let d_e = 2.0 * a_e;
    let (theta, d) = find_horizons(pfl(&a), a_e, &h);
    assert_close(theta[0], (z_rx - z_tx) / (d_km * 1e3) - (d_km * 1e3) / d_e);
    assert_close(theta[1], -(z_rx - z_tx) / (d_km * 1e3) - (d_km * 1e3) / d_e);
    assert_eq!(d[0], d_km * 1e3);
    assert_eq!(d[1], d_km * 1e3);
}

#[test]
fn find_horizons_small_profile() {
    let a = [2.0, 10.0, 2.0, 10.0, 4.0];
    let h = [1.0, 5.0];
    let a_e = 1000000.0;
    let d_km = a[0] * a[1] * 1e-3;
    let num_loops = a[0] as usize - 1;
    let (theta, d) = find_horizons(pfl(&a), a_e, &h);
    let z_tx = a[2] + h[0];
    let z_rx = a[4] + h[1];
    let d_e = a_e * 2.0;
    let d_tx = num_loops as f64 * a[1];
    let d_rx = d_km * 1e3 - num_loops as f64 * a[1];
    assert_close(theta[0], (a[num_loops + 2] - z_tx) / d_tx - d_tx / d_e);
    assert_close(theta[1], -(z_rx - a[num_loops + 2]) / d_rx - d_rx / d_e);
    assert_close(d[0], d_tx);
    assert_close(d[1], d_rx);
}

// ---------------------------------------------------------------- QuickPfl

#[test]
fn quick_pfl_simple() {
    let a = [8.0, 100.0, 1.0, 2.0, 3.0, 5.0, 3.0, 5.0, 3.0, 2.0, 1.0];
    let r = quick_pfl(pfl(&a), 1.0 / 7860e3, &[6.0, 5.0]);
    assert_close(r.theta_hzn[0], -0.0011422640014581357);
    assert_close(r.theta_hzn[1], -0.0010153790362535343);
    assert_close(r.delta_h_m, 12.901387666337202);
    assert_close(r.d_hzn_m[0], 8764.4253963572828);
    assert_close(r.d_hzn_m[1], 7922.7879168398858);
    assert_close(r.h_e_m[0], 6.0);
    assert_close(r.h_e_m[1], 5.0);
    assert_eq!(r.d_m, a[0] * a[1]);
}

// ---------------------------------------------------------------- LinearLeastSquaresFit

#[test]
fn linear_least_squares_fit_cases() {
    let a = [4.0, 1.0, 1.0, 2.0, 3.0, 4.0, 5.0];
    // (d_start, d_stop): basic, zero range at start/middle/end, negative range at start/end
    for (ds, de) in [(0.0, 4.0), (0.0, 0.0), (2.0, 2.0), (4.0, 4.0), (1.0, 0.0), (4.0, 3.0)] {
        let (y1, y2) = linear_least_squares_fit(pfl(&a), ds, de);
        assert_close(y1, a[2]);
        assert_close(y2, a[6]);
    }
}

// ---------------------------------------------------------------- LongleyRice

struct LrFixture {
    theta_hzn: [f64; 2],
    d_hzn_m: [f64; 2],
    h_e_m: [f64; 2],
    h_m: [f64; 2],
    d_m: f64,
}

impl LrFixture {
    fn new() -> Self {
        Self { theta_hzn: [-1.9, -0.8], d_hzn_m: [15e3, 7e3], h_e_m: [15.0, 5.0], h_m: [15.0, 5.0], d_m: 10000.0 }
    }
    const GAMMA_E: f64 = 1.0 / 7860e3;

    fn run(&self) -> (Result<(f64, PropMode), ItmError>, u32) {
        let mut w = 0;
        let r = longley_rice(
            &self.theta_hzn,
            3500.0,
            Complex::new(0.2, 0.0),
            &self.d_hzn_m,
            &self.h_e_m,
            Self::GAMMA_E,
            252.0,
            3.5,
            &self.h_m,
            self.d_m,
            ItmMode::P2p,
            &mut w,
        );
        (r, w)
    }
}

#[test]
fn longley_rice_paths() {
    let mut f = LrFixture::new();
    let (r, _) = f.run();
    let (a, m) = r.unwrap();
    assert_eq!(m, PropMode::LineOfSight);
    assert_close(a, 4.2910433148599818);

    f.d_m = 25e3;
    let (a, m) = f.run().0.unwrap();
    assert_eq!(m, PropMode::Diffraction);
    assert_close(a, 23.008174136481173);

    f.d_m = 10e6 + 1.0;
    let (a, m) = f.run().0.unwrap();
    assert_eq!(m, PropMode::Troposcatter);
    assert_close(a, 10257.44380116946);
}

#[test]
fn longley_rice_warnings() {
    let mut f = LrFixture::new();
    f.theta_hzn = [201e-3, 199e-3];
    assert_eq!(f.run().1, TX_HORIZON_ANGLE);

    let mut f = LrFixture::new();
    f.theta_hzn = [199e-3, 201e-3];
    assert_eq!(f.run().1, RX_HORIZON_ANGLE);

    for (i, factor, delta, flag) in [
        (0, 0.1, -1.0, TX_HORIZON_DISTANCE_1),
        (1, 0.1, -1.0, RX_HORIZON_DISTANCE_1),
        (0, 3.0, 1.0, TX_HORIZON_DISTANCE_2),
        (1, 3.0, 1.0, RX_HORIZON_DISTANCE_2),
    ] {
        let mut f = LrFixture::new();
        let d_hzn_s = (2.0 * f.h_e_m[i] / LrFixture::GAMMA_E).sqrt();
        f.d_hzn_m[i] = factor * d_hzn_s + delta;
        f.theta_hzn = [199e-3, 199e-3];
        let (r, w) = f.run();
        assert!(r.is_ok());
        assert_eq!(w, flag);
    }

    let mut f = LrFixture::new();
    f.theta_hzn = [199e-3, 199e-3];
    f.d_m = (f.h_e_m[0] - f.h_e_m[1]).abs() / 200e-3 - 1.0;
    assert_eq!(f.run().1, PATH_DISTANCE_TOO_SMALL_1 | PATH_DISTANCE_TOO_SMALL_2);

    let mut f = LrFixture::new();
    f.theta_hzn = [199e-3, 199e-3];
    f.h_e_m[0] = 206.0;
    f.d_m = (f.h_e_m[0] - f.h_e_m[1]).abs() / 200e-3 - 1.0;
    assert_eq!(f.run().1, PATH_DISTANCE_TOO_SMALL_1);

    let mut f = LrFixture::new();
    f.theta_hzn = [199e-3, 199e-3];
    f.d_m = 1001e3;
    assert_eq!(f.run().1, PATH_DISTANCE_TOO_BIG_1);

    let mut f = LrFixture::new();
    f.theta_hzn = [199e-3, 199e-3];
    f.d_m = 2001e3;
    assert_eq!(f.run().1, PATH_DISTANCE_TOO_BIG_1 | PATH_DISTANCE_TOO_BIG_2);
}

// ---------------------------------------------------------------- DiffractionLoss & friends

#[test]
fn diffraction_loss_case() {
    let d_hzn = [2500.0, 3000.0];
    let h_e: [f64; 2] = [8.0, 5.0];
    let a_e = 6371000.0;
    let theta_hzn = [-0.19, -0.08];
    let d_hzn_s = [(2.0 * h_e[0] * a_e).sqrt(), (2.0 * h_e[1] * a_e).sqrt()];
    let d_sml = d_hzn_s[0] + d_hzn_s[1];
    let d_ml = d_hzn[0] + d_hzn[1];
    let theta_los = -f64::max(theta_hzn[0] + theta_hzn[1], -d_ml / a_e);
    let r = diffraction_loss(
        1000.0,
        &d_hzn,
        &h_e,
        Complex::new(1.0, 0.1),
        a_e,
        10.0,
        &[8.0, 5.0],
        ItmMode::P2p,
        theta_los,
        d_sml,
        3500.0,
    );
    assert_close(r, 14.728185720658667);
}

#[test]
fn smooth_earth_diffraction_case() {
    let r = smooth_earth_diffraction(18e3, 3500.0, 7850e3, 0.002, &[8e3, 8e3], &[15.0, 3.0], Complex::new(0.2, 0.0));
    assert_close(r, 25.355962945991564);
}

#[test]
fn height_function_cases() {
    let (x, k) = (150.0f64, 1.0f64);
    assert_close(height_function(x, k), 2.5e-5 * x.powi(2) / k - 8.686 * -k.ln() - 15.0);
    let k = 1e-6;
    assert_close(height_function(x, k), 17.372 * x.ln() - 117.0);
    assert_close(height_function(0.9, k), -117.0);
    let x = 250.0f64;
    let mut r = 0.05751 * x - 4.343 * x.ln();
    let w = 0.0134 * x * (-0.005 * x).exp();
    r = (1.0 - w) * r + w * (17.372 * x.ln() - 117.0);
    assert_close(height_function(x, k), r);
    assert_close(height_function(x, k * 100.0), r);
    let x = 2001.0f64;
    assert_close(height_function(x, k), 0.05751 * x - 4.343 * x.ln());
}

#[test]
fn line_of_sight_loss_cases() {
    let h_e = [1.0, 2.0];
    let z_g = Complex::new(0.2, 0.0);
    // small internal q
    assert_close(line_of_sight_loss(5e3, &h_e, z_g, 10.0, 0.05, 0.0, 100e3, 47.7 * 1e12), 249.99999994754504);
    // large internal q
    assert_close(line_of_sight_loss(5e3, &h_e, z_g, 10.0, 0.05, 0.0, 100e3, 47.7), 44.454186433741803);
}

#[test]
fn knife_edge_case() {
    let r = crate::knife_edge_diffraction::knife_edge_diffraction(6.0, PI * 47.7, 3.0, 1.0, &[1.0, 1.0]);
    let v = 0.0795775 * PI * 4.0 / 5.0;
    assert_close(r, 2.0 * crate::fresnel_integral::fresnel_integral(v));
}

#[test]
fn troposcatter_cases() {
    assert_close(f_function(9e3), 133.4 + 0.332e-3 * 9e3 - 10.0 * 9e3f64.log10());
    assert_close(f_function(30e3), 104.6 + 0.212e-3 * 30e3 - 2.5 * 30e3f64.log10());
    assert_close(f_function(75e3), 71.8 + 0.157e-3 * 75e3 + 5.0 * 75e3f64.log10());

    let theta = [-0.001, -0.001];
    let d_hzn = [8e3, 8e3];
    let h_e = [10.0, 5.0];
    let mut h0 = -1.0;
    let r = troposcatter_loss(100e3, &theta, &d_hzn, &h_e, 7850e3, 301.0, 3500.0, -0.02, &mut h0);
    assert_close(r, 76.969449309403771);

    h0 = 16.0;
    let th = 100e3 / 7850e3 - -0.02;
    let expected = f_function(th * 100e3) + 10.0 * (3500.0 * th.powf(4.0)).log10() + h0;
    let r = troposcatter_loss(100e3, &theta, &d_hzn, &h_e, 7850e3, 301.0, 3500.0, -0.02, &mut h0);
    assert_close(r, expected);
}

#[test]
fn inverse_ccdf_cases() {
    assert_close(inverse_ccdf(0.5), 2.378045045325905e-07);
    assert!((inverse_ccdf(0.1) - 1.281729).abs() < 4.5e-4);
    assert!((inverse_ccdf(0.49) - 0.024998).abs() < 4.5e-4);
    assert!((inverse_ccdf(0.51) + 0.024998).abs() < 4.5e-4);
    assert!((inverse_ccdf(0.9) + 1.281729).abs() < 4.5e-4);
}

// ---------------------------------------------------------------- Variability

// Note: upstream TestVariability.cpp passes time/location/situation as
// 0.3/0.6/0.9, but its expected values are those produced by the (identical)
// Variability() code for 30/60/90 percent - the function divides by 100
// internally. With 30/60/90 all 13 expected values match to ~1e-15.
fn var(climate: i32, mdvar: i32) -> (f64, u32) {
    let mut w = 0;
    let r = variability(30.0, 60.0, 90.0, &[100.0, 100.0], 10.0, 3500.0, 10000.0, 50.0, climate, mdvar, &mut w);
    (r, w)
}

#[test]
fn variability_climates() {
    let expected = [
        62.258514685008194,
        62.172817403455056,
        62.180947861894204,
        62.285710999957999,
        62.148868476830557,
        62.202217322742925,
        62.171720116596696,
    ];
    for (i, &e) in expected.iter().enumerate() {
        let (r, w) = var(i as i32 + 1, 3);
        assert_close(r, e);
        assert_eq!(w, 0);
    }
}

#[test]
fn variability_modes() {
    for (mdvar, e) in [
        (0, 65.983366483245064),
        (1, 65.975212768420008),
        (2, 54.938699908329184),
        (3, 62.258514685008194),
        (13, 59.833829899214862),
        (23, 53.043626273213889),
    ] {
        let (r, w) = var(1, mdvar);
        assert_close(r, e);
        assert_eq!(w, 0);
    }
}

// ---------------------------------------------------------------- ValidateInputs

#[test]
fn validate_inputs_cases() {
    // (h_tx, h_rx, climate, time, loc, sit, N_0, f, pol, eps, sigma, mdvar)
    #[derive(Clone, Copy)]
    struct Args(f64, f64, i32, f64, f64, f64, f64, f64, i32, f64, f64, i32);
    let base = Args(15.0, 15.0, 4, 50.0, 50.0, 50.0, 300.0, 1000.0, 1, 15.0, 0.005, 3);
    let run = |a: Args| {
        let mut w = 0;
        let r = validate_inputs(a.0, a.1, a.2, a.3, a.4, a.5, a.6, a.7, a.8, a.9, a.10, a.11, &mut w);
        (r, w)
    };
    assert_eq!(run(base), (Ok(()), 0));
    assert_eq!(run(Args { 0: 0.9, ..base }).1, TX_TERMINAL_HEIGHT);
    assert_eq!(run(Args { 0: 1000.1, ..base }).1, TX_TERMINAL_HEIGHT);
    assert_eq!(run(Args { 1: 0.9, ..base }).1, RX_TERMINAL_HEIGHT);
    assert_eq!(run(Args { 1: 1000.1, ..base }).1, RX_TERMINAL_HEIGHT);
    assert_eq!(run(Args { 0: 0.49, ..base }).0, Err(ItmError::TxTerminalHeight));
    assert_eq!(run(Args { 0: 3000.1, ..base }).0, Err(ItmError::TxTerminalHeight));
    assert_eq!(run(Args { 1: 0.49, ..base }).0, Err(ItmError::RxTerminalHeight));
    assert_eq!(run(Args { 1: 3000.1, ..base }).0, Err(ItmError::RxTerminalHeight));
    assert_eq!(run(Args { 2: 0, ..base }).0, Err(ItmError::InvalidRadioClimate));
    assert_eq!(run(Args { 2: 8, ..base }).0, Err(ItmError::InvalidRadioClimate));
    assert_eq!(run(Args { 6: 249.9, ..base }).0, Err(ItmError::Refractivity));
    assert_eq!(run(Args { 6: 400.1, ..base }).0, Err(ItmError::Refractivity));
    assert_eq!(run(Args { 7: 39.9, ..base }).1, FREQUENCY);
    assert_eq!(run(Args { 7: 10000.1, ..base }).1, FREQUENCY);
    assert_eq!(run(Args { 7: 19.9, ..base }).0, Err(ItmError::Frequency));
    assert_eq!(run(Args { 7: 20000.1, ..base }).0, Err(ItmError::Frequency));
    assert_eq!(run(Args { 8: 2, ..base }).0, Err(ItmError::Polarization));
    assert_eq!(run(Args { 9: 0.99, ..base }).0, Err(ItmError::Epsilon));
    assert_eq!(run(Args { 10: 0.0, ..base }).0, Err(ItmError::Sigma));
    for bad in [-1, 4, 9, 14, 19, 24, 29, 34] {
        assert_eq!(run(Args { 11: bad, ..base }).0, Err(ItmError::Mdvar), "mdvar {bad}");
    }
    for good in [0, 3, 10, 13, 20, 23, 30, 33] {
        assert_eq!(run(Args { 11: good, ..base }).0, Ok(()), "mdvar {good}");
    }
    assert_eq!(run(Args { 5: 0.0, ..base }).0, Err(ItmError::InvalidSituation));
    assert_eq!(run(Args { 5: 100.0, ..base }).0, Err(ItmError::InvalidSituation));
    assert_eq!(run(Args { 3: 0.0, ..base }).0, Err(ItmError::InvalidTime));
    assert_eq!(run(Args { 3: 100.0, ..base }).0, Err(ItmError::InvalidTime));
    assert_eq!(run(Args { 4: 0.0, ..base }).0, Err(ItmError::InvalidLocation));
    assert_eq!(run(Args { 4: 100.0, ..base }).0, Err(ItmError::InvalidLocation));
}
