//! ITU-R P.841-7 (08/2022): conversion of annual statistics to worst-month statistics.
//!
//! `pw = Q(p)·p` with the two-parameter function Q(p; Q1, β) of eq. (4); the inverse
//! (worst month → annual) is solved numerically (`p = pw / Q`, eq. (5)).

use serde::{Deserialize, Serialize};

/// Q1 and β parameters of P.841.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct P841Params {
    /// Q1.
    pub q1: f64,
    /// β.
    pub beta: f64,
}

impl P841Params {
    /// Global values for multipath and terrestrial rain attenuation: Q1 = 2.85, β = 0.13
    /// (P.841-7 §4, also used by P.530-19 §2.3.4).
    pub const GLOBAL: P841Params = P841Params { q1: 2.85, beta: 0.13 };
    /// Rain rate, tropical / subtropical / temperate regions with frequent rain: 2.82, 0.15.
    pub const RAIN_RATE_FREQUENT_RAIN: P841Params = P841Params { q1: 2.82, beta: 0.15 };
    /// Rain rate, dry temperate, polar and desert regions: 4.48, 0.11.
    pub const RAIN_RATE_DRY: P841Params = P841Params { q1: 4.48, beta: 0.11 };
}

impl Default for P841Params {
    fn default() -> Self {
        Self::GLOBAL
    }
}

/// Conversion factor Q(p) of P.841-7 eq. (4); `p_annual_percent` is the annual exceedance
/// percentage. 1 ≤ Q ≤ 12.
pub fn q_factor(p_annual_percent: f64, params: &P841Params) -> f64 {
    let P841Params { q1, beta } = *params;
    let p = p_annual_percent;
    let p0 = (q1 / 12.0).powf(1.0 / beta);
    let q3 = q1 * 3f64.powf(-beta);
    let q = if p < p0 {
        12.0
    } else if p < 3.0 {
        q1 * p.powf(-beta)
    } else if p < 30.0 {
        q3
    } else {
        q3 * (p / 30.0).powf(q3.log10() / 0.3f64.log10())
    };
    q.clamp(1.0, 12.0)
}

/// Annual percentage p → average annual worst-month percentage pw = Q·p (%).
pub fn worst_month_from_annual(p_annual_percent: f64, params: &P841Params) -> f64 {
    (q_factor(p_annual_percent, params) * p_annual_percent).min(100.0)
}

/// Average annual worst-month percentage pw → annual percentage p = pw/Q (%), eq. (5),
/// solved by bisection (pw(p) is monotonic).
pub fn annual_from_worst_month(pw_percent: f64, params: &P841Params) -> f64 {
    if pw_percent <= 0.0 {
        return 0.0;
    }
    let pw = pw_percent.min(100.0);
    // Q ∈ [1, 12] ⇒ p ∈ [pw/12, pw]
    let (mut lo, mut hi) = ((pw / 12.0).ln(), pw.ln());
    for _ in 0..200 {
        let mid = 0.5 * (lo + hi);
        if worst_month_from_annual(mid.exp(), params) < pw {
            lo = mid;
        } else {
            hi = mid;
        }
    }
    (0.5 * (lo + hi)).exp()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn global_relationship_eq7() {
        // P.841-7 eq. (7): p = 0.30·pw^1.15 for 1.9e-4 ≤ pw ≤ 7.8 (global parameters)
        for pw in [2e-4, 1e-3, 0.01, 0.1, 1.0, 5.0] {
            let p = annual_from_worst_month(pw, &P841Params::GLOBAL);
            let approx = 0.30 * f64::powf(pw, 1.15);
            assert!(((p - approx) / approx).abs() < 0.02, "pw={pw}: {p} vs {approx}");
            // exact closed form in the middle range: p = (pw/Q1)^(1/(1-β))
            let exact = (pw / 2.85f64).powf(1.0 / 0.87);
            assert!(((p - exact) / exact).abs() < 1e-9, "pw={pw}: {p} vs {exact}");
        }
    }

    #[test]
    fn q_limits_and_roundtrip() {
        let g = P841Params::GLOBAL;
        assert_eq!(q_factor(1e-12, &g), 12.0);
        assert!((q_factor(10.0, &g) - 2.85 * 3f64.powf(-0.13)).abs() < 1e-12);
        assert!((worst_month_from_annual(100.0, &g) - 100.0).abs() < 1e-6);
        for p in [1e-6, 1e-4, 0.01, 0.5, 2.0, 10.0, 50.0] {
            let pw = worst_month_from_annual(p, &g);
            let back = annual_from_worst_month(pw, &g);
            assert!(((back - p) / p).abs() < 1e-9, "{p} -> {pw} -> {back}");
        }
        assert_eq!(annual_from_worst_month(0.0, &g), 0.0);
    }
}
