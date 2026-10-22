//! Geodesy helpers.
//!
//! * Spherical (mean radius 6 371 008.8 m) great-circle functions: fast, used for
//!   profile point generation. Distance error vs. the WGS84 ellipsoid ≤ ~0.5 %.
//! * Ellipsoidal WGS84 geodesics (Karney's algorithm via `geographiclib-rs`): accurate to
//!   nanometers; use for reported path lengths/azimuths.
//!
//! Bearings are degrees clockwise from true north in `[0, 360)`.

use geographiclib_rs::{DirectGeodesic, Geodesic, InverseGeodesic};

use crate::types::{LatLon, EARTH_RADIUS_M};

/// Great-circle (haversine) distance in meters on the mean-radius sphere.
#[inline]
pub fn haversine_m(a: LatLon, b: LatLon) -> f64 {
    let (p1, p2) = (a.lat.to_radians(), b.lat.to_radians());
    let dp = p2 - p1;
    let dl = (b.lon - a.lon).to_radians();
    let h = (dp * 0.5).sin().powi(2) + p1.cos() * p2.cos() * (dl * 0.5).sin().powi(2);
    2.0 * EARTH_RADIUS_M * h.sqrt().min(1.0).asin()
}

/// Central angle between two points in radians (sphere).
#[inline]
pub fn central_angle_rad(a: LatLon, b: LatLon) -> f64 {
    haversine_m(a, b) / EARTH_RADIUS_M
}

/// Initial great-circle bearing from `a` to `b` (sphere), degrees `[0, 360)`.
pub fn initial_bearing_deg(a: LatLon, b: LatLon) -> f64 {
    let (p1, p2) = (a.lat.to_radians(), b.lat.to_radians());
    let dl = (b.lon - a.lon).to_radians();
    let y = dl.sin() * p2.cos();
    let x = p1.cos() * p2.sin() - p1.sin() * p2.cos() * dl.cos();
    normalize_bearing(y.atan2(x).to_degrees())
}

/// Point reached travelling `distance_m` from `start` on initial bearing `bearing_deg`
/// along a great circle (sphere).
pub fn destination(start: LatLon, bearing_deg: f64, distance_m: f64) -> LatLon {
    let d = distance_m / EARTH_RADIUS_M;
    let th = bearing_deg.to_radians();
    let p1 = start.lat.to_radians();
    let l1 = start.lon.to_radians();
    let p2 = (p1.sin() * d.cos() + p1.cos() * d.sin() * th.cos()).clamp(-1.0, 1.0).asin();
    let l2 = l1 + (th.sin() * d.sin() * p1.cos()).atan2(d.cos() - p1.sin() * p2.sin());
    LatLon::new(p2.to_degrees(), normalize_lon(l2.to_degrees()))
}

/// Point at `fraction` (0 = a, 1 = b) along the great circle from `a` to `b` (sphere).
pub fn intermediate_point(a: LatLon, b: LatLon, fraction: f64) -> LatLon {
    let d = central_angle_rad(a, b);
    if d < 1e-12 {
        return a;
    }
    let va = to_unit(a);
    let vb = to_unit(b);
    let s = d.sin();
    let ka = ((1.0 - fraction) * d).sin() / s;
    let kb = (fraction * d).sin() / s;
    from_unit([ka * va[0] + kb * vb[0], ka * va[1] + kb * vb[1], ka * va[2] + kb * vb[2]])
}

/// Result of an ellipsoidal inverse problem.
#[derive(Debug, Clone, Copy, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct GeodesicInverse {
    /// Distance in meters on WGS84.
    pub distance_m: f64,
    /// Forward azimuth at the start point, degrees `[0, 360)`.
    pub azimuth1_deg: f64,
    /// Forward azimuth at the end point, degrees `[0, 360)` (back azimuth = +180°).
    pub azimuth2_deg: f64,
}

