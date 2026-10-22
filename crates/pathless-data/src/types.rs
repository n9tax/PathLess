//! Core geographic and raster types (all coordinates are WGS84 / EPSG:4326 degrees).

use serde::{Deserialize, Serialize};

/// Mean Earth radius (IUGG R1) in meters, used for local metric conversions.
pub const EARTH_RADIUS_M: f64 = 6_371_008.8;

/// A geographic point in decimal degrees (WGS84).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct LatLon {
    pub lat: f64,
    pub lon: f64,
}

impl LatLon {
    pub const fn new(lat: f64, lon: f64) -> Self {
        Self { lat, lon }
    }
}

/// Axis-aligned geographic bounding box in degrees.
///
/// Antimeridian-crossing boxes are not supported (`min_lon <= max_lon` is assumed).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct BBox {
    pub min_lon: f64,
    pub min_lat: f64,
    pub max_lon: f64,
    pub max_lat: f64,
}

impl BBox {
    pub const fn new(min_lon: f64, min_lat: f64, max_lon: f64, max_lat: f64) -> Self {
        Self { min_lon, min_lat, max_lon, max_lat }
    }

    /// Smallest box containing all polygon vertices. Returns `None` for an empty slice
    /// or if any vertex is non-finite.
    pub fn from_polygon(points: &[LatLon]) -> Option<Self> {
        let first = points.first()?;
        let mut b = BBox::new(first.lon, first.lat, first.lon, first.lat);
        for p in points {
            if !p.lat.is_finite() || !p.lon.is_finite() {
                return None;
            }
            b.min_lon = b.min_lon.min(p.lon);
            b.max_lon = b.max_lon.max(p.lon);
            b.min_lat = b.min_lat.min(p.lat);
            b.max_lat = b.max_lat.max(p.lat);
        }
        Some(b)
    }

    /// Grow the box by `meters` on every side (spherical approximation; the longitude
    /// margin uses the latitude closest to the pole so the margin is never too small).
    /// Result is clamped to valid lat/lon ranges.
    pub fn expand_m(&self, meters: f64) -> Self {
        let dlat = (meters / EARTH_RADIUS_M).to_degrees();
        let worst_lat = self.min_lat.abs().max(self.max_lat.abs()).min(89.0);
        let dlon = dlat / worst_lat.to_radians().cos();
        BBox {
            min_lon: (self.min_lon - dlon).max(-180.0),
            min_lat: (self.min_lat - dlat).max(-90.0),
            max_lon: (self.max_lon + dlon).min(180.0),
            max_lat: (self.max_lat + dlat).min(90.0),
        }
    }

    /// Grow the box by `deg` degrees on every side (clamped).
    pub fn expand_deg(&self, deg: f64) -> Self {
        BBox {
            min_lon: (self.min_lon - deg).max(-180.0),
            min_lat: (self.min_lat - deg).max(-90.0),
            max_lon: (self.max_lon + deg).min(180.0),
            max_lat: (self.max_lat + deg).min(90.0),
        }
    }

    pub fn contains(&self, p: LatLon) -> bool {
        p.lon >= self.min_lon && p.lon <= self.max_lon && p.lat >= self.min_lat && p.lat <= self.max_lat
    }

    /// True if the boxes overlap with positive area.
    pub fn intersects(&self, o: &BBox) -> bool {
        self.min_lon < o.max_lon && o.min_lon < self.max_lon && self.min_lat < o.max_lat && o.min_lat < self.max_lat
    }

    pub fn intersection(&self, o: &BBox) -> Option<BBox> {
        if !self.intersects(o) {
            return None;
        }
        Some(BBox {
            min_lon: self.min_lon.max(o.min_lon),
            min_lat: self.min_lat.max(o.min_lat),
            max_lon: self.max_lon.min(o.max_lon),
            max_lat: self.max_lat.min(o.max_lat),
        })
    }

    pub fn width_deg(&self) -> f64 {
        self.max_lon - self.min_lon
    }

    pub fn height_deg(&self) -> f64 {
        self.max_lat - self.min_lat
    }

    pub fn center(&self) -> LatLon {
        LatLon::new((self.min_lat + self.max_lat) * 0.5, (self.min_lon + self.max_lon) * 0.5)
    }

    /// Area on the sphere in km² (exact for a lat/lon rectangle on a sphere).
    pub fn area_km2(&self) -> f64 {
        let r_km = EARTH_RADIUS_M / 1000.0;
        let dlon = self.width_deg().max(0.0).to_radians();
        let s = self.max_lat.to_radians().sin() - self.min_lat.to_radians().sin();
        (r_km * r_km * dlon * s).abs()
    }

    /// True if all coordinates are finite, ordered and within valid ranges.
    pub fn is_valid(&self) -> bool {
        [self.min_lon, self.min_lat, self.max_lon, self.max_lat].iter().all(|v| v.is_finite())
            && self.min_lon < self.max_lon
            && self.min_lat < self.max_lat
            && self.min_lon >= -180.0
            && self.max_lon <= 180.0
            && self.min_lat >= -90.0
            && self.max_lat <= 90.0
    }
}

