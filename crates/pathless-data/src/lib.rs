//! # pathless-data
//!
//! Automatic acquisition of terrain (USGS 3DEP elevation) and clutter (NLCD land cover)
//! for a user-drawn area, with a persistent tile cache on a fixed global grid, mosaicking,
//! geodesy helpers and fast terrain/land-cover path profiles. Pure Rust (no GDAL).
//!
//! ```no_run
//! # async fn demo() -> pathless_data::Result<()> {
//! use pathless_data::*;
//! let polygon = [LatLon::new(40.0, -105.3), LatLon::new(39.95, -105.3), LatLon::new(39.97, -105.25)];
//! let area = BBox::from_polygon(&polygon).unwrap().expand_m(500.0);
//! let dm = DataManager::new(None)?;
//! let plan = dm.plan_elevation(&area, DemResolution::ArcSecThird);
//! println!("{} tiles, {} cached, ~{} MB to download", plan.tiles.len(), plan.cached, plan.estimated_bytes / 1_000_000);
//! let dem = dm.fetch_elevation(&area, DemResolution::ArcSecThird, |p| println!("{p:?}")).await?;
//! let lc = dm.fetch_landcover(&area, |_| {}).await?;
//! let prof = profile::terrain_profile(&dem, polygon[0], polygon[2], 10.0);
//! let clutter = profile::landcover_profile(&lc, &prof.points);
//! # Ok(()) }
//! ```

pub mod cache;
pub mod geodesy;
pub mod geotiff;
pub mod grid;
mod http;
pub mod manager;
pub mod nlcd;
pub mod profile;
pub mod sources;
pub mod types;

pub use grid::{DemResolution, PixelWindow, TileGrid};
pub use http::USER_AGENT;
pub use manager::{
    DataKind, DataManager, Progress, TilePlan, TileRequest, DEFAULT_CONCURRENCY, DEFAULT_MAX_MOSAIC_PIXELS,
};
pub use profile::{Profile, ProfileInfo};
pub use sources::{ElevationSource, LandCoverSource};
pub use types::{BBox, GeoTransform, LatLon, Raster, EARTH_RADIUS_M};

/// Errors produced by this crate. Serializes as its display string (Tauri-friendly).
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
    #[error("network error: {0}")]
    Http(String),
    #[error("data service error: {0}")]
    Service(String),
    #[error("decode error: {0}")]
    Decode(String),
    #[error("cache error: {0}")]
    Cache(String),
    #[error("invalid input: {0}")]
    InvalidInput(String),
    #[error(
        "area too large: {pixels} pixels at {resolution} exceeds the limit of {limit}{}",
        match suggestion { Some(s) => format!("; use {s:?} or a smaller area"), None => "; draw a smaller area".to_string() }
    )]
    TooLarge { pixels: u64, limit: u64, resolution: String, suggestion: Option<DemResolution> },
    #[error("{failed} of {total} tile download(s) failed (completed tiles were cached; retry to resume). First error: {first}")]
    Download { failed: usize, total: usize, first: String },
}

impl serde::Serialize for Error {
    fn serialize<S: serde::Serializer>(&self, s: S) -> std::result::Result<S::Ok, S::Error> {
        s.serialize_str(&self.to_string())
    }
}

pub type Result<T> = std::result::Result<T, Error>;
