//! Terrain and land-cover path profiles.
//!
//! Points are evenly spaced along the great circle (mean-radius sphere) from `a` to `b`,
//! endpoints included. With requested spacing `step_m`, `n = max(2, ceil(D / step_m) + 1)`
//! points are generated and the actual step is `D / (n - 1)`, so `n - 1` steps exactly span
//! the distance `D` (haversine meters).
//!
//! For hot loops (coverage maps calling this millions of times) use
//! [`terrain_profile_into`] / [`terrain_profile_n_into`], which reuse the caller's buffer
//! and perform no allocation; great-circle points are computed exactly every ≤500 m and
//! linearly interpolated in between (millimetre error, a few flops per point).

use serde::{Deserialize, Serialize};

use crate::geodesy::{central_angle_rad, from_unit, to_unit};
use crate::types::{LatLon, Raster, EARTH_RADIUS_M};

/// A sampled terrain profile.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Profile {
    /// Distance of each point from `a`, meters (0 … `distance_m`).
    pub distances_m: Vec<f64>,
    /// Bilinear terrain elevation at each point, meters (nodata filled, see below).
    pub elevations_m: Vec<f64>,
    pub points: Vec<LatLon>,
    /// Actual spacing between points, meters.
    pub step_m: f64,
    /// Total great-circle distance, meters.
    pub distance_m: f64,
    /// Number of points that had no DEM data (outside raster or nodata) and were filled
    /// by linear interpolation between the nearest valid neighbours (or copied from the
    /// nearest valid point at the ends).
    pub nodata_filled: usize,
    /// No point had valid data; all elevations were set to 0.
    pub all_nodata: bool,
}

/// Summary returned by the allocation-free profile functions.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct ProfileInfo {
    pub n: usize,
    pub step_m: f64,
    pub distance_m: f64,
    pub nodata_filled: usize,
    pub all_nodata: bool,
}

/// Number of profile points for a path of `distance_m` at requested spacing `step_m`.
#[inline]
pub fn point_count(distance_m: f64, step_m: f64) -> usize {
    if step_m.is_nan() || step_m <= 0.0 || !distance_m.is_finite() {
        return 2;
    }
    ((distance_m / step_m - 1e-9).ceil().max(1.0) as usize) + 1
}

/// Maximum spacing of exactly computed great-circle "knots"; points in between are
/// interpolated linearly in lat/lon. The deviation from the true great circle is
/// ≈ s²·tan(lat)/(8R) ≈ 5 mm·tan(lat) for s = 500 m — far below DEM resolution.
pub const GC_KNOT_SPACING_M: f64 = 500.0;

/// Call `f(i, point)` for `n >= 2` evenly spaced great-circle points from `a` to `b`
/// (endpoints exact). Exact slerp points are computed every ≤ [`GC_KNOT_SPACING_M`]
/// (at most every 64 points) and linearly interpolated in between, so the per-point cost
/// is a few multiply-adds.
#[inline]
pub fn for_each_gc_point(a: LatLon, b: LatLon, n: usize, mut f: impl FnMut(usize, LatLon)) {
    let n = n.max(2);
    let d = central_angle_rad(a, b);
    if d < 1e-12 {
        for i in 0..n {
            f(i, a);
        }
        return;
    }
    let va = to_unit(a);
    let vb = to_unit(b);
    let sd = d.sin();
    let last = n - 1;
    let knot = |i: usize| -> LatLon {
        if i == last {
            return b;
        }
        let t = i as f64 / last as f64;
        let ka = ((1.0 - t) * d).sin() / sd;
        let kb = (t * d).sin() / sd;
        from_unit([ka * va[0] + kb * vb[0], ka * va[1] + kb * vb[1], ka * va[2] + kb * vb[2]])
    };
    let step_m = d * EARTH_RADIUS_M / last as f64;
    let k = ((GC_KNOT_SPACING_M / step_m) as usize).clamp(1, 64);
    f(0, a);
    let (mut i0, mut p0) = (0usize, a);
    while i0 < last {
        let i1 = (i0 + k).min(last);
        let p1 = knot(i1);
        let dlat = p1.lat - p0.lat;
        let mut dlon = p1.lon - p0.lon;
        if dlon > 180.0 {
            dlon -= 360.0;
        } else if dlon < -180.0 {
            dlon += 360.0;
        }
        let inv = 1.0 / (i1 - i0) as f64;
        for j in 1..(i1 - i0) {
            let t = j as f64 * inv;
            let mut lon = p0.lon + dlon * t;
            if lon > 180.0 {
                lon -= 360.0;
            } else if lon < -180.0 {
                lon += 360.0;
            }
            f(i0 + j, LatLon::new(p0.lat + dlat * t, lon));
        }
        f(i1, p1);
        i0 = i1;
        p0 = p1;
    }
}

