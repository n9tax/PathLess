//! Fixed global tile grids in EPSG:4326.
//!
//! Every grid is defined by an integer number of pixels per degree (`px_per_deg`) and an
//! integer tile size in pixels (`tile_px`). Global pixel column `c` covers longitudes
//! `[-180 + c/ppd, -180 + (c+1)/ppd]`, global row `r` covers latitudes
//! `[90 - (r+1)/ppd, 90 - r/ppd]` (pixel-is-area). Tile `(tx, ty)` holds columns
//! `tx*tile_px .. (tx+1)*tile_px` and rows `ty*tile_px .. (ty+1)*tile_px`, so `tx` counts
//! eastward from the antimeridian and `ty` southward from the north pole. Because all
//! indices are integers, tiles from different requests/polygons line up exactly and a
//! mosaic is a plain memory copy.

use serde::{Deserialize, Serialize};

use crate::types::{BBox, GeoTransform};

const EPS: f64 = 1e-7;

/// DEM resolution (pixel pitch in geographic degrees, equal in lat and lon).
///
/// Approximate ground pixel sizes are given for north–south; east–west pixels shrink
/// with `cos(latitude)` (e.g. ×0.77 at 40°N).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum DemResolution {
    /// 1 arc-second (1/3600°, ≈30.9 m). USGS 3DEP covers all of the US at this resolution.
    ArcSec1,
    /// 1/3 arc-second (1/10800°, ≈10.3 m). 3DEP: nearly all of CONUS, parts of AK.
    ArcSecThird,
    /// 1/9 arc-second (1/32400°, ≈3.4 m). 3DEP: lidar areas; elsewhere upsampled.
    ArcSecNinth,
    /// ≈1 m (1/97200° = 1/27 arc-second, ≈1.14 m). 3DEP: lidar areas; elsewhere upsampled.
    Meter1,
}

impl DemResolution {
    pub const ALL: [DemResolution; 4] =
        [DemResolution::ArcSec1, DemResolution::ArcSecThird, DemResolution::ArcSecNinth, DemResolution::Meter1];

    /// Short identifier used in cache paths.
    pub fn id(self) -> &'static str {
        match self {
            DemResolution::ArcSec1 => "1s",
            DemResolution::ArcSecThird => "13s",
            DemResolution::ArcSecNinth => "19s",
            DemResolution::Meter1 => "1m",
        }
    }

    /// Pixels per degree.
    pub fn px_per_deg(self) -> u32 {
        match self {
            DemResolution::ArcSec1 => 3600,
            DemResolution::ArcSecThird => 10_800,
            DemResolution::ArcSecNinth => 32_400,
            DemResolution::Meter1 => 97_200,
        }
    }

    /// Approximate north–south pixel size in meters.
    pub fn approx_pixel_m(self) -> f64 {
        111_195.0 / self.px_per_deg() as f64
    }

    /// The global tile grid used for caching this resolution.
    ///
    /// | res | px/deg | tile | tile px | raw f32 size |
    /// |-----|--------|------|---------|--------------|
    /// | ArcSec1     | 3600   | 0.5°    | 1800 | 13 MB |
    /// | ArcSecThird | 10800  | 0.125°  | 1350 | 7.3 MB |
    /// | ArcSecNinth | 32400  | 0.05°   | 1620 | 10.5 MB |
    /// | Meter1      | 97200  | 0.02°   | 1944 | 15 MB |
    pub fn grid(self) -> TileGrid {
        let ppd = self.px_per_deg();
        let tile_px = match self {
            DemResolution::ArcSec1 => 1800,
            DemResolution::ArcSecThird => 1350,
            DemResolution::ArcSecNinth => 1620,
            DemResolution::Meter1 => 1944,
        };
        TileGrid { px_per_deg: ppd, tile_px }
    }

    /// Next coarser resolution, if any.
    pub fn coarser(self) -> Option<DemResolution> {
        match self {
            DemResolution::ArcSec1 => None,
            DemResolution::ArcSecThird => Some(DemResolution::ArcSec1),
            DemResolution::ArcSecNinth => Some(DemResolution::ArcSecThird),
            DemResolution::Meter1 => Some(DemResolution::ArcSecNinth),
        }
    }

    /// Finest resolution whose mosaic for `area` stays within `max_pixels`.
    pub fn finest_within(area: &BBox, max_pixels: u64) -> DemResolution {
        for r in [DemResolution::Meter1, DemResolution::ArcSecNinth, DemResolution::ArcSecThird] {
            if r.grid().window(area).pixel_count() <= max_pixels {
                return r;
            }
        }
        DemResolution::ArcSec1
    }
}

/// A fixed global tile grid (see module docs).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct TileGrid {
    pub px_per_deg: u32,
    pub tile_px: u32,
}

/// A rectangular window of global pixel indices `[col0, col1) x [row0, row1)`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct PixelWindow {
    pub col0: i64,
    pub row0: i64,
    pub col1: i64,
    pub row1: i64,
}

impl PixelWindow {
    pub fn width(&self) -> usize {
        (self.col1 - self.col0).max(0) as usize
    }
    pub fn height(&self) -> usize {
        (self.row1 - self.row0).max(0) as usize
    }
    pub fn pixel_count(&self) -> u64 {
        self.width() as u64 * self.height() as u64
    }
}

impl TileGrid {
    pub fn pixel_deg(&self) -> f64 {
        1.0 / self.px_per_deg as f64
    }

    pub fn tile_deg(&self) -> f64 {
        self.tile_px as f64 / self.px_per_deg as f64
    }

