//! Point-to-point link analysis: path profile, clearance, path loss, two-way
//! link budget and (for microwave) ITU-R P.530 availability.

use anyhow::{bail, Context as _};
use pathless_data::{geodesy, profile, LatLon};
use pathless_rf::antenna::InterpolationMethod;
use pathless_rf::budget::{field_strength_dbuv_m_from_dbm, link_budget, LinkBudgetInput};
use pathless_rf::diffraction::{los_clearance, PathParams};
use pathless_rf::itu::availability::{microwave_availability, MicrowaveLinkInput};
use pathless_rf::itu::p530::MultipathModel;
use pathless_rf::itu::p837::RainRate;
use serde::Serialize;

use crate::antennas::AntennaLibrary;
use crate::clutter;
use crate::dataset::Dataset;
use crate::project::{Link, Pol, Project, Site};
use crate::propagation::{choose_method, elevation_angle_deg, Method, PathModel};

/// Clutter within this distance of a terminal is left out of the profile; the
/// terminal's own surroundings are handled by the P.2108 terminal loss.
pub const TERMINAL_CLEAR_M: f64 = 100.0;
/// Profiles are sampled at the DEM resolution but never with more points than this.
const MAX_PROFILE_POINTS: usize = 4000;
/// Microwave availability is evaluated from this frequency up.
const MICROWAVE_AVAILABILITY_MHZ: f64 = 3_000.0;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EndReport {
    pub name: String,
    pub ground_m: f64,
    pub antenna_amsl_m: f64,
    /// Azimuth toward the far end, degrees true.
    pub azimuth_deg: f64,
    /// Elevation angle toward the far end antenna (incl. earth curvature), degrees.
    pub elevation_deg: f64,
    /// Antenna gain in the direction of the path, dBi.
    pub antenna_gain_dbi: f64,
    pub clutter_class: u8,
    pub terminal_clutter_db: f64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LossReport {
    pub fspl_db: f64,
    pub itm_db: Option<f64>,
    pub itm_mode: Option<String>,
    pub diffraction_db: Option<f64>,
    pub gas_db: f64,
    pub clutter_db: f64,
    pub total_db: f64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Budget {
    pub eirp_dbm: f64,
    pub rx_gain_dbi: f64,
    pub rx_level_dbm: f64,
    pub field_dbuvm: f64,
    pub fade_margin_db: f64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Clearance {
    pub worst_index: Option<usize>,
    /// Worst clearance / first Fresnel radius (1.0 = fully clear F1, < 0 = obstructed).
    pub worst_ratio: f64,
    pub worst_distance_km: f64,
    pub line_of_sight: bool,
    pub fresnel60_clear: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Microwave {
    pub r001: f64,
    pub rain_atten001_db: f64,
    pub multipath_unavail_pct: f64,
    pub rain_unavail_pct: f64,
    pub availability_pct: f64,
    pub outage_min_per_year: f64,
    pub target_pct: f64,
    pub meets_target: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProfileData {
    pub distance_m: Vec<f64>,
    pub terrain_m: Vec<f64>,
    pub clutter_m: Vec<f64>,
    pub bulge_m: Vec<f64>,
    pub los_m: Vec<f64>,
    pub fresnel_m: Vec<f64>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LinkReport {
    pub link_id: String,
    pub site_a: EndReport,
    pub site_b: EndReport,
    pub distance_km: f64,
    pub frequency_mhz: f64,
    pub model: String,
    pub loss: LossReport,
    #[serde(rename = "budgetAB")]
    pub budget_ab: Budget,
    #[serde(rename = "budgetBA")]
    pub budget_ba: Budget,
    pub clearance: Clearance,
    pub microwave: Option<Microwave>,
    pub warnings: Vec<String>,
    pub profile: ProfileData,
}

/// Clutter heights along a profile from the land-cover raster, zeroed near
/// both terminals.
#[allow(clippy::too_many_arguments)]
pub fn clutter_along(
    project: &Project,
    ds: &Dataset,
    a: LatLon,
    b: LatLon,
    n: usize,
    step_m: f64,
    classes: &mut Vec<u8>,
    out: &mut Vec<f64>,
) {
    out.clear();
    out.resize(n, 0.0);
    let Some(lc) = ds.landcover.as_ref().filter(|_| project.data.use_clutter) else {
        return;
    };
    profile::landcover_profile_n_into(classes, lc, a, b, n);
    let table = project.env.clutter_table();
    let total = step_m * (n - 1) as f64;
    for (i, (h, &c)) in out.iter_mut().zip(classes.iter()).enumerate() {
        let d = step_m * i as f64;
        if d > TERMINAL_CLEAR_M && total - d > TERMINAL_CLEAR_M {
            *h = table[c as usize] as f64;
        }
    }
}

/// P.2108 terminal loss for a terminal at `h_agl` in land-cover `class`.
pub fn terminal_loss(project: &Project, class: u8, h_agl: f64, f_mhz: f64) -> f64 {
    if !project.env.terminal_clutter_loss || !clutter::is_obstructing_class(class) {
        return 0.0;
    }
    clutter::terminal_clutter_loss_db(f_mhz, h_agl, project.env.clutter_height(class), 27.0)
}

fn rx_gain_and_warn(
    lib: &AntennaLibrary,
    site: &Site,
    f_mhz: f64,
    toward_az: f64,
    toward_el: f64,
    align: bool,
    warnings: &mut Vec<String>,
) -> anyhow::Result<f64> {
    let pat = lib.pattern(site.antenna_id.as_deref(), f_mhz)?;
    if let Some(spec) = site.antenna_id.as_deref().and_then(|id| lib.spec(id)) {
        if !spec.covers(f_mhz) {
            warnings.push(format!(
                "{}: antenna '{}' is specified for {:.0}–{:.0} MHz, not {:.0} MHz",
                site.name, spec.name, spec.min_mhz, spec.max_mhz, f_mhz
            ));
        }
    }
    Ok(if align {
        pat.gain_dbi
    } else {
        pat.gain_toward(site.azimuth_deg, site.downtilt_deg, toward_az, toward_el, InterpolationMethod::Weighted)
    })
}

fn rain_rate(link: &Link) -> RainRate {
    match link.custom_r001 {
        Some(r) if r > 0.0 => RainRate::Custom { r001: r },
        _ => RainRate::Zone(link.rain_zone.chars().next().unwrap_or('K')),
    }
}

fn std_dev(v: &[f64]) -> f64 {
    let n = v.len().max(1) as f64;
    let mean = v.iter().sum::<f64>() / n;
    (v.iter().map(|x| (x - mean).powi(2)).sum::<f64>() / n).sqrt()
}

pub fn analyze(project: &Project, link_id: &str, ds: &Dataset, lib: &AntennaLibrary) -> anyhow::Result<LinkReport> {
    let link = project.link(link_id).context("Unknown link")?;
    let sa = project.site(&link.site_a).context("Link site A missing")?;
    let sb = project.site(&link.site_b).context("Link site B missing")?;
    if sa.id == sb.id {
        bail!("A link needs two different sites");
    }
    let f = link.frequency_mhz;
    if f.is_nan() || f <= 0.0 {
        bail!("Set a link frequency");
    }
    let (pa, pb) = (LatLon::new(sa.lat, sa.lon), LatLon::new(sb.lat, sb.lon));
    for (s, p) in [(sa, pa), (sb, pb)] {
        if !ds.contains(p) {
            bail!("{} is outside the loaded terrain — fetch data again", s.name);
        }
    }
    let mut warnings = Vec::new();

    // Geometry and profile.
    let geo = geodesy::geodesic_inverse(pa, pb);
    let d_m = geo.distance_m;
    if d_m < 10.0 {
        bail!("Sites are too close together");
    }
    let pixel_m = ds.resolution.approx_pixel_m();
    let n = ((d_m / pixel_m).ceil() as usize + 1).clamp(2, MAX_PROFILE_POINTS);
    let mut terrain = Vec::with_capacity(n);
    let info = profile::terrain_profile_n_into(&mut terrain, &ds.dem, pa, pb, n);
    let step = info.step_m;
    if info.nodata_filled > 0 {
        warnings.push(format!("{} profile points had no elevation data and were interpolated", info.nodata_filled));
    }
    let mut classes = Vec::with_capacity(n);
    let mut clutter_h = Vec::with_capacity(n);
    clutter_along(project, ds, pa, pb, n, step, &mut classes, &mut clutter_h);
    let heights: Vec<f64> = terrain.iter().zip(&clutter_h).map(|(t, c)| t + c).collect();
    let distances: Vec<f64> = (0..n).map(|i| i as f64 * step).collect();

    let (ga, gb) = (terrain[0], terrain[n - 1]);
    let (amsl_a, amsl_b) = (ga + sa.antenna_height_m, gb + sb.antenna_height_m);
    let k = project.env.k_factor.clamp(0.3, 10.0);

    // Clearance (line of sight and first Fresnel zone).
    let clr = los_clearance(&distances, &heights, &PathParams::new(amsl_a, amsl_b, f, k))?;
    let bulge: Vec<f64> = clr.points.iter().map(|p| p.bulge_m).collect();
    let los: Vec<f64> = clr.points.iter().map(|p| p.los_height_m).collect();
    let fresnel: Vec<f64> = clr.points.iter().map(|p| p.fresnel_radius_m).collect();

    // Path loss: compute every applicable model, then pick one for the budget.
    let method = choose_method(project.env.model, f, true);
    let model = PathModel::new(&project.env, method, f, link.polarization, sa.antenna_height_m, sb.antenna_height_m);
    let fspl = model.fspl_db(d_m);
    let gas = model.gas_db_per_km * d_m / 1000.0;
    let diffraction = model.diffraction_db(&distances, &heights, amsl_a, amsl_b);
    let itm = if (crate::propagation::ITM_MIN_MHZ..=crate::propagation::ITM_MAX_MHZ).contains(&f) {
        match model.itm_loss(&heights, step) {
            Ok(o) => {
                if o.warnings != 0 && method == Method::Itm {
                    for w in pathless_itm::warnings::describe(o.warnings) {
                        warnings.push(format!("ITM: {w}"));
                    }
                }
                Some(o)
            }
            Err(e) => {
                warnings.push(format!("ITM could not be evaluated: {e}"));
                None
            }
        }
    } else {
        None
    };
    let (model_label, base_loss) = match (method, &itm) {
        (Method::Itm, Some(o)) => (Method::Itm.label(), o.loss_db + gas),
        _ => (Method::Microwave.label(), fspl + diffraction.unwrap_or(0.0) + gas),
    };

    // Terminal clutter.
    let class_a = ds.landcover_at(pa);
    let class_b = ds.landcover_at(pb);
    let tc_a = terminal_loss(project, class_a, sa.antenna_height_m, f);
    let tc_b = terminal_loss(project, class_b, sb.antenna_height_m, f);
    let total = base_loss + tc_a + tc_b;

    // Antenna gains along the path.
    let az_ab = geo.azimuth1_deg;
    let az_ba = (geo.azimuth2_deg + 180.0).rem_euclid(360.0);
    let el_ab = elevation_angle_deg(amsl_a, amsl_b, d_m, k);
    let el_ba = elevation_angle_deg(amsl_b, amsl_a, d_m, k);
    let gain_a = rx_gain_and_warn(lib, sa, f, az_ab, el_ab, link.align_antennas, &mut warnings)?;
    let gain_b = rx_gain_and_warn(lib, sb, f, az_ba, el_ba, link.align_antennas, &mut warnings)?;

    let budget = |tx: &Site, rx: &Site, g_tx: f64, g_rx: f64| {
        let r = link_budget(&LinkBudgetInput {
            tx_power_dbm: tx.tx_power_dbm,
            tx_line_loss_db: tx.line_loss_db,
            tx_antenna_gain_dbi: g_tx,
            rx_antenna_gain_dbi: g_rx,
            rx_line_loss_db: rx.line_loss_db,
            other_losses_db: tx.other_loss_db + rx.other_loss_db,
            path_loss_db: total,
            rx_sensitivity_dbm: rx.rx_sensitivity_dbm,
        });
        Budget {
            eirp_dbm: r.eirp_dbm,
            rx_gain_dbi: g_rx,
            rx_level_dbm: r.rx_level_dbm,
            field_dbuvm: field_strength_dbuv_m_from_dbm(r.isotropic_rx_level_dbm, f, 0.0),
            fade_margin_db: r.fade_margin_db,
        }
    };
    let budget_ab = budget(sa, sb, gain_a, gain_b);
    let budget_ba = budget(sb, sa, gain_b, gain_a);

    // Microwave availability.
    let microwave = if f >= MICROWAVE_AVAILABILITY_MHZ {
        let margin = budget_ab.fade_margin_db.min(budget_ba.fade_margin_db);
        if margin <= 0.0 {
            warnings.push("Fade margin is negative — the link does not close; availability not computed".into());
            None
        } else {
            let input = MicrowaveLinkInput {
                frequency_ghz: f / 1000.0,
                path_length_km: d_m / 1000.0,
                tx_antenna_amsl_m: amsl_a,
                rx_antenna_amsl_m: amsl_b,
                fade_margin_db: margin,
                polarization: match link.polarization {
                    Pol::Vertical => pathless_rf::itu::Polarization::Vertical,
                    Pol::Horizontal => pathless_rf::itu::Polarization::Horizontal,
                },
                rain: rain_rate(link),
                rain_method: Default::default(),
                multipath: MultipathModel::P530v17 { dn1: link.dn1, s_a_m: std_dev(&terrain).max(1.0) },
                worst_month_conversion: Default::default(),
                ignore_multipath_below_5km: true,
            };
            match microwave_availability(&input) {
                Ok(a) => Some(Microwave {
                    r001: a.rain_r001_mm_h,
                    rain_atten001_db: a.rain_a001_db,
                    multipath_unavail_pct: a.multipath_annual_pct,
                    rain_unavail_pct: a.rain_annual_pct,
                    availability_pct: a.availability_pct,
                    outage_min_per_year: a.total_minutes_per_year,
                    target_pct: link.target_availability_pct,
                    meets_target: a.availability_pct >= link.target_availability_pct,
                }),
                Err(e) => {
                    warnings.push(format!("Availability not computed: {e}"));
                    None
                }
            }
        }
    } else {
        None
    };

    let clearance = Clearance {
        worst_index: clr.min_index,
        worst_ratio: clr.min_clearance_ratio.unwrap_or(1.0),
        worst_distance_km: clr.min_distance_m.unwrap_or(0.0) / 1000.0,
        line_of_sight: clr.line_of_sight,
        fresnel60_clear: clr.fresnel_60_clear,
    };
    if !clearance.line_of_sight {
        warnings.push("Path is obstructed (no radio line of sight at this k-factor)".into());
    } else if !clearance.fresnel60_clear {
        warnings.push("Less than 60% of the first Fresnel zone is clear".into());
    }
    if d_m < 1000.0 && method == Method::Itm {
        warnings.push("ITM is intended for paths of 1 km or more".into());
    }
    if project.data.use_clutter && ds.landcover.is_none() {
        warnings.push("Land cover was not loaded; clutter ignored".into());
    }

    let end = |s: &Site, ground: f64, amsl: f64, az: f64, el: f64, g: f64, class: u8, tc: f64| EndReport {
        name: s.name.clone(),
        ground_m: ground,
        antenna_amsl_m: amsl,
        azimuth_deg: az,
        elevation_deg: el,
        antenna_gain_dbi: g,
        clutter_class: class,
        terminal_clutter_db: tc,
    };

    Ok(LinkReport {
        link_id: link.id.clone(),
        site_a: end(sa, ga, amsl_a, az_ab, el_ab, gain_a, class_a, tc_a),
        site_b: end(sb, gb, amsl_b, az_ba, el_ba, gain_b, class_b, tc_b),
        distance_km: d_m / 1000.0,
        frequency_mhz: f,
        model: model_label.to_string(),
        loss: LossReport {
            fspl_db: fspl,
            itm_db: itm.as_ref().map(|o| o.loss_db),
            itm_mode: itm.as_ref().map(|o| format!("{:?}", o.intermediate.mode)),
            diffraction_db: diffraction,
            gas_db: gas,
            clutter_db: tc_a + tc_b,
            total_db: total,
        },
        budget_ab,
        budget_ba,
        clearance,
        microwave,
        warnings,
        profile: ProfileData {
            distance_m: distances,
            terrain_m: terrain,
            clutter_m: clutter_h,
            bulge_m: bulge,
            los_m: los,
            fresnel_m: fresnel,
        },
    })
}