/// Evenly spaced great-circle points (allocates; see [`for_each_gc_point`]).
pub fn path_points(a: LatLon, b: LatLon, n: usize) -> Vec<LatLon> {
    let mut v = Vec::with_capacity(n.max(2));
    for_each_gc_point(a, b, n, |_, p| v.push(p));
    v
}

/// Replace NaNs by linear interpolation between valid neighbours (ends: nearest valid).
/// Returns (filled_count, all_nodata). If no value is valid, all become 0.
fn fill_nodata(v: &mut [f64]) -> (usize, bool) {
    let n = v.len();
    let mut filled = 0;
    let mut prev: Option<usize> = None;
    let mut i = 0;
    while i < n {
        if !v[i].is_nan() {
            prev = Some(i);
            i += 1;
            continue;
        }
        let start = i;
        while i < n && v[i].is_nan() {
            i += 1;
        }
        filled += i - start;
        match (prev, (i < n).then_some(i)) {
            (Some(p), Some(q)) => {
                let (vp, vq) = (v[p], v[q]);
                let span = (q - p) as f64;
                for (k, x) in v.iter_mut().enumerate().take(q).skip(start) {
                    *x = vp + (vq - vp) * (k - p) as f64 / span;
                }
            }
            (Some(p), None) => {
                let vp = v[p];
                v[start..].iter_mut().for_each(|x| *x = vp);
            }
            (None, Some(q)) => {
                let vq = v[q];
                v[..q].iter_mut().for_each(|x| *x = vq);
            }
            (None, None) => {
                v.iter_mut().for_each(|x| *x = 0.0);
                return (n, true);
            }
        }
    }
    (filled, false)
}

/// Sample exactly `n` (≥ 2) points from `a` to `b` into `out` (cleared first, capacity
/// reused). No allocation once `out` has grown to `n`.
pub fn terrain_profile_n_into(out: &mut Vec<f64>, dem: &Raster<f32>, a: LatLon, b: LatLon, n: usize) -> ProfileInfo {
    let n = n.max(2);
    out.clear();
    out.reserve(n);
    for_each_gc_point(a, b, n, |_, p| out.push(dem.sample_bilinear(p).map_or(f64::NAN, |v| v as f64)));
    let (nodata_filled, all_nodata) = fill_nodata(out);
    let distance_m = central_angle_rad(a, b) * EARTH_RADIUS_M;
    ProfileInfo { n, step_m: distance_m / (n - 1) as f64, distance_m, nodata_filled, all_nodata }
}

/// Allocation-free terrain profile at requested spacing `step_m` (see module docs).
pub fn terrain_profile_into(out: &mut Vec<f64>, dem: &Raster<f32>, a: LatLon, b: LatLon, step_m: f64) -> ProfileInfo {
    let d = central_angle_rad(a, b) * EARTH_RADIUS_M;
    terrain_profile_n_into(out, dem, a, b, point_count(d, step_m))
}

