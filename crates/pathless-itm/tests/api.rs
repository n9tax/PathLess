//! API behaviour tests: error mapping, profile validation, serde, threading.

use pathless_itm::*;

fn params() -> P2pParams {
    P2pParams {
        h_tx_m: 15.0,
        h_rx_m: 3.0,
        climate: Climate::ContinentalTemperate,
        n_0: 301.0,
        f_mhz: 3500.0,
        polarization: Polarization::Vertical,
        epsilon: ground::AVERAGE.epsilon,
        sigma: ground::AVERAGE.sigma,
        mdvar: ModeOfVariability::Accidental.mdvar(false, false),
    }
}

fn profile() -> Vec<f64> {
    (0..=142).map(|i| 1692.0 + (i as f64) * 0.12).collect()
}

#[test]
fn cr_maps_time_and_situation_errors() {
    let p = params();
    let z = profile();
    assert_eq!(p2p_cr_profile(&z, 25.6, &p, 50.0, 0.0), Err(ItmError::InvalidReliability));
    assert_eq!(p2p_cr_profile(&z, 25.6, &p, 100.0, 50.0), Err(ItmError::InvalidConfidence));
    assert_eq!(p2p_tls_profile(&z, 25.6, &p, 0.0, 50.0, 50.0), Err(ItmError::InvalidTime));
    assert_eq!(p2p_tls_profile(&z, 25.6, &p, 50.0, 100.0, 50.0), Err(ItmError::InvalidLocation));
    assert_eq!(p2p_tls_profile(&z, 25.6, &p, 50.0, 50.0, 0.0), Err(ItmError::InvalidSituation));

    let a = AreaInput {
        h_tx_m: 10.0,
        h_rx_m: 2.0,
        tx_siting: SitingCriteria::Random,
        rx_siting: SitingCriteria::Careful,
        d_km: 10.0,
        delta_h_m: 90.0,
        climate: Climate::ContinentalTemperate,
        n_0: 301.0,
        f_mhz: 150.0,
        polarization: Polarization::Horizontal,
        epsilon: 15.0,
        sigma: 0.005,
        mdvar: 1,
    };
    assert!(area_cr(&a, 50.0, 50.0).is_ok());
    assert_eq!(area_cr(&a, 50.0, 100.0), Err(ItmError::InvalidReliability));
    assert_eq!(area_cr(&a, 0.0, 50.0), Err(ItmError::InvalidConfidence));
    assert_eq!(area_tls(&AreaInput { d_km: 0.0, ..a }, 50.0, 50.0, 50.0), Err(ItmError::PathDistance));
    assert_eq!(area_tls(&AreaInput { delta_h_m: -1.0, ..a }, 50.0, 50.0, 50.0), Err(ItmError::DeltaH));
}

#[test]
fn ntia_validation_errors_and_order() {
    let z = profile();
    let p = params();
    let e = |p: P2pParams| p2p_tls_profile(&z, 25.6, &p, 50.0, 50.0, 50.0).unwrap_err();
    assert_eq!(e(P2pParams { h_tx_m: 0.4, ..p }), ItmError::TxTerminalHeight);
    assert_eq!(e(P2pParams { h_rx_m: 3001.0, ..p }), ItmError::RxTerminalHeight);
    assert_eq!(e(P2pParams { n_0: 200.0, ..p }), ItmError::Refractivity);
    assert_eq!(e(P2pParams { f_mhz: 19.0, ..p }), ItmError::Frequency);
    assert_eq!(e(P2pParams { epsilon: 0.5, ..p }), ItmError::Epsilon);
    assert_eq!(e(P2pParams { sigma: 0.0, ..p }), ItmError::Sigma);
    assert_eq!(e(P2pParams { mdvar: 5, ..p }), ItmError::Mdvar);
    // horizontal polarization over very lossy ground -> ground impedance check
    assert_eq!(
        e(P2pParams { epsilon: 1.0, sigma: 5.0, f_mhz: 20.0, polarization: Polarization::Horizontal, ..p }),
        ItmError::GroundImpedance
    );
    // NTIA input errors take precedence over the profile check
    assert_eq!(
        p2p_tls_profile(&[], 25.6, &P2pParams { h_tx_m: 0.1, ..p }, 50.0, 50.0, 50.0),
        Err(ItmError::TxTerminalHeight)
    );
    assert_eq!(ItmError::TxTerminalHeight.code(), 1000);
}