    /// Total global columns (360°) and rows (180°).
    pub fn global_cols(&self) -> i64 {
        360 * self.px_per_deg as i64
    }
    pub fn global_rows(&self) -> i64 {
        180 * self.px_per_deg as i64
    }

    /// Smallest pixel window fully covering `area` (clamped to the globe).
    pub fn window(&self, area: &BBox) -> PixelWindow {
        let ppd = self.px_per_deg as f64;
        let c0 = ((area.min_lon + 180.0) * ppd + EPS).floor() as i64;
        let c1 = ((area.max_lon + 180.0) * ppd - EPS).ceil() as i64;
        let r0 = ((90.0 - area.max_lat) * ppd + EPS).floor() as i64;
        let r1 = ((90.0 - area.min_lat) * ppd - EPS).ceil() as i64;
        let (gc, gr) = (self.global_cols(), self.global_rows());
        let c0 = c0.clamp(0, gc - 1);
        let r0 = r0.clamp(0, gr - 1);
        PixelWindow { col0: c0, row0: r0, col1: c1.clamp(c0 + 1, gc), row1: r1.clamp(r0 + 1, gr) }
    }

    /// Geotransform of a window's top-left pixel.
    pub fn window_geo(&self, win: &PixelWindow) -> GeoTransform {
        let ppd = self.px_per_deg as f64;
        GeoTransform {
            origin_lon: -180.0 + win.col0 as f64 / ppd,
            origin_lat: 90.0 - win.row0 as f64 / ppd,
            px_lon: 1.0 / ppd,
            px_lat: -1.0 / ppd,
        }
    }

    /// Tile indices `(tx, ty)` intersecting a pixel window, row-major north to south.
    pub fn tiles_for_window(&self, win: &PixelWindow) -> Vec<(i64, i64)> {
        let t = self.tile_px as i64;
        let (tx0, tx1) = (win.col0.div_euclid(t), (win.col1 - 1).div_euclid(t));
        let (ty0, ty1) = (win.row0.div_euclid(t), (win.row1 - 1).div_euclid(t));
        let mut out = Vec::with_capacity(((tx1 - tx0 + 1) * (ty1 - ty0 + 1)).max(0) as usize);
        for ty in ty0..=ty1 {
            for tx in tx0..=tx1 {
                out.push((tx, ty));
            }
        }
        out
    }

    /// Pixel window of tile `(tx, ty)`.
    pub fn tile_window(&self, tx: i64, ty: i64) -> PixelWindow {
        let t = self.tile_px as i64;
        PixelWindow { col0: tx * t, row0: ty * t, col1: (tx + 1) * t, row1: (ty + 1) * t }
    }

    /// Geographic bounds of tile `(tx, ty)` (outer pixel edges).
    pub fn tile_bbox(&self, tx: i64, ty: i64) -> BBox {
        let w = self.tile_window(tx, ty);
        let ppd = self.px_per_deg as f64;
        BBox {
            min_lon: -180.0 + w.col0 as f64 / ppd,
            max_lon: -180.0 + w.col1 as f64 / ppd,
            max_lat: 90.0 - w.row0 as f64 / ppd,
            min_lat: 90.0 - w.row1 as f64 / ppd,
        }
    }

    /// Cache file stem for a tile: south-west corner, e.g. `n39.87500_w105.25000`.
    pub fn tile_name(&self, tx: i64, ty: i64) -> String {
        let b = self.tile_bbox(tx, ty);
        let ns = if b.min_lat < 0.0 { 's' } else { 'n' };
        let ew = if b.min_lon < 0.0 { 'w' } else { 'e' };
        format!("{ns}{:08.5}_{ew}{:09.5}", b.min_lat.abs(), b.min_lon.abs())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn grid_alignment() {
        let g = DemResolution::ArcSecThird.grid();
        assert!((g.tile_deg() - 0.125).abs() < 1e-12);
        let area = BBox::new(-105.30, 39.95, -105.25, 40.00);
        let win = g.window(&area);
        assert_eq!(win.width(), 540);
        assert_eq!(win.height(), 540);
        let geo = g.window_geo(&win);
        assert!((geo.origin_lon - -105.30).abs() < 1e-9);
        assert!((geo.origin_lat - 40.0).abs() < 1e-9);
        let tiles = g.tiles_for_window(&win);
        // -105.30..-105.25 lies in tile -105.375..-105.25; 39.95..40.0 in 39.875..40.0
        assert_eq!(tiles.len(), 1);
        let b = g.tile_bbox(tiles[0].0, tiles[0].1);
        assert!((b.min_lon - -105.375).abs() < 1e-9 && (b.max_lon - -105.25).abs() < 1e-9);
        assert!((b.min_lat - 39.875).abs() < 1e-9 && (b.max_lat - 40.0).abs() < 1e-9);
        assert_eq!(g.tile_name(tiles[0].0, tiles[0].1), "n39.87500_w105.37500");
        // straddling
        let tiles = g.tiles_for_window(&g.window(&BBox::new(-105.26, 39.99, -105.24, 40.01)));
        assert_eq!(tiles.len(), 4);
    }

    #[test]
    fn finest_within() {
        let small = BBox::new(-105.3, 39.95, -105.25, 40.0);
        assert_eq!(DemResolution::finest_within(&small, 400_000_000), DemResolution::Meter1);
        let big = BBox::new(-110.0, 35.0, -100.0, 45.0);
        assert_eq!(DemResolution::finest_within(&big, 400_000_000), DemResolution::ArcSec1);
    }
}
