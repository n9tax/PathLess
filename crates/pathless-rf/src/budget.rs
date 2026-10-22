//! Link budget and field-strength conversions.
//!
//! Field strength relations (far field, matched polarization):
//!
//! * Received power ↔ field strength: `E(dBµV/m) = Pr(dBm) − Gr(dBi) + 20·log10 f(MHz) + 77.2`
//! * Field strength from EIRP in free space (ITU-R P.525):
//!   `E(dBµV/m) = EIRP(dBm) − 20·log10 d(km) + 44.77` (= `EIRP(dBW) + 74.77 − 20 log d`)
//! * Broadcast ERP is referenced to a half-wave dipole: `EIRP = ERP + 2.15 dB`.

use serde::{Deserialize, Serialize};

use crate::basic::{dbm_to_watts, DIPOLE_GAIN_DBI};

/// Constant of the Pr ↔ E conversion for Pr in dBm, G in dBi, f in MHz and E in dBµV/m:
/// 77.2 dB (exactly 77.21, from Pr = E²/(120π)·λ²G/(4π)).
pub const FIELD_STRENGTH_CONST_DB: f64 = 77.2;

/// Constant of the free-space field strength from EIRP (dBm, km → dBµV/m): 44.77 dB
/// (74.77 dB for EIRP in dBW; `E = √(30·P)/d`).
pub const FREE_SPACE_FIELD_CONST_DBM_KM: f64 = 44.77;

/// Inputs of a point-to-point link budget. All values in dB/dBm/dBi.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct LinkBudgetInput {
    /// Transmitter output power (dBm).
    pub tx_power_dbm: f64,
    /// Transmit feeder / connector / branching losses (dB, positive).
    pub tx_line_loss_db: f64,
    /// Transmit antenna gain toward the receiver (dBi).
    pub tx_antenna_gain_dbi: f64,
    /// Receive antenna gain toward the transmitter (dBi).
    pub rx_antenna_gain_dbi: f64,
    /// Receive feeder losses (dB, positive).
    pub rx_line_loss_db: f64,
    /// Any other losses: radome, gases, misc. (dB, positive).
    pub other_losses_db: f64,
    /// Basic transmission loss: free space + diffraction/ITM etc. (dB).
    pub path_loss_db: f64,
    /// Receiver sensitivity / threshold (dBm).
    pub rx_sensitivity_dbm: f64,
}

/// Result of [`link_budget`].
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct LinkBudgetResult {
    /// EIRP (dBm) = Ptx − Ltx + Gtx.
    pub eirp_dbm: f64,
    /// ERP (dBm) = EIRP − 2.15.
    pub erp_dbm: f64,
    /// EIRP (W).
    pub eirp_watts: f64,
    /// Isotropic received level at the receive antenna (dBm), before Grx and Lrx.
    pub isotropic_rx_level_dbm: f64,
    /// Received signal level at the receiver input (dBm).
    pub rx_level_dbm: f64,
    /// Fade margin = rx level − sensitivity (dB).
    pub fade_margin_db: f64,
    /// Sum of all gains (dB).
    pub total_gain_db: f64,
    /// Sum of all losses including path loss (dB).
    pub total_loss_db: f64,
    /// Net system gain: Ptx − sensitivity + gains − line losses (dB): the maximum
    /// tolerable path + other loss for 0 dB margin.
    pub system_gain_db: f64,
}

/// Evaluate a link budget.
pub fn link_budget(i: &LinkBudgetInput) -> LinkBudgetResult {
    let eirp = i.tx_power_dbm - i.tx_line_loss_db + i.tx_antenna_gain_dbi;
    let iso = eirp - i.path_loss_db - i.other_losses_db;
    let rx = iso + i.rx_antenna_gain_dbi - i.rx_line_loss_db;
    let gains = i.tx_antenna_gain_dbi + i.rx_antenna_gain_dbi;
    let losses = i.tx_line_loss_db + i.rx_line_loss_db + i.other_losses_db + i.path_loss_db;
    LinkBudgetResult {
        eirp_dbm: eirp,
        erp_dbm: eirp - DIPOLE_GAIN_DBI,
        eirp_watts: dbm_to_watts(eirp),
        isotropic_rx_level_dbm: iso,
        rx_level_dbm: rx,
        fade_margin_db: rx - i.rx_sensitivity_dbm,
        total_gain_db: gains,
        total_loss_db: losses,
        system_gain_db: i.tx_power_dbm - i.rx_sensitivity_dbm + gains - i.tx_line_loss_db - i.rx_line_loss_db,
    }
}

/// Field strength (dBµV/m) corresponding to a received power `pr_dbm` at the terminals of
/// an antenna of gain `rx_gain_dbi` at `f_mhz`:
/// `E = Pr − G + 20·log10 f + 77.2`.
pub fn field_strength_dbuv_m_from_dbm(pr_dbm: f64, f_mhz: f64, rx_gain_dbi: f64) -> f64 {
    pr_dbm - rx_gain_dbi + 20.0 * f_mhz.log10() + FIELD_STRENGTH_CONST_DB
}

/// Received power (dBm) at the terminals of an antenna of gain `rx_gain_dbi` in a field of
/// `e_dbuv_m`: `Pr = E + G − 20·log10 f − 77.2`.
pub fn dbm_from_field_strength_dbuv_m(e_dbuv_m: f64, f_mhz: f64, rx_gain_dbi: f64) -> f64 {
    e_dbuv_m + rx_gain_dbi - 20.0 * f_mhz.log10() - FIELD_STRENGTH_CONST_DB
}

