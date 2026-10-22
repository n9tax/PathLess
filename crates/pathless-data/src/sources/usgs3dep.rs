//! USGS 3DEP elevation via the National Map ArcGIS ImageServer `exportImage` endpoint.
//!
//! The service resamples the best available 3DEP data (1 m lidar … 1 arc-second) to any
//! bbox/size (max 8000×8000), so a cache tile is fetched with a single request whose bbox is
//! the tile's outer pixel edges and whose size is the tile's pixel count. The server's output
//! grid then coincides with our global grid (pixel-is-area), verified on decode.

use crate::geotiff::read_elevation_geotiff;
use crate::grid::TileGrid;
use crate::http::Http;
use crate::sources::resample_nearest;
use crate::types::GeoTransform;
use crate::{Error, Result};

pub const EXPORT_URL: &str =
    "https://elevation.nationalmap.gov/arcgis/rest/services/3DEPElevation/ImageServer/exportImage";

/// Build the exportImage URL for tile `(tx, ty)` of `grid`.
pub fn tile_url(grid: TileGrid, tx: i64, ty: i64) -> String {
    let b = grid.tile_bbox(tx, ty);
    format!(
        "{EXPORT_URL}?bbox={:.10},{:.10},{:.10},{:.10}&bboxSR=4326&imageSR=4326&size={},{}\
         &format=tiff&pixelType=F32&interpolation=RSP_BilinearInterpolation&noData=-9999\
         &compression=LZ77&f=image",
        b.min_lon, b.min_lat, b.max_lon, b.max_lat, grid.tile_px, grid.tile_px
    )
}

/// Download one tile. Returns `tile_px²` row-major f32 meters (NaN = nodata) and the number
/// of bytes downloaded. Areas outside 3DEP coverage come back as all-NaN (not an error).
pub async fn fetch_tile(http: &Http, grid: TileGrid, tx: i64, ty: i64) -> Result<(Vec<f32>, u64)> {
    let url = tile_url(grid, tx, ty);
    let mut last = String::new();
    // The service occasionally answers 200 with a JSON error body; retry those too.
    for attempt in 0..3u32 {
        if attempt > 0 {
            tokio::time::sleep(std::time::Duration::from_secs(2 << attempt)).await;
        }
        let resp = http.get(&url).await?;
        if resp.status != 200 {
            return Err(Error::Service(format!(
                "3DEP HTTP {}: {}",
                resp.status,
                String::from_utf8_lossy(&resp.bytes[..resp.bytes.len().min(300)])
            )));
        }
        let nbytes = resp.bytes.len() as u64;
        if !(resp.bytes.starts_with(b"II") || resp.bytes.starts_with(b"MM")) {
            last = format!(
                "3DEP returned non-TIFF ({}): {}",
                resp.content_type,
                String::from_utf8_lossy(&resp.bytes[..resp.bytes.len().min(300)])
            );
            tracing::warn!("{last}");
            continue;
        }
        let bytes = resp.bytes;
        let data = tokio::task::spawn_blocking(move || -> Result<Vec<f32>> {
            let r = read_elevation_geotiff(&bytes)?;
            let n = grid.tile_px as usize;
            let b = grid.tile_bbox(tx, ty);
            let dst = GeoTransform {
                origin_lon: b.min_lon,
                origin_lat: b.max_lat,
                px_lon: grid.pixel_deg(),
                px_lat: -grid.pixel_deg(),
            };
            if r.width == n && r.height == n {
                let tol = grid.pixel_deg() * 0.01;
                if (r.geo.origin_lon - dst.origin_lon).abs() < tol && (r.geo.origin_lat - dst.origin_lat).abs() < tol {
                    return Ok(r.data);
                }
            }
            tracing::warn!(w = r.width, h = r.height, ?r.geo, "3DEP grid mismatch; resampling");
            Ok(resample_nearest(&r, &dst, n, n, f32::NAN))
        })
        .await
        .map_err(|e| Error::Decode(e.to_string()))??;
        return Ok((data, nbytes));
    }
    Err(Error::Service(last))
}
