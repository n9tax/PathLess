//! Area coverage: for every grid cell inside the study area, extract the
//! terrain (+ clutter) profile from the transmitter and evaluate the path
//! model, antenna pattern and link budget. Cells run in parallel with rayon.

use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Instant;

use anyhow::{bail, Context as _};
use pathless_data::{geodesy, profile, BBox, LatLon, Progress};
use pathless_rf::antenna::InterpolationMethod;
use rayon::prelude::*;
use serde::Serialize;

use crate::antennas::AntennaLibrary;
use crate::dataset::{area_polygon, Dataset};
use crate::link::{clutter_along, terminal_loss};
use crate::project::{CoverageOutput, LegendEntry, Project};
use crate::propagation::{choose_method, elevation_angle_deg, Method, PathModel};
use crate::render;

/// Upper bound on grid cells; finer requests are coarsened with a warning.
const MAX_CELLS: usize = 4_000_000;
/// Upper bound on profile points per cell (longer paths use a coarser step).
const MAX_PROFILE_POINTS: usize = 800;
const M_PER_DEG_LAT: f64 = 111_320.0;

/// The computed grid, kept by the app for hover read-outs.
#[derive(Debug, Clone)]
pub struct CoverageGrid {
    pub values: Vec<f32>,
    pub width: usize,
    pub height: usize,
    pub west: f64,
    pub north: f64,
    pub dlon: f64,
    pub dlat: f64,
}

