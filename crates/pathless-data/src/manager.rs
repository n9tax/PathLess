//! `DataManager`: plans, downloads, caches and mosaics terrain and land-cover tiles.

use std::path::{Path, PathBuf};

use futures_util::stream::{self, StreamExt};
use serde::{Deserialize, Serialize};

use crate::cache;
use crate::grid::{DemResolution, PixelWindow, TileGrid};
use crate::http::Http;
use crate::sources::{nlcd_wcs, usgs3dep, ElevationSource, LandCoverSource};
use crate::types::{BBox, Raster};
use crate::{Error, Result};

/// Default mosaic size limit (pixels). 400 M f32 pixels = 1.6 GB.
pub const DEFAULT_MAX_MOSAIC_PIXELS: u64 = 400_000_000;
/// Default number of simultaneous HTTP requests (shared by all fetches of one manager).
pub const DEFAULT_CONCURRENCY: usize = 4;

/// Kind of data a tile holds.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum DataKind {
    Elevation,
    LandCover,
}

/// One cache tile needed for a request.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TileRequest {
    pub kind: DataKind,
    /// Source id (cache directory), e.g. "3dep" or "nlcd".
    pub source: String,
    /// DEM resolution (elevation tiles only).
    pub resolution: Option<DemResolution>,
    /// Global tile column (eastward from -180°).
    pub tx: i64,
    /// Global tile row (southward from +90°).
    pub ty: i64,
    /// Tile extent (outer pixel edges).
    pub bbox: BBox,
    pub width: u32,
    pub height: u32,
    /// Already present in the cache.
    pub cached: bool,
    /// Cache file path.
    pub path: PathBuf,
}

/// What a fetch will need to do.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TilePlan {
    pub tiles: Vec<TileRequest>,
    /// Pixel count of the output mosaic covering the requested area.
    pub total_pixels: u64,
    /// Estimated bytes to download for the tiles not yet cached.
    pub estimated_bytes: u64,
    pub cached: usize,
    pub to_download: usize,
    /// Output mosaic dimensions.
    pub mosaic_width: usize,
    pub mosaic_height: usize,
    /// `total_pixels` exceeds the manager's mosaic limit; a fetch would fail.
    pub exceeds_limit: bool,
}

/// Progress report passed to fetch callbacks.
///
/// Stages: `"plan"`, `"download"` (done/total = tiles), `"mosaic"`, `"done"`.
/// `bytes` is the cumulative number of bytes downloaded in this fetch.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Progress {
    pub stage: String,
    pub done: usize,
    pub total: usize,
    pub bytes: u64,
    pub message: String,
}

impl Progress {
    fn new(stage: &str, done: usize, total: usize, bytes: u64, message: impl Into<String>) -> Self {
        Progress { stage: stage.into(), done, total, bytes, message: message.into() }
    }
}

/// Downloads and caches terrain / land-cover tiles and assembles mosaics.
///
/// Cheap to share behind an `Arc`; all methods take `&self`. The HTTP concurrency limit
/// is global to the manager.
#[derive(Clone)]
pub struct DataManager {
    cache_dir: PathBuf,
    http: Http,
    elevation_source: ElevationSource,
    landcover_source: LandCoverSource,
    max_mosaic_pixels: u64,
}

impl DataManager {
    /// Create a manager. `cache_dir = None` uses the platform cache dir
    /// (`~/.cache/pathless` on Linux). The directory is created if missing.
    pub fn new(cache_dir: Option<PathBuf>) -> Result<Self> {
        let cache_dir = match cache_dir {
            Some(d) => d,
            None => directories::ProjectDirs::from("", "", "pathless")
                .map(|p| p.cache_dir().to_path_buf())
                .ok_or_else(|| Error::Cache("cannot determine a cache directory (no home dir?)".into()))?,
        };
        std::fs::create_dir_all(&cache_dir)?;
        Ok(DataManager {
            cache_dir,
            http: Http::new(DEFAULT_CONCURRENCY)?,
            elevation_source: ElevationSource::default(),
            landcover_source: LandCoverSource::default(),
            max_mosaic_pixels: DEFAULT_MAX_MOSAIC_PIXELS,
        })
    }

    /// Change the mosaic pixel limit (default [`DEFAULT_MAX_MOSAIC_PIXELS`]).
    pub fn with_max_mosaic_pixels(mut self, limit: u64) -> Self {
        self.max_mosaic_pixels = limit;
        self
    }

