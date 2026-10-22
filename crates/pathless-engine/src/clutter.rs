//! Terminal clutter loss — ITU-R P.2108-1 §3.1 "height gain terminal
//! correction model" (urban / suburban / forest form, eq. 2a).
//!
//! Only the diffraction form (2a) is applied. The open/rural/water form (2b)
//! is a height-gain correction relative to a 10 m reference, which ITM already
//! accounts for by using the real antenna height, so applying it would double
//! count.

use pathless_rf::diffraction::knife_edge_loss_db;

/// NLCD classes that obstruct a terminal sitting inside them:
/// developed low/medium/high, deciduous/evergreen/mixed forest, woody wetland.
pub fn is_obstructing_class(class: u8) -> bool {
    matches!(class, 22 | 23 | 24 | 41 | 42 | 43 | 90)
}

/// Additional loss (dB) for a terminal at height `h_m` above ground standing
/// in clutter of representative height `clutter_m`.
///
/// `f_mhz` is clamped to the recommendation's 30 MHz – 3 GHz range.
/// `street_width_m` defaults to 27 m in the recommendation.
pub fn terminal_clutter_loss_db(f_mhz: f64, h_m: f64, clutter_m: f64, street_width_m: f64) -> f64 {
    if clutter_m <= 0.0 || h_m >= clutter_m {
        return 0.0;
    }
    let f_ghz = (f_mhz / 1000.0).clamp(0.03, 3.0);
    let h_dif = clutter_m - h_m;
    let theta_clut_deg = (h_dif / street_width_m).atan().to_degrees();
    let k_nu = 0.342 * f_ghz.sqrt();
    let nu = k_nu * (h_dif * theta_clut_deg).sqrt();
    (knife_edge_loss_db(nu) - 6.03).max(0.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn loss_is_zero_at_clutter_height() {
        // J(0) from P.526 eq. 31 is 6.03 dB, which the model subtracts.
        assert!((knife_edge_loss_db(0.0) - 6.03).abs() < 0.01);
    }

    #[test]
    fn no_loss_above_clutter() {
        assert_eq!(terminal_clutter_loss_db(900.0, 20.0, 15.0, 27.0), 0.0);
        assert!(terminal_clutter_loss_db(900.0, 15.0, 15.0, 27.0).abs() < 0.05);
    }

    #[test]
    fn loss_grows_with_frequency_and_depth() {
        let a = terminal_clutter_loss_db(150.0, 1.5, 15.0, 27.0);
        let b = terminal_clutter_loss_db(2000.0, 1.5, 15.0, 27.0);
        let c = terminal_clutter_loss_db(2000.0, 1.5, 20.0, 27.0);
        assert!(a > 0.0 && b > a && c > b, "{a} {b} {c}");
        // ~ 15–25 dB is typical for a 1.5 m terminal in 15 m clutter at 2 GHz
        assert!(b > 12.0 && b < 30.0, "{b}");
    }
}
