//! Data sources. Each source knows how to produce one cache tile of the fixed global grid.
//!
//! Adding a source (see README): add an enum variant, give it a cache `id()`, a tile grid,
//! and a `fetch_*_tile` implementation that returns exactly `tile_px * tile_px` values
//! aligned to the grid (resample with [`resample_nearest`] if the upstream grid differs).

pub mod nlcd_wcs;
pub mod usgs3dep;

use serde::{Deserialize, Serialize};

use crate::grid::{DemResolution, TileGrid};
use crate::types::{BBox, GeoTransform, Raster};

/// Where elevation comes from.
///
/// TODO: `CopernicusGlo30` — global 30 m DSM from the public AWS bucket
/// `s3://copernicus-dem-30m` (HTTPS: `https://copernicus-dem-30m.s3.amazonaws.com/`),
/// 1°×1° COG tiles with deterministic names
/// `Copernicus_DSM_COG_10_N39_00_W106_00_DEM/Copernicus_DSM_COG_10_N39_00_W106_00_DEM.tif`.
/// Note GLO-30 tiles are pixel-is-point with longitude spacing that widens above 50° lat,
/// so tiles must be resampled into our `ArcSec1` grid. Implementation: map our 0.5° tile to
/// its 1° source tile, download (and keep) the COG, decode with `geotiff::read_geotiff`, then
/// resample (bilinear) into the grid. Only `ArcSec1` would be supported.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
pub enum ElevationSource {
    /// USGS 3D Elevation Program via the National Map ArcGIS ImageServer (`exportImage`).
    /// US + territories, best available resolution resampled to the requested grid.
    #[default]
    Usgs3dep,
}

impl ElevationSource {
    /// Short id used as the cache directory name.
    pub fn id(self) -> &'static str {
        match self {
            ElevationSource::Usgs3dep => "3dep",
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            ElevationSource::Usgs3dep => "USGS 3DEP",
        }
    }

    pub fn supports(self, res: DemResolution) -> bool {
        match (self, res) {
            (ElevationSource::Usgs3dep, _) => true,
            // (CopernicusGlo30, r) => r == DemResolution::ArcSec1,
        }
    }

    /// Rough geographic coverage (for UI hints; tiles outside are simply empty).
    pub fn coverage(self) -> Vec<BBox> {
        match self {
            ElevationSource::Usgs3dep => vec![
                BBox::new(-125.5, 24.0, -66.5, 49.5),    // CONUS
                BBox::new(-180.0, 51.0, -129.5, 71.5),   // Alaska
                BBox::new(-160.5, 18.5, -154.5, 22.5),   // Hawaii
                BBox::new(-68.0, 17.5, -64.5, 18.6),     // PR / USVI
                BBox::new(144.5, 13.0, 146.2, 20.6),     // Guam / CNMI
                BBox::new(-171.2, -14.6, -168.0, -11.0), // American Samoa
            ],
        }
    }

    /// Typical size in bytes of one downloaded tile (for progress estimates).
    pub fn estimated_tile_download_bytes(self, grid: TileGrid) -> u64 {
        match self {
            // F32 TIFF, LZW (compression=LZ77 yields LZW, ~85 % of raw for float DEMs)
            ElevationSource::Usgs3dep => (grid.tile_px as u64).pow(2) * 4 * 85 / 100,
        }
    }
}

/// Where land cover (clutter) comes from.
///
/// TODO: `EsaWorldCover` — global 10 m land cover (2021 v200) from AWS
/// `s3://esa-worldcover/v200/2021/map/ESA_WorldCover_10m_2021_v200_N39W106_Map.tif`
/// (3°×3° COG tiles, names from the SW corner rounded down to multiples of 3). Classes
/// differ from NLCD (10 tree, 20 shrub, 30 grass, 40 crop, 50 built-up, 60 bare, 70 snow,
/// 80 water, 90 wetland, 95 mangrove, 100 moss); expose a mapping to NLCD-like classes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
pub enum LandCoverSource {
    /// USGS/MRLC National Land Cover Database via MRLC GeoServer WCS 2.0.1.
    #[default]
    Nlcd,
}