    /// Change the HTTP concurrency limit (default [`DEFAULT_CONCURRENCY`]).
    pub fn with_concurrency(mut self, n: usize) -> Result<Self> {
        self.http = Http::new(n)?;
        Ok(self)
    }

    pub fn with_elevation_source(mut self, s: ElevationSource) -> Self {
        self.elevation_source = s;
        self
    }

    pub fn with_landcover_source(mut self, s: LandCoverSource) -> Self {
        self.landcover_source = s;
        self
    }

    pub fn cache_dir(&self) -> &Path {
        &self.cache_dir
    }

    pub fn max_mosaic_pixels(&self) -> u64 {
        self.max_mosaic_pixels
    }

    pub fn elevation_source(&self) -> ElevationSource {
        self.elevation_source
    }

    pub fn landcover_source(&self) -> LandCoverSource {
        self.landcover_source
    }

    fn elevation_dir(&self, res: DemResolution) -> PathBuf {
        self.cache_dir.join(self.elevation_source.id()).join(res.id())
    }

    fn landcover_dir(&self) -> PathBuf {
        self.cache_dir.join(self.landcover_source.id()).join("1s")
    }

    #[allow(clippy::too_many_arguments)]
    fn make_plan(
        &self,
        area: &BBox,
        grid: TileGrid,
        kind: DataKind,
        source: &str,
        resolution: Option<DemResolution>,
        dir: &Path,
        tile_bytes: u64,
    ) -> TilePlan {
        let win = grid.window(area);
        let tiles: Vec<TileRequest> = grid
            .tiles_for_window(&win)
            .into_iter()
            .map(|(tx, ty)| {
                let path = dir.join(format!("{}.bin", grid.tile_name(tx, ty)));
                TileRequest {
                    kind,
                    source: source.to_string(),
                    resolution,
                    tx,
                    ty,
                    bbox: grid.tile_bbox(tx, ty),
                    width: grid.tile_px,
                    height: grid.tile_px,
                    cached: path.is_file(),
                    path,
                }
            })
            .collect();
        let cached = tiles.iter().filter(|t| t.cached).count();
        let to_download = tiles.len() - cached;
        TilePlan {
            total_pixels: win.pixel_count(),
            estimated_bytes: to_download as u64 * tile_bytes,
            cached,
            to_download,
            mosaic_width: win.width(),
            mosaic_height: win.height(),
            exceeds_limit: win.pixel_count() > self.max_mosaic_pixels,
            tiles,
        }
    }

    /// Which elevation tiles cover `area` at `res`, and how many are already cached.
    pub fn plan_elevation(&self, area: &BBox, res: DemResolution) -> TilePlan {
        let grid = res.grid();
        let src = self.elevation_source;
        self.make_plan(
            area,
            grid,
            DataKind::Elevation,
            src.id(),
            Some(res),
            &self.elevation_dir(res),
            src.estimated_tile_download_bytes(grid),
        )
    }

    /// Which land-cover tiles cover `area`, and how many are already cached.
    pub fn plan_landcover(&self, area: &BBox) -> TilePlan {
        let src = self.landcover_source;
        let grid = src.grid();
        self.make_plan(
            area,
            grid,
            DataKind::LandCover,
            src.id(),
            None,
            &self.landcover_dir(),
            src.estimated_tile_download_bytes(grid),
        )
    }

    fn check_area(&self, area: &BBox, plan: &TilePlan, res: Option<DemResolution>) -> Result<()> {
        if !area.is_valid() {
            return Err(Error::InvalidInput(format!("invalid bounding box {area:?}")));
        }
        if plan.exceeds_limit {
            let suggestion = res.and_then(|_| {
                let s = DemResolution::finest_within(area, self.max_mosaic_pixels);
                (s.grid().window(area).pixel_count() <= self.max_mosaic_pixels).then_some(s)
            });
            return Err(Error::TooLarge {
                pixels: plan.total_pixels,
                limit: self.max_mosaic_pixels,
                resolution: res.map_or("1 arc-second land cover".to_string(), |r| format!("{r:?}")),
                suggestion,
            });
        }
        Ok(())
    }

