//! Port of the shared definitions in `include/itm.h`: constants, the
//! `MIN`/`MAX`/`DIM` macros and C's `fdim`, plus the internal borrowed
//! terrain-profile view that replaces the C++ `pfl[]` array.
//!
//! The helpers deliberately reproduce the exact C semantics (including how
//! NaN propagates through the ternary-based macros) rather than using
//! `f64::min`/`f64::max`, so that the numerics match the reference code.

/// `PI` from itm.h (3.1415926535897932384 rounds to the same double).
pub(crate) const PI: f64 = std::f64::consts::PI;
/// `a_0__meter` from itm.h.
pub(crate) const A_0_METER: f64 = 6370e3;
/// `a_9000__meter` from itm.h.
pub(crate) const A_9000_METER: f64 = 9000e3;
/// `THIRD` from itm.h.
pub(crate) const THIRD: f64 = 1.0 / 3.0;

/// `MODE__P2P` / `MODE__AREA` from itm.h.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ItmMode {
    P2p,
    Area,
}

/// `#define MAX(x, y) (((x) > (y)) ? (x) : (y))`
#[inline(always)]
pub(crate) fn max(x: f64, y: f64) -> f64 {
    if x > y {
        x
    } else {
        y
    }
}

/// `#define MIN(x, y) (((x) < (y)) ? (x) : (y))`
#[inline(always)]
pub(crate) fn min(x: f64, y: f64) -> f64 {
    if x < y {
        x
    } else {
        y
    }
}

/// Integer flavour of `MAX`.
#[inline(always)]
pub(crate) fn max_i(x: i32, y: i32) -> i32 {
    if x > y {
        x
    } else {
        y
    }
}

/// Integer flavour of `MIN`.
#[inline(always)]
pub(crate) fn min_i(x: i32, y: i32) -> i32 {
    if x < y {
        x
    } else {
        y
    }
}

/// `#define DIM(x, y) (((x) > (y)) ? (x - y) : (0))`
#[inline(always)]
pub(crate) fn dim(x: f64, y: f64) -> f64 {
    if x > y {
        x - y
    } else {
        0.0
    }
}

/// C99 `fdim`: positive difference, NaN if either argument is NaN.
#[inline(always)]
pub(crate) fn fdim(x: f64, y: f64) -> f64 {
    if x.is_nan() || y.is_nan() {
        f64::NAN
    } else if x > y {
        x - y
    } else {
        0.0
    }
}

/// C++ `int(x)` / `(int)x` conversion (truncation toward zero).
///
/// Rust's `as` saturates on overflow and maps NaN to 0, where C++ is UB;
/// for all in-range values the results are identical.
#[inline(always)]
pub(crate) fn trunc_i(x: f64) -> i32 {
    x as i32
}

/// Borrowed view of an ITM terrain profile.
///
/// The C++ code uses `pfl = [np, xi, z_0, z_1, ..., z_np]`. Here
/// `pfl[0] == np`, `pfl[1] == xi` and `pfl[k + 2] == z[k]`.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Pfl<'a> {
    /// Number of profile intervals (number of points minus one), `pfl[0]`.
    pub np: usize,
    /// Spacing between points in meters, `pfl[1]`.
    pub xi: f64,
    /// Elevations; at least `np + 1` entries.
    pub z: &'a [f64],
}

impl Pfl<'_> {
    /// `pfl[0]` as a double.
    #[inline(always)]
    pub fn np_f(&self) -> f64 {
        self.np as f64
    }

    /// `int(pfl[0])`.
    #[inline(always)]
    pub fn np_i(&self) -> i32 {
        self.np as i32
    }
}