/// Full terrain profile with distances and point coordinates.
pub fn terrain_profile(dem: &Raster<f32>, a: LatLon, b: LatLon, step_m: f64) -> Profile {
    let distance_m = central_angle_rad(a, b) * EARTH_RADIUS_M;
    let n = point_count(distance_m, step_m);
    let points = path_points(a, b, n);
    let mut elevations_m: Vec<f64> =
        points.iter().map(|&p| dem.sample_bilinear(p).map_or(f64::NAN, |v| v as f64)).collect();
    let (nodata_filled, all_nodata) = fill_nodata(&mut elevations_m);
    let step = distance_m / (n - 1) as f64;
    let distances_m = (0..n).map(|i| if i == n - 1 { distance_m } else { i as f64 * step }).collect();
    Profile { distances_m, elevations_m, points, step_m: step, distance_m, nodata_filled, all_nodata }
}

/// Nearest-neighbour land-cover class at each point (0 = nodata / outside raster).
pub fn landcover_profile(lc: &Raster<u8>, points: &[LatLon]) -> Vec<u8> {
    points.iter().map(|&p| lc.sample_nearest(p).unwrap_or(0)).collect()
}

/// Allocation-free land-cover profile of `n` evenly spaced points from `a` to `b`
/// (same points as [`terrain_profile_n_into`] with the same `n`).
pub fn landcover_profile_n_into(out: &mut Vec<u8>, lc: &Raster<u8>, a: LatLon, b: LatLon, n: usize) {
    out.clear();
    out.reserve(n.max(2));
    for_each_gc_point(a, b, n, |_, p| out.push(lc.sample_nearest(p).unwrap_or(0)));
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geodesy::{haversine_m, intermediate_point};
    use crate::types::GeoTransform;

    fn plane_dem() -> Raster<f32> {
        // elevation = 1000 + 10000*(lon + 105) : linear in lon → bilinear exact
        let (w, h) = (200, 100);
        let geo = GeoTransform { origin_lon: -105.0, origin_lat: 40.1, px_lon: 0.001, px_lat: -0.001 };
        let mut data = vec![0f32; w * h];
        for y in 0..h {
            for x in 0..w {
                let lon = -105.0 + (x as f64 + 0.5) * 0.001;
                data[y * w + x] = (1000.0 + 10000.0 * (lon + 105.0)) as f32;
            }
        }
        Raster { width: w, height: h, data, geo, nodata: None }
    }

    #[test]
    fn counts_and_spacing() {
        assert_eq!(point_count(1000.0, 100.0), 11);
        assert_eq!(point_count(1001.0, 100.0), 12);
        assert_eq!(point_count(0.0, 100.0), 2);
        assert_eq!(point_count(50.0, 100.0), 2);
        let a = LatLon::new(40.05, -104.99);
        let b = LatLon::new(40.02, -104.81);
        let pts = path_points(a, b, 1001);
        let d = haversine_m(a, b);
        for (i, p) in pts.iter().enumerate() {
            let q = intermediate_point(a, b, i as f64 / 1000.0);
            assert!(haversine_m(*p, q) < 0.01, "point {i} off by {}", haversine_m(*p, q));
        }
        assert!((haversine_m(pts[0], pts[1]) - d / 1000.0).abs() < 1e-3);
        assert_eq!(pts[1000], b);
        // long path (Denver -> Chicago): interpolation error stays at millimetres
        let (a, b) = (LatLon::new(39.74, -104.99), LatLon::new(41.88, -87.63));
        let pts = path_points(a, b, 30_001);
        let worst = pts
            .iter()
            .enumerate()
            .map(|(i, p)| haversine_m(*p, intermediate_point(a, b, i as f64 / 30_000.0)))
            .fold(0.0, f64::max);
        assert!(worst < 0.02, "worst {worst}");
    }

    #[test]
    fn terrain_on_plane() {
        let dem = plane_dem();
        let a = LatLon::new(40.05, -104.99);
        let b = LatLon::new(40.05, -104.81);
        let p = terrain_profile(&dem, a, b, 30.0);
        let n = p.elevations_m.len();
        assert_eq!(n, p.points.len());
        assert_eq!(n, p.distances_m.len());
        assert!((p.distances_m[n - 1] - p.distance_m).abs() < 1e-9);
        assert!((p.step_m * (n - 1) as f64 - p.distance_m).abs() < 1e-6);
        assert!(p.step_m <= 30.0);
        assert!((p.elevations_m[0] - 1100.0).abs() < 0.01, "{}", p.elevations_m[0]);
        assert!((p.elevations_m[n - 1] - 2900.0).abs() < 0.01);
        assert_eq!(p.nodata_filled, 0);
        let mut buf = Vec::new();
        let info = terrain_profile_into(&mut buf, &dem, a, b, 30.0);
        assert_eq!(info.n, n);
        for (x, y) in buf.iter().zip(&p.elevations_m) {
            assert!((x - y).abs() < 1e-6);
        }
    }

    #[test]
    fn nodata_filling() {
        let mut v = vec![f64::NAN, 1.0, f64::NAN, f64::NAN, 4.0, f64::NAN];
        assert_eq!(fill_nodata(&mut v), (4, false));
        assert_eq!(v, vec![1.0, 1.0, 2.0, 3.0, 4.0, 4.0]);
        let mut v = vec![f64::NAN; 3];
        assert_eq!(fill_nodata(&mut v), (3, true));
        assert_eq!(v, vec![0.0; 3]);
        // path leaving the raster
        let dem = plane_dem();
        let p = terrain_profile(&dem, LatLon::new(40.05, -104.9), LatLon::new(40.05, -104.7), 100.0);
        assert!(p.nodata_filled > 0 && !p.all_nodata);
    }

    #[test]
    fn landcover() {
        let lc = Raster {
            width: 2,
            height: 1,
            data: vec![41u8, 82],
            geo: GeoTransform { origin_lon: 0.0, origin_lat: 1.0, px_lon: 1.0, px_lat: -1.0 },
            nodata: Some(0),
        };
        let (a, b) = (LatLon::new(0.5, 0.3), LatLon::new(0.5, 2.7));
        let pts = path_points(a, b, 4);
        assert_eq!(landcover_profile(&lc, &pts), vec![41, 82, 82, 0]);
        let mut buf = Vec::new();
        landcover_profile_n_into(&mut buf, &lc, a, b, 4);
        assert_eq!(buf, vec![41, 82, 82, 0]);
    }
}