#[test]
fn warnings_are_reported() {
    let z = profile();
    let p = P2pParams { h_tx_m: 0.75, f_mhz: 30.0, ..params() };
    let out = p2p_tls_profile(&z, 25.6, &p, 50.0, 50.0, 99.95).unwrap();
    assert_ne!(out.warnings & warnings::TX_TERMINAL_HEIGHT, 0);
    assert_ne!(out.warnings & warnings::FREQUENCY, 0);
    assert_ne!(out.warnings & warnings::EXTREME_VARIABILITIES, 0);
    assert_eq!(out.return_code(), SUCCESS_WITH_WARNINGS);
    assert!(warnings::describe(out.warnings).count() >= 3);
}

#[test]
fn invalid_profiles_are_rejected_not_panicking() {
    let p = params();
    for (z, step) in [
        (vec![], 30.0),
        (vec![100.0], 30.0),
        (vec![100.0, 101.0], 0.0),
        (vec![100.0, 101.0], -5.0),
        (vec![100.0, 101.0], f64::NAN),
        (vec![100.0, 101.0], f64::INFINITY),
    ] {
        assert_eq!(p2p_tls_profile(&z, step, &p, 50.0, 50.0, 50.0), Err(ItmError::InvalidProfile));
    }
    // malformed NTIA pfl arrays
    for pfl in [vec![], vec![2.0, 10.0, 1.0, 2.0], vec![1.5, 10.0, 1.0, 2.0, 3.0], vec![0.0, 10.0, 1.0, 2.0]] {
        assert_eq!(p2p_tls_pfl(&pfl, &p, 50.0, 50.0, 50.0), Err(ItmError::InvalidProfile));
    }
    // minimal 2-point and 3-point profiles must not panic
    for z in [vec![100.0, 100.0], vec![100.0, 150.0, 100.0]] {
        let _ = p2p_tls_profile(&z, 500.0, &p, 50.0, 50.0, 50.0);
    }
    // NaN elevations must not panic either (result is unspecified)
    let mut z = profile();
    z[40] = f64::NAN;
    let _ = p2p_tls_profile(&z, 25.6, &p, 50.0, 50.0, 50.0);
}

#[test]
fn pfl_and_profile_variants_agree() {
    let z = profile();
    let mut pfl = vec![(z.len() - 1) as f64, 25.6];
    pfl.extend_from_slice(&z);
    let p = params();
    let a = p2p_tls_pfl(&pfl, &p, 10.0, 50.0, 90.0).unwrap();
    let b = p2p_tls_profile(&z, 25.6, &p, 10.0, 50.0, 90.0).unwrap();
    assert_eq!(a, b);
    let c = p2p_cr_pfl(&pfl, &p, 90.0, 10.0).unwrap();
    assert_eq!(a, c); // CR(conf, rel) == TLS(rel, 50, conf)
}

#[test]
fn compute_delta_h_helper() {
    let z = profile();
    let d = (z.len() - 1) as f64 * 25.6;
    let dh = compute_delta_h_m(&z, 25.6, 0.0, d).unwrap();
    assert!(dh.is_finite() && dh >= 0.0);
    assert_eq!(compute_delta_h_m(&z, 25.6, 0.0, d + 1.0), Err(ItmError::InvalidProfile));
}

#[test]
fn serde_roundtrip() {
    let input = P2pInput::new(&params(), profile(), 25.6);
    let json = serde_json::to_string(&input).unwrap();
    let back: P2pInput = serde_json::from_str(&json).unwrap();
    assert_eq!(input, back);
    let out = p2p_tls(&input, 50.0, 50.0, 50.0).unwrap();
    let json = serde_json::to_string(&out).unwrap();
    let back: ItmOutput = serde_json::from_str(&json).unwrap();
    // serde_json (without its `float_roundtrip` feature) may differ in the last ULP
    assert!((out.loss_db - back.loss_db).abs() < 1e-9);
    assert!((out.intermediate.theta_hzn[0] - back.intermediate.theta_hzn[0]).abs() < 1e-15);
    assert_eq!(out.warnings, back.warnings);
    assert_eq!(out.intermediate.mode, back.intermediate.mode);
    let err_json = serde_json::to_string(&ItmError::GroundImpedance).unwrap();
    assert_eq!(serde_json::from_str::<ItmError>(&err_json).unwrap(), ItmError::GroundImpedance);
}

#[test]
fn concurrent_calls_are_deterministic() {
    let z = profile();
    let p = params();
    let reference = p2p_tls_profile(&z, 25.6, &p, 50.0, 50.0, 50.0).unwrap();
    std::thread::scope(|s| {
        for _ in 0..8 {
            s.spawn(|| {
                for _ in 0..200 {
                    assert_eq!(p2p_tls_profile(&z, 25.6, &p, 50.0, 50.0, 50.0).unwrap(), reference);
                }
            });
        }
    });
}