impl CoverageGrid {
    pub fn value_at(&self, lat: f64, lon: f64) -> Option<f64> {
        let x = ((lon - self.west) / self.dlon).floor();
        let y = ((self.north - lat) / self.dlat).floor();
        if x < 0.0 || y < 0.0 || x >= self.width as f64 || y >= self.height as f64 {
            return None;
        }
        let v = self.values[y as usize * self.width + x as usize];
        v.is_finite().then_some(v as f64)
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CoverageSummary {
    pub image_data_url: String,
    /// `[west, south, east, north]`.
    pub bounds: [f64; 4],
    pub width: usize,
    pub height: usize,
    pub compute_ms: u128,
    pub covered_pct: f64,
    pub model: String,
    pub output: CoverageOutput,
    pub unit: &'static str,
    pub legend: Vec<LegendEntry>,
    pub site_id: String,
    pub frequency_mhz: f64,
    pub warnings: Vec<String>,
}

/// Ray-casting point-in-polygon on lon/lat.
fn in_polygon(poly: &[LatLon], p: LatLon) -> bool {
    let mut inside = false;
    let mut j = poly.len() - 1;
    for i in 0..poly.len() {
        let (a, b) = (poly[i], poly[j]);
        if (a.lat > p.lat) != (b.lat > p.lat) && p.lon < (b.lon - a.lon) * (p.lat - a.lat) / (b.lat - a.lat) + a.lon {
            inside = !inside;
        }
        j = i;
    }
    inside
}

struct Scratch {
    terrain: Vec<f64>,
    clutter: Vec<f64>,
    classes: Vec<u8>,
    heights: Vec<f64>,
    distances: Vec<f64>,
}

pub fn run(
    project: &Project,
    ds: &Dataset,
    lib: &AntennaLibrary,
    progress: impl Fn(Progress) + Send + Sync,
) -> anyhow::Result<(CoverageGrid, CoverageSummary)> {
    let started = Instant::now();
    let cov = &project.coverage;
    let site_id = cov.site_id.as_deref().context("Choose a transmitting site")?;
    let site = project.site(site_id).context("Coverage site not found")?;
    let f = cov.frequency_mhz;
    if f.is_nan() || f <= 0.0 {
        bail!("Set a coverage frequency");
    }
    let tx = LatLon::new(site.lat, site.lon);
    if !ds.contains(tx) {
        bail!("{} is outside the loaded terrain — fetch data again", site.name);
    }
    let mut warnings = Vec::new();

    // Region: polygon (if any) ∩ radius circle ∩ loaded data.
    let radius_m = (cov.max_radius_km * 1000.0).max(100.0);
    let poly = area_polygon(project);
    let use_poly = poly.len() >= 3;
    let circle = BBox::from_polygon(&[0.0, 90.0, 180.0, 270.0].map(|b| geodesy::destination(tx, b, radius_m)))
        .context("Invalid radius")?;
    let mut region = circle.intersection(&ds.dem.bbox()).context("Coverage area is outside the loaded terrain")?;
    if use_poly {
        let pb = BBox::from_polygon(&poly).context("Invalid study polygon")?;
        region = region.intersection(&pb).context("The coverage radius does not reach the study area")?;
    }

    // Grid.
    let lat0 = (region.min_lat + region.max_lat) / 2.0;
    let mut res_m = cov.resolution_m.max(ds.resolution.approx_pixel_m() * 0.5).max(1.0);
    let cells = |r: f64| {
        let w = ((region.max_lon - region.min_lon) * M_PER_DEG_LAT * lat0.to_radians().cos() / r).ceil() as usize;
        let h = ((region.max_lat - region.min_lat) * M_PER_DEG_LAT / r).ceil() as usize;
        (w.max(1), h.max(1))
    };
    let (mut width, mut height) = cells(res_m);
    if width * height > MAX_CELLS {
        res_m *= ((width * height) as f64 / MAX_CELLS as f64).sqrt();
        (width, height) = cells(res_m);
        warnings.push(format!("Grid coarsened to {res_m:.0} m to stay under {} M cells", MAX_CELLS / 1_000_000));
    }
    let dlon = (region.max_lon - region.min_lon) / width as f64;
    let dlat = (region.max_lat - region.min_lat) / height as f64;

    // Models and constants.
    let method = choose_method(project.env.model, f, false);
    let model = PathModel::new(&project.env, method, f, cov.polarization, site.antenna_height_m, cov.rx_height_m);
    let pattern = lib.pattern(site.antenna_id.as_deref(), f)?;
    if let Some(spec) = site.antenna_id.as_deref().and_then(|id| lib.spec(id)) {
        if !spec.covers(f) {
            warnings
                .push(format!("Antenna '{}' is specified for {:.0}–{:.0} MHz", spec.name, spec.min_mhz, spec.max_mhz));
        }
    }
    let tx_ground = ds.elevation_at(site.lat, site.lon).unwrap_or(0.0);
    let tx_amsl = tx_ground + site.antenna_height_m;
    let tx_terminal = terminal_loss(project, ds.landcover_at(tx), site.antenna_height_m, f);
    let tx_power_after_losses = site.tx_power_dbm - site.line_loss_db - site.other_loss_db;
    let k = model.k_factor;
    let pixel_m = ds.resolution.approx_pixel_m();
    let need_distances = method == Method::Microwave;

    let done = AtomicUsize::new(0);
    let report_every = (height / 50).max(1);
    progress(Progress {
        stage: "coverage".into(),
        done: 0,
        total: height,
        bytes: 0,
        message: "Computing coverage…".into(),
    });

    let mut values = vec![f32::NAN; width * height];
    values.par_chunks_mut(width).enumerate().for_each_init(
        || Scratch {
            terrain: Vec::new(),
            clutter: Vec::new(),
            classes: Vec::new(),
            heights: Vec::new(),
            distances: Vec::new(),
        },
        |s, (y, row)| {
            let lat = region.max_lat - (y as f64 + 0.5) * dlat;
            for (x, out) in row.iter_mut().enumerate() {
                let p = LatLon::new(lat, region.min_lon + (x as f64 + 0.5) * dlon);
                if use_poly && !in_polygon(&poly, p) {
                    continue;
                }
                let d = geodesy::haversine_m(tx, p);
                if d > radius_m || d < 1.0 {
                    continue;
                }
                let n = ((d / pixel_m).ceil() as usize + 1).clamp(2, MAX_PROFILE_POINTS);
                let info = profile::terrain_profile_n_into(&mut s.terrain, &ds.dem, tx, p, n);
                if info.all_nodata {
                    continue;
                }
                let step = info.step_m;
                clutter_along(project, ds, tx, p, n, step, &mut s.classes, &mut s.clutter);
                s.heights.clear();
                s.heights.extend(s.terrain.iter().zip(&s.clutter).map(|(t, c)| t + c));
                let rx_ground = s.terrain[n - 1];
                let rx_amsl = rx_ground + cov.rx_height_m;
                if need_distances {
                    s.distances.clear();
                    s.distances.extend((0..n).map(|i| i as f64 * step));
                }
                let Some(loss) = model.loss(&s.heights, &s.distances, step, tx_amsl, rx_amsl) else {
                    continue;
                };
                let rx_class = s.classes.get(n - 1).copied().unwrap_or(0);
                let path_loss = loss.total_db + tx_terminal + terminal_loss(project, rx_class, cov.rx_height_m, f);
                let az = geodesy::initial_bearing_deg(tx, p);
                let el = elevation_angle_deg(tx_amsl, rx_amsl, d, k);
                let g_tx =
                    pattern.gain_toward(site.azimuth_deg, site.downtilt_deg, az, el, InterpolationMethod::Weighted);
                let iso_rx = tx_power_after_losses + g_tx - path_loss;
                *out = match cov.output {
                    CoverageOutput::RxDbm => iso_rx + cov.rx_gain_dbi - cov.rx_line_loss_db,
                    CoverageOutput::FieldDbuvm => pathless_rf::budget::field_strength_dbuv_m_from_dbm(iso_rx, f, 0.0),
                    CoverageOutput::PathLossDb => path_loss,
                } as f32;
            }
            let c = done.fetch_add(1, Ordering::Relaxed) + 1;
            if c.is_multiple_of(report_every) {
                progress(Progress {
                    stage: "coverage".into(),
                    done: c,
                    total: height,
                    bytes: 0,
                    message: format!("Computing coverage… row {c} of {height}"),
                });
            }
        },
    );

    let higher_is_better = cov.output != CoverageOutput::PathLossDb;
    let (image_data_url, covered) = render::coverage_png(&values, width, height, &cov.legend, higher_is_better)?;
    let grid = CoverageGrid { values, width, height, west: region.min_lon, north: region.max_lat, dlon, dlat };
    let summary = CoverageSummary {
        image_data_url,
        bounds: [region.min_lon, region.min_lat, region.max_lon, region.max_lat],
        width,
        height,
        compute_ms: started.elapsed().as_millis(),
        covered_pct: covered * 100.0,
        model: method.label().to_string(),
        output: cov.output,
        unit: cov.output.unit(),
        legend: cov.legend.clone(),
        site_id: site.id.clone(),
        frequency_mhz: f,
        warnings,
    };
    Ok((grid, summary))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn point_in_polygon() {
        let sq = [LatLon::new(0.0, 0.0), LatLon::new(0.0, 1.0), LatLon::new(1.0, 1.0), LatLon::new(1.0, 0.0)];
        assert!(in_polygon(&sq, LatLon::new(0.5, 0.5)));
        assert!(!in_polygon(&sq, LatLon::new(1.5, 0.5)));
    }
}
