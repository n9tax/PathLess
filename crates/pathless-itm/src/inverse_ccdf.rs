//! Port of `src/InverseComplementaryCumulativeDistributionFunction.cpp`.

/// Inverse complementary cumulative distribution function approximation,
/// Formula 26.2.23 in Abramowitz & Stegun (|error| < 4.5e-4).
/// `q` is the quantile, 0.0 < q < 1.0.
pub(crate) fn inverse_ccdf(q: f64) -> f64 {
    const C_0: f64 = 2.515516;
    const C_1: f64 = 0.802853;
    const C_2: f64 = 0.010328;
    const D_1: f64 = 1.432788;
    const D_2: f64 = 0.189269;
    const D_3: f64 = 0.001308;

    let mut x = q;
    if q > 0.5 {
        x = 1.0 - x;
    }

    let t_x = (-2.0 * x.ln()).sqrt();

    let zeta_x = ((C_2 * t_x + C_1) * t_x + C_0) / (((D_3 * t_x + D_2) * t_x + D_1) * t_x + 1.0);

    let mut q_q = t_x - zeta_x;

    if q > 0.5 {
        q_q = -q_q;
    }

    q_q
}

#[cfg(test)]
mod tests {
    #[test]
    fn symmetric_and_sane() {
        assert!(super::inverse_ccdf(0.5).abs() < 1e-3);
        let a = super::inverse_ccdf(0.1);
        assert!((a - 1.2816).abs() < 5e-4);
        assert!((super::inverse_ccdf(0.9) + a).abs() < 1e-12);
    }
}
