//! Port of `include/Warnings.h`: ITM warning bit flags.
//!
//! Warnings never stop the computation; they flag that an input or an
//! internally derived quantity is near / outside the range over which the
//! model was validated. Test with `output.warnings & FLAG != 0`.

/// No warning flags.
pub const NO_WARNINGS: u32 = 0x0000;
/// TX terminal height is near its limits (outside 1..=1000 m).
pub const TX_TERMINAL_HEIGHT: u32 = 0x0001;
/// RX terminal height is near its limits (outside 1..=1000 m).
pub const RX_TERMINAL_HEIGHT: u32 = 0x0002;
/// Frequency is near its limits (outside 40..=10000 MHz).
pub const FREQUENCY: u32 = 0x0004;
/// Path distance is near its upper limit (> 1000 km).
pub const PATH_DISTANCE_TOO_BIG_1: u32 = 0x0008;
/// Path distance is large (> 2000 km) - care must be taken with result.
pub const PATH_DISTANCE_TOO_BIG_2: u32 = 0x0010;
/// Path distance is near its lower limit (< |h_e_tx - h_e_rx| / 0.2).
pub const PATH_DISTANCE_TOO_SMALL_1: u32 = 0x0020;
/// Path distance is small (< 1 km) - care must be taken with result.
pub const PATH_DISTANCE_TOO_SMALL_2: u32 = 0x0040;
/// TX horizon angle is large (> 200 mrad) - small angle approximations could break down.
pub const TX_HORIZON_ANGLE: u32 = 0x0080;
/// RX horizon angle is large (> 200 mrad) - small angle approximations could break down.
pub const RX_HORIZON_ANGLE: u32 = 0x0100;
/// TX horizon distance is less than 1/10 of the smooth earth horizon distance.
pub const TX_HORIZON_DISTANCE_1: u32 = 0x0200;
/// RX horizon distance is less than 1/10 of the smooth earth horizon distance.
pub const RX_HORIZON_DISTANCE_1: u32 = 0x0400;
/// TX horizon distance is greater than 3 times the smooth earth horizon distance.
pub const TX_HORIZON_DISTANCE_2: u32 = 0x0800;
/// RX horizon distance is greater than 3 times the smooth earth horizon distance.
pub const RX_HORIZON_DISTANCE_2: u32 = 0x1000;
/// One of the provided variabilities is located far in the tail of its
/// distribution (|z| > 3.10, i.e. roughly < 0.1 % or > 99.9 %).
pub const EXTREME_VARIABILITIES: u32 = 0x2000;
/// Internally computed surface refractivity value is small (150 <= N_s < 250)
/// - care must be taken with result.
pub const SURFACE_REFRACTIVITY: u32 = 0x4000;

const ALL: [(u32, &str); 15] = [
    (TX_TERMINAL_HEIGHT, "TX terminal height is near its limits"),
    (RX_TERMINAL_HEIGHT, "RX terminal height is near its limits"),
    (FREQUENCY, "Frequency is near its limits"),
    (PATH_DISTANCE_TOO_BIG_1, "Path distance is near its upper limit"),
    (PATH_DISTANCE_TOO_BIG_2, "Path distance is large - care must be taken with result"),
    (PATH_DISTANCE_TOO_SMALL_1, "Path distance is near its lower limit"),
    (PATH_DISTANCE_TOO_SMALL_2, "Path distance is small - care must be taken with result"),
    (TX_HORIZON_ANGLE, "TX horizon angle is large - small angle approximations could break down"),
    (RX_HORIZON_ANGLE, "RX horizon angle is large - small angle approximations could break down"),
    (TX_HORIZON_DISTANCE_1, "TX horizon distance is less than 1/10 of the smooth earth horizon distance"),
    (RX_HORIZON_DISTANCE_1, "RX horizon distance is less than 1/10 of the smooth earth horizon distance"),
    (TX_HORIZON_DISTANCE_2, "TX horizon distance is greater than 3 times the smooth earth horizon distance"),
    (RX_HORIZON_DISTANCE_2, "RX horizon distance is greater than 3 times the smooth earth horizon distance"),
    (EXTREME_VARIABILITIES, "One of the provided variabilities is located far in the tail of its distribution"),
    (SURFACE_REFRACTIVITY, "Internally computed surface refractivity value is small - care must be taken with result"),
];

/// Iterate over human-readable descriptions of every flag set in `warnings`.
pub fn describe(warnings: u32) -> impl Iterator<Item = &'static str> {
    ALL.iter().filter(move |(bit, _)| warnings & bit != 0).map(|(_, s)| *s)
}