/// Affine (north-up, no rotation) georeference of a raster in EPSG:4326.
///
/// Pixel `(x, y)` covers lon `[origin_lon + x*px_lon, origin_lon + (x+1)*px_lon]` and
/// lat `[origin_lat + (y+1)*px_lat, origin_lat + y*px_lat]` (pixel-is-area); its center is
/// at `x + 0.5, y + 0.5`.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct GeoTransform {
    /// Longitude of the left edge of the top-left pixel.
    pub origin_lon: f64,
    /// Latitude of the top edge of the top-left pixel.
    pub origin_lat: f64,
    /// Pixel width in degrees (positive).
    pub px_lon: f64,
    /// Pixel height in degrees (negative for north-up rasters).
    pub px_lat: f64,
}

impl GeoTransform {
    /// Continuous pixel coordinates (0,0 = top-left corner of top-left pixel).
    #[inline]
    pub fn to_pixel(&self, p: LatLon) -> (f64, f64) {
        ((p.lon - self.origin_lon) / self.px_lon, (p.lat - self.origin_lat) / self.px_lat)
    }

    /// Geographic location of continuous pixel coordinates.
    #[inline]
    pub fn to_geo(&self, x: f64, y: f64) -> LatLon {
        LatLon::new(self.origin_lat + y * self.px_lat, self.origin_lon + x * self.px_lon)
    }
}

/// A single-band north-up raster in EPSG:4326, row-major from the top-left.
///
/// For `Raster<f32>` (elevation) `NaN` always means "no data" in addition to `nodata`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Raster<T> {
    pub width: usize,
    pub height: usize,
    pub data: Vec<T>,
    pub geo: GeoTransform,
    pub nodata: Option<T>,
}

impl<T: Copy> Raster<T> {
    /// Create a raster filled with `fill`.
    pub fn filled(width: usize, height: usize, geo: GeoTransform, fill: T, nodata: Option<T>) -> Self {
        Self { width, height, data: vec![fill; width * height], geo, nodata }
    }

    /// Value at pixel `(x, y)`, or `None` if out of bounds.
    #[inline]
    pub fn get(&self, x: usize, y: usize) -> Option<T> {
        if x < self.width && y < self.height {
            Some(self.data[y * self.width + x])
        } else {
            None
        }
    }

    /// Geographic extent covered by the raster (outer pixel edges).
    pub fn bbox(&self) -> BBox {
        let x1 = self.geo.origin_lon + self.width as f64 * self.geo.px_lon;
        let y1 = self.geo.origin_lat + self.height as f64 * self.geo.px_lat;
        BBox {
            min_lon: self.geo.origin_lon.min(x1),
            max_lon: self.geo.origin_lon.max(x1),
            min_lat: self.geo.origin_lat.min(y1),
            max_lat: self.geo.origin_lat.max(y1),
        }
    }

    /// Value of the pixel containing `p`, or `None` if outside the raster.
    /// Nodata pixels are returned as-is (compare with `nodata` if needed).
    #[inline]
    pub fn sample_nearest(&self, p: LatLon) -> Option<T> {
        let (fx, fy) = self.geo.to_pixel(p);
        if !(fx >= 0.0 && fy >= 0.0) {
            return None;
        }
        let (x, y) = (fx as usize, fy as usize);
        // Points exactly on the far edge belong to the last pixel.
        let x = if x == self.width && fx <= self.width as f64 { x.wrapping_sub(1) } else { x };
        let y = if y == self.height && fy <= self.height as f64 { y.wrapping_sub(1) } else { y };
        self.get(x, y)
    }
}

impl Raster<f32> {
    #[inline]
    fn valid(&self, v: f32) -> bool {
        !v.is_nan() && self.nodata != Some(v)
    }

    /// True if `v` is a nodata value for this raster (NaN or equal to `nodata`).
    #[inline]
    pub fn is_nodata(&self, v: f32) -> bool {
        !self.valid(v)
    }