    async fn download_tile(&self, t: &TileRequest) -> Result<u64> {
        match t.kind {
            DataKind::Elevation => {
                let res =
                    t.resolution.ok_or_else(|| Error::InvalidInput("elevation tile without resolution".into()))?;
                let grid = res.grid();
                let (data, nbytes) = match self.elevation_source {
                    ElevationSource::Usgs3dep => usgs3dep::fetch_tile(&self.http, grid, t.tx, t.ty).await?,
                };
                let header = cache::TileHeader::new("f32", &t.source, grid, t.tx, t.ty);
                let path = t.path.clone();
                tokio::task::spawn_blocking(move || cache::write_atomic(&path, &cache::encode_f32(header, &data)?))
                    .await
                    .map_err(|e| Error::Cache(e.to_string()))??;
                Ok(nbytes)
            }
            DataKind::LandCover => {
                let grid = self.landcover_source.grid();
                let (data, nbytes) = match self.landcover_source {
                    LandCoverSource::Nlcd => nlcd_wcs::fetch_tile(&self.http, grid, t.tx, t.ty).await?,
                };
                let header = cache::TileHeader::new("u8", &t.source, grid, t.tx, t.ty);
                let path = t.path.clone();
                tokio::task::spawn_blocking(move || cache::write_atomic(&path, &cache::encode_u8(header, &data, 0)?))
                    .await
                    .map_err(|e| Error::Cache(e.to_string()))??;
                Ok(nbytes)
            }
        }
    }

    /// Download all uncached tiles of `plan` (bounded concurrency). Successfully downloaded
    /// tiles are kept even if others fail.
    async fn download_missing(
        &self,
        plan: &TilePlan,
        label: &str,
        progress: &(impl Fn(Progress) + Send + Sync),
    ) -> Result<u64> {
        // Owned requests: a closure over `&&TileRequest` returning an async block
        // makes the future fail higher-ranked Send checks (rust-lang/rust#102211)
        // when the caller spawns it.
        let missing: Vec<TileRequest> = plan.tiles.iter().filter(|t| !t.cached).cloned().collect();
        let total = missing.len();
        let mut bytes = 0u64;
        if total == 0 {
            return Ok(0);
        }
        progress(Progress::new("download", 0, total, 0, format!("Downloading {total} {label} tile(s)")));
        let mut results = stream::iter(missing.into_iter().map(|t| async move {
            let r = self.download_tile(&t).await;
            (t, r)
        }))
        .buffer_unordered(DEFAULT_CONCURRENCY);
        let (mut done, mut failed, mut first_err) = (0usize, 0usize, None::<String>);
        while let Some((t, r)) = results.next().await {
            done += 1;
            match r {
                Ok(n) => {
                    bytes += n;
                    progress(Progress::new(
                        "download",
                        done,
                        total,
                        bytes,
                        format!("{label} tile {} ready", t.path.file_stem().and_then(|s| s.to_str()).unwrap_or("?")),
                    ));
                }
                Err(e) => {
                    tracing::error!(tx = t.tx, ty = t.ty, "tile download failed: {e}");
                    failed += 1;
                    first_err.get_or_insert_with(|| e.to_string());
                    progress(Progress::new("download", done, total, bytes, format!("{label} tile failed: {e}")));
                }
            }
        }
        if failed > 0 {
            return Err(Error::Download { failed, total, first: first_err.unwrap_or_default() });
        }
        Ok(bytes)
    }