#[cfg(test)]
mod bench {
    use super::*;
    use crate::types::GeoTransform;

    /// `cargo test -p pathless-data --release -- --ignored bench_profile --nocapture`
    #[test]
    #[ignore = "benchmark"]
    fn bench_profile() {
        let (w, h) = (2000, 2000);
        let data: Vec<f32> =
            (0..w * h).map(|i| ((i % w) as f32 * 0.37 + (i / w) as f32 * 0.11).sin() * 500.0 + 1500.0).collect();
        let dem = Raster {
            width: w,
            height: h,
            data,
            geo: GeoTransform { origin_lon: -105.5, origin_lat: 40.0, px_lon: 1e-4, px_lat: -1e-4 },
            nodata: None,
        };
        let a = LatLon::new(39.9, -105.4);
        let mut buf = Vec::new();
        let n_paths = 20_000;
        let t = std::time::Instant::now();
        let mut acc = 0.0;
        for i in 0..n_paths {
            let ang = i as f64 * 0.001;
            let b = LatLon::new(39.9 + 0.08 * ang.sin(), -105.4 + 0.08 * ang.cos());
            let info = terrain_profile_n_into(&mut buf, &dem, a, b, 300);
            acc += buf[info.n / 2];
        }
        let dt = t.elapsed();
        println!(
            "{n_paths} profiles x 300 pts in {dt:?} ({:.1} ns/point) [{acc:.0}]",
            dt.as_nanos() as f64 / (n_paths * 300) as f64
        );
    }
}