/// WGS84 geodesic distance and azimuths (Karney).
pub fn geodesic_inverse(a: LatLon, b: LatLon) -> GeodesicInverse {
    let g = Geodesic::wgs84();
    let (s12, azi1, azi2, _a12): (f64, f64, f64, f64) = g.inverse(a.lat, a.lon, b.lat, b.lon);
    GeodesicInverse { distance_m: s12, azimuth1_deg: normalize_bearing(azi1), azimuth2_deg: normalize_bearing(azi2) }
}

/// WGS84 geodesic distance in meters (Karney).
pub fn geodesic_distance_m(a: LatLon, b: LatLon) -> f64 {
    let g = Geodesic::wgs84();
    let s12: f64 = g.inverse(a.lat, a.lon, b.lat, b.lon);
    s12
}

/// WGS84 direct problem: point at `distance_m` from `start` along azimuth `bearing_deg`.
pub fn geodesic_destination(start: LatLon, bearing_deg: f64, distance_m: f64) -> LatLon {
    let g = Geodesic::wgs84();
    let (lat, lon): (f64, f64) = g.direct(start.lat, start.lon, bearing_deg, distance_m);
    LatLon::new(lat, normalize_lon(lon))
}

#[inline]
pub fn normalize_bearing(deg: f64) -> f64 {
    let b = deg.rem_euclid(360.0);
    if b >= 360.0 {
        0.0
    } else {
        b
    }
}

#[inline]
pub fn normalize_lon(deg: f64) -> f64 {
    let l = (deg + 180.0).rem_euclid(360.0) - 180.0;
    if l == -180.0 && deg > 0.0 {
        180.0
    } else {
        l
    }
}

#[inline]
pub(crate) fn to_unit(p: LatLon) -> [f64; 3] {
    let (sp, cp) = p.lat.to_radians().sin_cos();
    let (sl, cl) = p.lon.to_radians().sin_cos();
    [cp * cl, cp * sl, sp]
}

#[inline]
pub(crate) fn from_unit(v: [f64; 3]) -> LatLon {
    LatLon::new(v[2].atan2(v[0].hypot(v[1])).to_degrees(), v[1].atan2(v[0]).to_degrees())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn spherical() {
        // Boulder -> Denver ~ 39 km
        let boulder = LatLon::new(40.015, -105.2705);
        let denver = LatLon::new(39.7392, -104.9903);
        let d = haversine_m(boulder, denver);
        assert!((d - 38_700.0).abs() < 500.0, "{d}");
        let brg = initial_bearing_deg(boulder, denver);
        assert!(brg > 130.0 && brg < 150.0, "{brg}");
        let p = destination(boulder, brg, d);
        assert!(haversine_m(p, denver) < 0.01);
        let mid = intermediate_point(boulder, denver, 0.5);
        assert!((haversine_m(boulder, mid) - d / 2.0).abs() < 1e-3);
        assert_eq!(intermediate_point(boulder, denver, 0.0), boulder);
        // quarter of the equator
        let q = haversine_m(LatLon::new(0.0, 0.0), LatLon::new(0.0, 90.0));
        assert!((q - EARTH_RADIUS_M * std::f64::consts::FRAC_PI_2).abs() < 1e-6);
        assert!((normalize_lon(190.0) - -170.0).abs() < 1e-12);
    }

    #[test]
    fn ellipsoidal() {
        // 1 degree of latitude at the equator on WGS84 = 110 574.389 m
        let d = geodesic_distance_m(LatLon::new(0.0, 0.0), LatLon::new(1.0, 0.0));
        assert!((d - 110_574.389).abs() < 0.01, "{d}");
        let inv = geodesic_inverse(LatLon::new(40.0, -105.0), LatLon::new(40.0, -104.0));
        assert!((inv.azimuth1_deg - 89.68).abs() < 0.05, "{inv:?}");
        let p = geodesic_destination(LatLon::new(40.0, -105.0), inv.azimuth1_deg, inv.distance_m);
        assert!((p.lat - 40.0).abs() < 1e-9 && (p.lon - -104.0).abs() < 1e-9);
    }
}
