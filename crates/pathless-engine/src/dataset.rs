//! Terrain + clutter loaded for a project: works out the data area from the
//! study polygon and sites, plans tiles, fetches and holds the mosaics.

use pathless_data::{BBox, DataManager, DemResolution, LatLon, Progress, Raster, TilePlan};
use serde::Serialize;

use crate::project::{DemChoice, Project};

/// Padding around the study area / sites so profiles near the edge stay inside data.
const AREA_PAD_M: f64 = 1_000.0;

pub fn dem_resolution(c: DemChoice) -> DemResolution {
    match c {
        DemChoice::ArcSec1 => DemResolution::ArcSec1,
        DemChoice::ArcSecThird => DemResolution::ArcSecThird,
        DemChoice::ArcSecNinth => DemResolution::ArcSecNinth,
        DemChoice::Meter1 => DemResolution::Meter1,
    }
}

/// Polygon to `LatLon` vertices.
pub fn area_polygon(project: &Project) -> Vec<LatLon> {
    project.area.iter().map(|p| LatLon::new(p[1], p[0])).collect()
}

/// Bounding box of everything that needs terrain: the study polygon, every
/// site, and — when no polygon is drawn — the coverage radius around the
/// coverage site.
pub fn data_area(project: &Project) -> anyhow::Result<BBox> {
    let mut pts = area_polygon(project);
    for s in &project.sites {
        pts.push(LatLon::new(s.lat, s.lon));
    }
    if project.area.len() < 3 {
        if let Some(s) = project.coverage.site_id.as_deref().and_then(|id| project.site(id)) {
            let r = project.coverage.max_radius_km * 1000.0;
            for brg in [0.0, 90.0, 180.0, 270.0] {
                pts.push(pathless_data::geodesy::destination(LatLon::new(s.lat, s.lon), brg, r));
            }
        }
    }
    let bbox = BBox::from_polygon(&pts).ok_or_else(|| anyhow::anyhow!("Draw a study area or add sites first"))?;
    Ok(bbox.expand_m(AREA_PAD_M))
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PlanSummary {
    pub tile_count: usize,
    pub cached: usize,
    pub to_download: usize,
    pub total_pixels: u64,
    pub estimated_bytes: u64,
    pub mosaic_width: usize,
    pub mosaic_height: usize,
}

impl From<&TilePlan> for PlanSummary {
    fn from(p: &TilePlan) -> Self {
        PlanSummary {
            tile_count: p.tiles.len(),
            cached: p.cached,
            to_download: p.to_download,
            total_pixels: p.total_pixels,
            estimated_bytes: p.estimated_bytes,
            mosaic_width: p.mosaic_width,
            mosaic_height: p.mosaic_height,
        }
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DataPlan {
    pub bbox: BBox,
    pub area_km2: f64,
    pub elevation: PlanSummary,
    pub landcover: Option<PlanSummary>,
    pub warnings: Vec<String>,
}

pub fn plan(dm: &DataManager, project: &Project) -> anyhow::Result<DataPlan> {
    let bbox = data_area(project)?;
    let res = dem_resolution(project.data.dem_resolution);
    let elev = dm.plan_elevation(&bbox, res);
    let mut warnings = Vec::new();
    if elev.exceeds_limit {
        let better = DemResolution::finest_within(&bbox, dm.max_mosaic_pixels());
        warnings.push(format!(
            "The area is too large for this resolution ({:.0} M pixels). Try {} (~{:.0} m).",
            elev.total_pixels as f64 / 1e6,
            better.id(),
            better.approx_pixel_m()
        ));
    }
    let lc = project.data.use_clutter.then(|| dm.plan_landcover(&bbox));
    Ok(DataPlan {
        bbox,
        area_km2: bbox.area_km2(),
        elevation: (&elev).into(),
        landcover: lc.as_ref().map(Into::into),
        warnings,
    })
}

/// Loaded terrain (and optionally land cover) for the project's data area.
pub struct Dataset {
    pub bbox: BBox,
    pub resolution: DemResolution,
    pub dem: Raster<f32>,
    pub landcover: Option<Raster<u8>>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RasterInfo {
    pub width: usize,
    pub height: usize,
    pub pixel_m: f64,
    pub min_m: Option<f32>,
    pub max_m: Option<f32>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DataStatus {
    pub bbox: BBox,
    pub dem_resolution: String,
    pub elevation: RasterInfo,
    pub landcover: Option<RasterInfo>,
}

pub async fn fetch(
    dm: &DataManager,
    project: &Project,
    progress: impl Fn(Progress) + Send + Sync + Clone,
) -> anyhow::Result<Dataset> {
    let bbox = data_area(project)?;
    let res = dem_resolution(project.data.dem_resolution);
    let dem = dm.fetch_elevation(&bbox, res, progress.clone()).await?;
    if dem.min_max().is_none() {
        anyhow::bail!("No elevation data is available for this area (3DEP covers the US and territories)");
    }
    let landcover = if project.data.use_clutter {
        match dm.fetch_landcover(&bbox, progress).await {
            Ok(lc) => Some(lc),
            Err(e) => {
                tracing::warn!("land cover unavailable: {e}");
                None
            }
        }
    } else {
        None
    };
    Ok(Dataset { bbox, resolution: res, dem, landcover })
}

impl Dataset {
    pub fn status(&self) -> DataStatus {
        let mm = self.dem.min_max();
        DataStatus {
            bbox: self.bbox,
            dem_resolution: self.resolution.id().to_string(),
            elevation: RasterInfo {
                width: self.dem.width,
                height: self.dem.height,
                pixel_m: self.resolution.approx_pixel_m(),
                min_m: mm.map(|m| m.0),
                max_m: mm.map(|m| m.1),
            },
            landcover: self.landcover.as_ref().map(|lc| RasterInfo {
                width: lc.width,
                height: lc.height,
                pixel_m: 30.0,
                min_m: None,
                max_m: None,
            }),
        }
    }

    pub fn elevation_at(&self, lat: f64, lon: f64) -> Option<f64> {
        self.dem.sample_bilinear(LatLon::new(lat, lon)).map(f64::from)
    }

    pub fn landcover_at(&self, p: LatLon) -> u8 {
        self.landcover.as_ref().and_then(|lc| lc.sample_nearest(p)).unwrap_or(0)
    }

    pub fn contains(&self, p: LatLon) -> bool {
        self.dem.bbox().contains(p)
    }
}