    /// Bilinear interpolation between the four surrounding pixel centers.
    ///
    /// Near the border the nearest edge pixels are used (clamped). If some of the four
    /// neighbours are nodata, the valid ones are re-weighted; returns `None` if all four
    /// are nodata or `p` is outside the raster extent.
    #[inline]
    pub fn sample_bilinear(&self, p: LatLon) -> Option<f32> {
        let (fx, fy) = self.geo.to_pixel(p);
        let (w, h) = (self.width as f64, self.height as f64);
        if !(fx >= 0.0 && fy >= 0.0 && fx <= w && fy <= h) || self.width == 0 || self.height == 0 {
            return None;
        }
        // Shift to pixel-center coordinates and clamp.
        let cx = (fx - 0.5).clamp(0.0, w - 1.0);
        let cy = (fy - 0.5).clamp(0.0, h - 1.0);
        let x0 = cx as usize;
        let y0 = cy as usize;
        let x1 = (x0 + 1).min(self.width - 1);
        let y1 = (y0 + 1).min(self.height - 1);
        let tx = cx - x0 as f64;
        let ty = cy - y0 as f64;
        let row0 = y0 * self.width;
        let row1 = y1 * self.width;
        let v00 = self.data[row0 + x0];
        let v10 = self.data[row0 + x1];
        let v01 = self.data[row1 + x0];
        let v11 = self.data[row1 + x1];
        let w00 = (1.0 - tx) * (1.0 - ty);
        let w10 = tx * (1.0 - ty);
        let w01 = (1.0 - tx) * ty;
        let w11 = tx * ty;
        if self.valid(v00) && self.valid(v10) && self.valid(v01) && self.valid(v11) {
            return Some((v00 as f64 * w00 + v10 as f64 * w10 + v01 as f64 * w01 + v11 as f64 * w11) as f32);
        }
        let mut sum = 0.0;
        let mut wsum = 0.0;
        for (v, wt) in [(v00, w00), (v10, w10), (v01, w01), (v11, w11)] {
            if self.valid(v) {
                sum += v as f64 * wt;
                wsum += wt;
            }
        }
        if wsum > 1e-12 {
            Some((sum / wsum) as f32)
        } else {
            // Valid pixels exist but with zero weight (exactly on a center): fall back.
            [v00, v10, v01, v11].into_iter().find(|&v| self.valid(v))
        }
    }

    /// (min, max) of valid values, or `None` if the raster has no valid data.
    pub fn min_max(&self) -> Option<(f32, f32)> {
        let mut it = self.data.iter().copied().filter(|&v| self.valid(v));
        let first = it.next()?;
        Some(it.fold((first, first), |(lo, hi), v| (lo.min(v), hi.max(v))))
    }

    /// Number of nodata pixels.
    pub fn nodata_count(&self) -> usize {
        self.data.iter().filter(|&&v| !self.valid(v)).count()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ramp() -> Raster<f32> {
        // 3x2 raster, 1 degree pixels, origin (0, 2)
        Raster {
            width: 3,
            height: 2,
            data: vec![0.0, 1.0, 2.0, 10.0, 11.0, 12.0],
            geo: GeoTransform { origin_lon: 0.0, origin_lat: 2.0, px_lon: 1.0, px_lat: -1.0 },
            nodata: None,
        }
    }

    #[test]
    fn bbox_helpers() {
        let b =
            BBox::from_polygon(&[LatLon::new(40.0, -105.3), LatLon::new(39.95, -105.25), LatLon::new(39.97, -105.28)])
                .unwrap();
        assert_eq!(b, BBox::new(-105.3, 39.95, -105.25, 40.0));
        assert!(b.contains(LatLon::new(39.97, -105.27)));
        assert!(!b.contains(LatLon::new(39.0, -105.27)));
        let e = b.expand_m(1000.0);
        assert!((e.max_lat - 40.0 - 0.008993).abs() < 1e-5);
        assert!(e.min_lon < -105.3 - 0.0117);
        assert!(b.intersects(&e));
        assert!(!b.intersects(&BBox::new(0.0, 0.0, 1.0, 1.0)));
        // 1x1 degree at equator ≈ 12364 km²
        let a = BBox::new(0.0, 0.0, 1.0, 1.0).area_km2();
        assert!((a - 12364.0).abs() < 10.0, "{a}");
    }

    #[test]
    fn sampling() {
        let r = ramp();
        assert_eq!(r.sample_nearest(LatLon::new(1.5, 0.5)), Some(0.0));
        assert_eq!(r.sample_nearest(LatLon::new(0.5, 2.5)), Some(12.0));
        assert_eq!(r.sample_nearest(LatLon::new(0.0, 3.0)), Some(12.0));
        assert_eq!(r.sample_nearest(LatLon::new(0.5, 3.5)), None);
        // center of pixel (1,0)
        assert_eq!(r.sample_bilinear(LatLon::new(1.5, 1.5)), Some(1.0));
        // halfway between pixel centers (0,0),(1,0),(0,1),(1,1)
        let v = r.sample_bilinear(LatLon::new(1.0, 1.0)).unwrap();
        assert!((v - 5.5).abs() < 1e-6);
        assert_eq!(r.bbox(), BBox::new(0.0, 0.0, 3.0, 2.0));
        let mut r2 = r.clone();
        r2.data[1] = f32::NAN;
        let v = r2.sample_bilinear(LatLon::new(1.0, 1.0)).unwrap();
        assert!((v - (0.0 + 10.0 + 11.0) / 3.0).abs() < 1e-5);
        assert_eq!(r2.min_max(), Some((0.0, 12.0)));
    }
}