    /// Ensure all elevation tiles for `area` are cached (downloading missing ones) and
    /// return a mosaic exactly covering `area` on the global grid of `res`.
    ///
    /// Elevations are meters (NAVD88 orthometric heights for 3DEP); nodata is `NaN`
    /// (`nodata` field is `None`). Fails with [`Error::TooLarge`] — before downloading
    /// anything — if the mosaic would exceed the pixel limit; the error suggests the
    /// finest resolution that fits.
    pub async fn fetch_elevation(
        &self,
        area: &BBox,
        res: DemResolution,
        progress: impl Fn(Progress) + Send + Sync,
    ) -> Result<Raster<f32>> {
        let plan = self.plan_elevation(area, res);
        self.check_area(area, &plan, Some(res))?;
        if !self.elevation_source.supports(res) {
            return Err(Error::InvalidInput(format!("{} does not provide {res:?}", self.elevation_source.name())));
        }
        progress(Progress::new(
            "plan",
            0,
            plan.tiles.len(),
            0,
            format!(
                "{} elevation tiles ({} cached), mosaic {}x{}",
                plan.tiles.len(),
                plan.cached,
                plan.mosaic_width,
                plan.mosaic_height
            ),
        ));
        let bytes = self.download_missing(&plan, "elevation", &progress).await?;
        progress(Progress::new("mosaic", 0, plan.tiles.len(), bytes, "Assembling elevation mosaic"));
        let grid = res.grid();
        let win = grid.window(area);
        let tiles: Vec<_> = plan.tiles.iter().map(|t| (t.tx, t.ty, t.path.clone())).collect();
        let raster = tokio::task::spawn_blocking(move || {
            mosaic(grid, win, tiles, f32::NAN, None, |b| cache::decode_f32(b).map(|(_, d)| d))
        })
        .await
        .map_err(|e| Error::Cache(e.to_string()))??;
        let nd = raster.nodata_count();
        let msg = if nd == 0 {
            "Elevation ready".to_string()
        } else {
            format!(
                "Elevation ready; {:.1}% of pixels have no data (outside coverage)",
                100.0 * nd as f64 / raster.data.len() as f64
            )
        };
        progress(Progress::new("done", plan.tiles.len(), plan.tiles.len(), bytes, msg));
        Ok(raster)
    }

    /// Ensure all land-cover tiles for `area` are cached and return a 1 arc-second mosaic
    /// of NLCD class codes (`nodata = Some(0)`).
    pub async fn fetch_landcover(&self, area: &BBox, progress: impl Fn(Progress) + Send + Sync) -> Result<Raster<u8>> {
        let plan = self.plan_landcover(area);
        self.check_area(area, &plan, None)?;
        progress(Progress::new(
            "plan",
            0,
            plan.tiles.len(),
            0,
            format!(
                "{} land-cover tiles ({} cached), mosaic {}x{}",
                plan.tiles.len(),
                plan.cached,
                plan.mosaic_width,
                plan.mosaic_height
            ),
        ));
        let bytes = self.download_missing(&plan, "land-cover", &progress).await?;
        progress(Progress::new("mosaic", 0, plan.tiles.len(), bytes, "Assembling land-cover mosaic"));
        let grid = self.landcover_source.grid();
        let win = grid.window(area);
        let tiles: Vec<_> = plan.tiles.iter().map(|t| (t.tx, t.ty, t.path.clone())).collect();
        let raster = tokio::task::spawn_blocking(move || {
            mosaic(grid, win, tiles, 0u8, Some(0u8), |b| cache::decode_u8(b, 0).map(|(_, d)| d))
        })
        .await
        .map_err(|e| Error::Cache(e.to_string()))??;
        progress(Progress::new("done", plan.tiles.len(), plan.tiles.len(), bytes, "Land cover ready"));
        Ok(raster)
    }

    /// Total size of all files under the cache directory.
    pub fn cache_size_bytes(&self) -> u64 {
        fn walk(p: &Path) -> u64 {
            let Ok(rd) = std::fs::read_dir(p) else { return 0 };
            rd.flatten()
                .map(|e| match e.file_type() {
                    Ok(ft) if ft.is_dir() => walk(&e.path()),
                    Ok(ft) if ft.is_file() => e.metadata().map(|m| m.len()).unwrap_or(0),
                    _ => 0,
                })
                .sum()
        }
        walk(&self.cache_dir)
    }

    /// Delete all cached tiles of every known source. Only the source sub-directories
    /// this crate creates are removed, never other files in the cache directory.
    pub fn clear_cache(&self) -> Result<()> {
        for id in [ElevationSource::Usgs3dep.id(), LandCoverSource::Nlcd.id()] {
            let d = self.cache_dir.join(id);
            if d.exists() {
                std::fs::remove_dir_all(&d)?;
            }
        }
        Ok(())
    }
}