impl LandCoverSource {
    pub fn id(self) -> &'static str {
        match self {
            LandCoverSource::Nlcd => "nlcd",
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            LandCoverSource::Nlcd => "NLCD",
        }
    }

    /// Cache grid: 1 arc-second pixels in 0.25° (900 px) tiles.
    pub fn grid(self) -> TileGrid {
        match self {
            LandCoverSource::Nlcd => TileGrid { px_per_deg: 3600, tile_px: 900 },
        }
    }

    pub fn estimated_tile_download_bytes(self, grid: TileGrid) -> u64 {
        match self {
            // native ~30 m (≈0.0004°×0.00029°) uncompressed 8-bit
            LandCoverSource::Nlcd => {
                let d = grid.tile_deg() + 2.0 * nlcd_wcs::PAD_DEG;
                ((d / 0.000396) * (d / 0.000287)) as u64
            }
        }
    }
}

/// Nearest-neighbour resample of `src` onto a `w`×`h` grid with geotransform `dst`.
/// Pixels whose center falls outside `src` get `fill`. Exact-alignment fast path copies rows.
pub fn resample_nearest<T: Copy>(src: &Raster<T>, dst: &GeoTransform, w: usize, h: usize, fill: T) -> Vec<T> {
    let mut out = vec![fill; w * h];
    let sx = dst.px_lon / src.geo.px_lon;
    let sy = dst.px_lat / src.geo.px_lat;
    let ox = (dst.origin_lon - src.geo.origin_lon) / src.geo.px_lon;
    let oy = (dst.origin_lat - src.geo.origin_lat) / src.geo.px_lat;
    // Fast path: same pixel size and integer offset.
    if (sx - 1.0).abs() < 1e-9
        && (sy - 1.0).abs() < 1e-9
        && (ox - ox.round()).abs() < 1e-3
        && (oy - oy.round()).abs() < 1e-3
    {
        let (ox, oy) = (ox.round() as i64, oy.round() as i64);
        for y in 0..h {
            let syy = oy + y as i64;
            if syy < 0 || syy >= src.height as i64 {
                continue;
            }
            let x_start = (-ox).max(0) as usize;
            let x_end = ((src.width as i64 - ox).min(w as i64)).max(0) as usize;
            if x_start >= x_end {
                continue;
            }
            let s0 = syy as usize * src.width + (ox + x_start as i64) as usize;
            out[y * w + x_start..y * w + x_end].copy_from_slice(&src.data[s0..s0 + (x_end - x_start)]);
        }
        return out;
    }
    let col_map: Vec<Option<usize>> = (0..w)
        .map(|x| {
            let f = ox + (x as f64 + 0.5) * sx;
            (f >= 0.0 && f < src.width as f64).then_some(f as usize)
        })
        .collect();
    for y in 0..h {
        let f = oy + (y as f64 + 0.5) * sy;
        if !(f >= 0.0 && f < src.height as f64) {
            continue;
        }
        let row = f as usize * src.width;
        for (x, c) in col_map.iter().enumerate() {
            if let Some(c) = c {
                out[y * w + x] = src.data[row + c];
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resample() {
        let src = Raster {
            width: 4,
            height: 2,
            data: vec![1u8, 2, 3, 4, 5, 6, 7, 8],
            geo: GeoTransform { origin_lon: 0.0, origin_lat: 2.0, px_lon: 1.0, px_lat: -1.0 },
            nodata: Some(0),
        };
        // aligned, shifted by one column
        let dst = GeoTransform { origin_lon: 1.0, origin_lat: 2.0, px_lon: 1.0, px_lat: -1.0 };
        assert_eq!(resample_nearest(&src, &dst, 4, 2, 0), vec![2, 3, 4, 0, 6, 7, 8, 0]);
        // half-size pixels
        let dst = GeoTransform { origin_lon: 0.0, origin_lat: 2.0, px_lon: 0.5, px_lat: -0.5 };
        let r = resample_nearest(&src, &dst, 3, 3, 0);
        assert_eq!(r, vec![1, 1, 2, 1, 1, 2, 5, 5, 6]);
    }
}