/// Free-space field strength (dBµV/m) at `d_km` from a transmitter of `eirp_dbm`.
pub fn free_space_field_strength_dbuv_m(eirp_dbm: f64, d_km: f64) -> f64 {
    eirp_dbm - 20.0 * d_km.log10() + FREE_SPACE_FIELD_CONST_DBM_KM
}

/// Field strength (dBµV/m) given EIRP (dBm), frequency and a basic transmission loss
/// (dB, e.g. from ITM / P.526): `E = EIRP − Lb + 20·log10 f + 77.2` (isotropic receive
/// antenna).
pub fn field_strength_from_path_loss(eirp_dbm: f64, path_loss_db: f64, f_mhz: f64) -> f64 {
    field_strength_dbuv_m_from_dbm(eirp_dbm - path_loss_db, f_mhz, 0.0)
}

/// Field strength (dBµV/m) given broadcast ERP (dBm), frequency and basic transmission loss.
pub fn field_strength_from_erp(erp_dbm: f64, path_loss_db: f64, f_mhz: f64) -> f64 {
    field_strength_from_path_loss(erp_dbm + DIPOLE_GAIN_DBI, path_loss_db, f_mhz)
}

/// Field strength dBµV/m → V/m.
pub fn dbuv_m_to_v_m(e_dbuv_m: f64) -> f64 {
    10f64.powf(e_dbuv_m / 20.0) * 1e-6
}

/// Field strength V/m → dBµV/m.
pub fn v_m_to_dbuv_m(e_v_m: f64) -> f64 {
    20.0 * (e_v_m * 1e6).log10()
}

/// Power (dBm) into a 50 Ω load → voltage dBµV: `dBµV = dBm + 107` (50 Ω, 106.99).
pub fn dbm_to_dbuv_50ohm(dbm: f64) -> f64 {
    dbm + 106.9897
}

/// Voltage dBµV across 50 Ω → power dBm.
pub fn dbuv_to_dbm_50ohm(dbuv: f64) -> f64 {
    dbuv - 106.9897
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::basic::{fspl_db, wavelength_m};

    fn close(a: f64, b: f64, tol: f64) -> bool {
        (a - b).abs() <= tol
    }

    #[test]
    fn budget() {
        let i = LinkBudgetInput {
            tx_power_dbm: 25.0,
            tx_line_loss_db: 1.5,
            tx_antenna_gain_dbi: 30.0,
            rx_antenna_gain_dbi: 30.0,
            rx_line_loss_db: 1.5,
            other_losses_db: 1.0,
            path_loss_db: 140.0,
            rx_sensitivity_dbm: -80.0,
        };
        let r = link_budget(&i);
        assert!(close(r.eirp_dbm, 53.5, 1e-12));
        assert!(close(r.erp_dbm, 51.35, 1e-12));
        assert!(close(r.rx_level_dbm, -59.0, 1e-12));
        assert!(close(r.fade_margin_db, 21.0, 1e-12));
        assert!(close(r.system_gain_db - r.fade_margin_db, i.path_loss_db + i.other_losses_db, 1e-12));
        assert!(close(r.eirp_watts, 223.872, 1e-3));
    }

    #[test]
    fn field_strength_consistency() {
        // Free-space: E from EIRP must match Pr/E conversion through FSPL.
        let f = 100.0;
        let d = 10.0;
        let eirp = 60.0; // dBm (1 kW)
        let pr_iso = eirp - fspl_db(d, f);
        let e1 = field_strength_dbuv_m_from_dbm(pr_iso, f, 0.0);
        let e2 = free_space_field_strength_dbuv_m(eirp, d);
        assert!(close(e1, e2, 0.05), "{e1} vs {e2}");
        // 1 kW EIRP at 1 km: E = sqrt(30·1000)/1000 V/m = 0.1732 V/m = 104.77 dBµV/m
        assert!(close(free_space_field_strength_dbuv_m(60.0, 1.0), v_m_to_dbuv_m(0.173205), 0.01));
        assert!(close(free_space_field_strength_dbuv_m(60.0, 1.0), 104.77, 0.01));
        // round trip
        let p = dbm_from_field_strength_dbuv_m(e1, f, 2.15);
        assert!(close(field_strength_dbuv_m_from_dbm(p, f, 2.15), e1, 1e-12));
        assert!(close(dbuv_m_to_v_m(120.0), 1.0, 1e-12));
        assert!(close(dbuv_to_dbm_50ohm(dbm_to_dbuv_50ohm(-90.0)), -90.0, 1e-12));
        // Physics check of the 77.2 constant: Pr = E²/(120π)·λ²G/(4π)
        let e_v = 1e-3; // 60 dBµV/m
        let pr_w = e_v * e_v / (120.0 * std::f64::consts::PI) * wavelength_m(f).powi(2) / (4.0 * std::f64::consts::PI);
        let pr_dbm = 10.0 * pr_w.log10() + 30.0;
        assert!(close(field_strength_dbuv_m_from_dbm(pr_dbm, f, 0.0), 60.0, 0.02));
        assert!(close(
            field_strength_from_erp(50.0, 100.0, 98.0) - field_strength_from_path_loss(50.0, 100.0, 98.0),
            2.15,
            1e-12
        ));
    }
}