/// Copy cached tiles into a mosaic covering `win`. Missing tile files (should not happen
/// after a successful download) leave `fill`. A corrupt tile file is deleted and reported.
fn mosaic<T: Copy>(
    grid: TileGrid,
    win: PixelWindow,
    tiles: Vec<(i64, i64, PathBuf)>,
    fill: T,
    nodata: Option<T>,
    decode: impl Fn(&[u8]) -> Result<Vec<T>>,
) -> Result<Raster<T>> {
    let (w, h) = (win.width(), win.height());
    let mut out = Raster::filled(w, h, grid.window_geo(&win), fill, nodata);
    let tp = grid.tile_px as usize;
    for (tx, ty, path) in tiles {
        let bytes = match std::fs::read(&path) {
            Ok(b) => b,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                tracing::warn!("tile missing from cache: {}", path.display());
                continue;
            }
            Err(e) => return Err(e.into()),
        };
        let data = match decode(&bytes) {
            Ok(d) if d.len() == tp * tp => d,
            Ok(_) | Err(_) => {
                let _ = std::fs::remove_file(&path);
                return Err(Error::Cache(format!("corrupt cache tile {} was removed; please retry", path.display())));
            }
        };
        let tw = grid.tile_window(tx, ty);
        let c0 = tw.col0.max(win.col0);
        let c1 = tw.col1.min(win.col1);
        let r0 = tw.row0.max(win.row0);
        let r1 = tw.row1.min(win.row1);
        if c0 >= c1 || r0 >= r1 {
            continue;
        }
        let n = (c1 - c0) as usize;
        for r in r0..r1 {
            let src = (r - tw.row0) as usize * tp + (c0 - tw.col0) as usize;
            let dst = (r - win.row0) as usize * w + (c0 - win.col0) as usize;
            out.data[dst..dst + n].copy_from_slice(&data[src..src + n]);
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp_manager(tag: &str) -> DataManager {
        let dir = std::env::temp_dir().join(format!("pathless-mgr-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        DataManager::new(Some(dir)).unwrap()
    }

    #[test]
    fn plan_and_offline_mosaic() {
        let m = tmp_manager("plan");
        let area = BBox::new(-105.26, 39.99, -105.24, 40.01);
        let plan = m.plan_elevation(&area, DemResolution::ArcSecThird);
        assert_eq!(plan.tiles.len(), 4);
        assert_eq!(plan.cached, 0);
        assert_eq!(plan.to_download, 4);
        assert_eq!(plan.mosaic_width, 216);
        assert!(plan.tiles[0].path.to_string_lossy().contains("3dep/13s/"));

        // Fabricate cached tiles whose value encodes the global column, then mosaic.
        let grid = DemResolution::ArcSecThird.grid();
        let tp = grid.tile_px as usize;
        for t in &plan.tiles {
            let tw = grid.tile_window(t.tx, t.ty);
            let data: Vec<f32> = (0..tp * tp).map(|i| ((tw.col0 + (i % tp) as i64) % 100_000) as f32).collect();
            let h = cache::TileHeader::new("f32", "3dep", grid, t.tx, t.ty);
            cache::write_atomic(&t.path, &cache::encode_f32(h, &data).unwrap()).unwrap();
        }
        let plan2 = m.plan_elevation(&area, DemResolution::ArcSecThird);
        assert_eq!(plan2.cached, 4);
        let win = grid.window(&area);
        let tiles: Vec<_> = plan2.tiles.iter().map(|t| (t.tx, t.ty, t.path.clone())).collect();
        let r = mosaic(grid, win, tiles, f32::NAN, None, |b| cache::decode_f32(b).map(|(_, d)| d)).unwrap();
        for x in [0usize, 107, 108, 215] {
            assert_eq!(r.get(x, 5).unwrap(), ((win.col0 + x as i64) % 100_000) as f32);
            assert_eq!(r.get(x, 200).unwrap(), ((win.col0 + x as i64) % 100_000) as f32);
        }
        assert!(m.cache_size_bytes() > 0);
        m.clear_cache().unwrap();
        assert_eq!(m.cache_size_bytes(), 0);
        let _ = std::fs::remove_dir_all(m.cache_dir());
    }

    #[test]
    fn too_large_is_rejected() {
        let m = tmp_manager("big");
        let area = BBox::new(-110.0, 35.0, -105.0, 40.0);
        let plan = m.plan_elevation(&area, DemResolution::ArcSecThird);
        assert!(plan.exceeds_limit);
        let rt = tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap();
        let err = rt.block_on(m.fetch_elevation(&area, DemResolution::ArcSecThird, |_| {})).unwrap_err();
        match err {
            Error::TooLarge { suggestion, .. } => assert_eq!(suggestion, Some(DemResolution::ArcSec1)),
            e => panic!("unexpected {e}"),
        }
        let _ = std::fs::remove_dir_all(m.cache_dir());
    }
}
