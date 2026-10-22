//! NLCD land cover via the MRLC GeoServer WCS 2.0.1 (`GetCoverage`, reprojected server-side
//! to EPSG:4326).
//!
//! The server returns its own (~30 m, snapped) grid, so each cache tile is requested with a
//! padded bbox and resampled (nearest neighbour) into our fixed 1 arc-second grid. Several
//! regional coverages exist; every coverage whose rough extent intersects the tile is
//! queried and the results are composited (first non-zero class wins). Requests entirely
//! outside a coverage return HTTP 404 `InvalidSubsetting`, which is treated as "no data".
//!
//! As of 2026-10 the newest *per-year* land-cover coverages on the MRLC WCS are
//! NLCD 2021 (CONUS), 2016 (Alaska), 2001 (Hawaii, Puerto Rico). Annual NLCD (1985–2023) is
//! only published there as change summaries (`Annual_NLCD_Smy_LndCov_*`), not as land cover.

use serde::{Deserialize, Serialize};

use crate::geotiff::read_geotiff;
use crate::grid::TileGrid;
use crate::http::Http;
use crate::sources::resample_nearest;
use crate::types::{BBox, GeoTransform};
use crate::{Error, Result};

pub const WCS_URL: &str = "https://www.mrlc.gov/geoserver/ows";

/// Padding (degrees) added around each tile request to avoid reprojection-edge nodata.
pub const PAD_DEG: f64 = 0.005;

/// A regional NLCD coverage on the MRLC WCS.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct NlcdCoverage {
    pub coverage_id: &'static str,
    pub region: &'static str,
    pub year: u16,
    /// Generous geographic envelope used to decide whether to query this coverage.
    pub bounds: BBox,
}

/// Coverages in priority order.
pub const COVERAGES: &[NlcdCoverage] = &[
    NlcdCoverage {
        coverage_id: "mrlc_download__NLCD_2021_Land_Cover_L48",
        region: "CONUS",
        year: 2021,
        bounds: BBox::new(-130.5, 21.5, -63.0, 53.0),
    },
    NlcdCoverage {
        coverage_id: "mrlc_download__NLCD_2016_Land_Cover_AK",
        region: "Alaska",
        year: 2016,
        bounds: BBox::new(-180.0, 50.5, -128.0, 72.0),
    },
    NlcdCoverage {
        coverage_id: "mrlc_download__NLCD_2001_Land_Cover_HI",
        region: "Hawaii",
        year: 2001,
        bounds: BBox::new(-161.0, 18.5, -154.5, 22.5),
    },
    NlcdCoverage {
        coverage_id: "mrlc_download__NLCD_2001_Land_Cover_PR",
        region: "Puerto Rico",
        year: 2001,
        bounds: BBox::new(-68.0, 17.5, -65.0, 18.7),
    },
];

/// Coverages whose envelope intersects `area`.
pub fn coverages_for(area: &BBox) -> Vec<&'static NlcdCoverage> {
    COVERAGES.iter().filter(|c| c.bounds.intersects(area)).collect()
}

pub fn coverage_url(coverage_id: &str, b: &BBox) -> String {
    let crs = "http://www.opengis.net/def/crs/EPSG/0/4326";
    format!(
        "{WCS_URL}?service=WCS&version=2.0.1&request=GetCoverage&coverageId={coverage_id}\
         &subset=Long({:.8},{:.8})&subset=Lat({:.8},{:.8})&subsettingCrs={crs}&outputCrs={crs}\
         &format=image/geotiff",
        b.min_lon, b.max_lon, b.min_lat, b.max_lat
    )
}

/// Download one tile: `tile_px²` NLCD class codes (0 = nodata) plus bytes downloaded.
pub async fn fetch_tile(http: &Http, grid: TileGrid, tx: i64, ty: i64) -> Result<(Vec<u8>, u64)> {
    let tb = grid.tile_bbox(tx, ty);
    let n = grid.tile_px as usize;
    let dst = GeoTransform {
        origin_lon: tb.min_lon,
        origin_lat: tb.max_lat,
        px_lon: grid.pixel_deg(),
        px_lat: -grid.pixel_deg(),
    };
    let req = tb.expand_deg(PAD_DEG);
    let mut out = vec![0u8; n * n];
    let mut total_bytes = 0u64;
    for cov in coverages_for(&tb) {
        if !out.contains(&0) {
            break;
        }
        let url = coverage_url(cov.coverage_id, &req);
        let resp = http.get(&url).await?;
        total_bytes += resp.bytes.len() as u64;
        let body = || String::from_utf8_lossy(&resp.bytes[..resp.bytes.len().min(400)]).into_owned();
        if resp.status == 404 && body().contains("InvalidSubsetting") {
            tracing::debug!(coverage = cov.coverage_id, tx, ty, "tile outside coverage");
            continue;
        }
        if resp.status != 200 {
            return Err(Error::Service(format!("NLCD WCS HTTP {}: {}", resp.status, body())));
        }
        if !(resp.bytes.starts_with(b"II") || resp.bytes.starts_with(b"MM")) {
            return Err(Error::Service(format!("NLCD WCS returned non-TIFF ({}): {}", resp.content_type, body())));
        }
        let bytes = resp.bytes;
        let part = tokio::task::spawn_blocking(move || -> Result<Vec<u8>> {
            let g = read_geotiff::<u8>(&bytes, 0)?;
            let mut r = g.raster;
            // Classes are codes 1..=255; honour a non-zero GDAL nodata if one is ever set.
            if let Some(nd) = r.nodata.filter(|&v| v != 0) {
                r.data.iter_mut().filter(|v| **v == nd).for_each(|v| *v = 0);
            }
            Ok(resample_nearest(&r, &dst, n, n, 0u8))
        })
        .await
        .map_err(|e| Error::Decode(e.to_string()))??;
        for (o, p) in out.iter_mut().zip(part) {
            if *o == 0 {
                *o = p;
            }
        }
    }
    Ok((out, total_bytes))
}
