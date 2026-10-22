//! End-to-end run against the live USGS / MRLC services (network required):
//! `cargo test -p pathless-engine --release -- --ignored --nocapture`
//! Set PATHLESS_E2E_OUT=<dir> to also write the coverage PNG there.

use base64::Engine as _;
use pathless_engine::antennas::AntennaLibrary;
use pathless_engine::project::Project;
use pathless_engine::{coverage, dataset, link, DataManager};

fn boulder_project() -> Project {
    serde_json::from_value(serde_json::json!({
        "name": "Boulder test",
        "area": [[-105.36, 39.98], [-105.16, 39.98], [-105.16, 40.15], [-105.36, 40.15]],
        "data": {"demResolution": "arcSecThird", "useClutter": true},
        "env": {"climate": "continentalTemperate", "n0": 301, "ground": "average", "epsilon": 15,
                "sigma": 0.005, "kFactor": 1.3333, "timePct": 50, "locationPct": 50, "situationPct": 90,
                "variability": "accidental", "model": "auto",
                "clutterHeights": {"21": 2, "22": 6, "23": 10, "24": 18, "41": 15, "42": 18, "43": 15,
                                   "52": 2, "71": 0.5, "81": 0.5, "82": 1, "90": 12},
                "terminalClutterLoss": true},
        "sites": [
            {"id": "a", "name": "Hilltop", "lat": 40.1306, "lon": -105.2425, "antennaHeightM": 30,
             "antennaId": "dish-0.6m", "azimuthDeg": 0, "downtiltDeg": 0, "txPowerDbm": 27,
             "lineLossDb": 1, "otherLossDb": 0, "rxSensitivityDbm": -75},
            {"id": "b", "name": "Downtown", "lat": 40.0150, "lon": -105.2705, "antennaHeightM": 20,
             "antennaId": "dish-0.6m", "azimuthDeg": 0, "downtiltDeg": 0, "txPowerDbm": 27,
             "lineLossDb": 1, "otherLossDb": 0, "rxSensitivityDbm": -75},
            {"id": "c", "name": "LMR base", "lat": 40.1306, "lon": -105.2425, "antennaHeightM": 30,
             "antennaId": "omni-uhf-6dbi", "azimuthDeg": 0, "downtiltDeg": 0, "txPowerDbm": 47,
             "lineLossDb": 2, "otherLossDb": 0, "rxSensitivityDbm": -110}
        ],
        "links": [
            {"id": "mw", "name": "5.8 GHz hop", "siteA": "a", "siteB": "b", "frequencyMhz": 5800,
             "polarization": "vertical", "rainZone": "K", "customR001": null, "dn1": -200,
             "targetAvailabilityPct": 99.99},
            {"id": "uhf", "name": "UHF", "siteA": "c", "siteB": "b", "frequencyMhz": 450,
             "polarization": "vertical", "rainZone": "K", "customR001": null, "dn1": -200,
             "targetAvailabilityPct": 99.99}
        ],
        "coverage": {"siteId": "c", "frequencyMhz": 450, "polarization": "vertical", "rxHeightM": 1.5,
                     "rxGainDbi": 0, "rxLineLossDb": 0, "resolutionM": 60, "maxRadiusKm": 15,
                     "output": "rxDbm", "legend": [
                        {"value": -60, "color": "#1a9850"}, {"value": -80, "color": "#a6d96a"},
                        {"value": -100, "color": "#fdae61"}, {"value": -110, "color": "#f46d43"}]}
    }))
    .unwrap()
}

#[tokio::test]
#[ignore = "network"]
async fn boulder_end_to_end() {
    let project = boulder_project();
    let dm = DataManager::new(None).unwrap();
    let plan = dataset::plan(&dm, &project).unwrap();
    println!(
        "plan: {} elevation tiles ({} cached), ~{} MB to download",
        plan.elevation.tile_count,
        plan.elevation.cached,
        plan.elevation.estimated_bytes / 1_000_000
    );
    let ds = dataset::fetch(&dm, &project, |p| println!("  [{}] {}", p.stage, p.message)).await.unwrap();
    let st = ds.status();
    println!(
        "dem {}x{} {:?}-{:?} m, landcover {:?}",
        st.elevation.width,
        st.elevation.height,
        st.elevation.min_m,
        st.elevation.max_m,
        st.landcover.as_ref().map(|l| (l.width, l.height))
    );

    let lib = AntennaLibrary::load(None).unwrap();
    let out_dir = std::env::var("PATHLESS_E2E_OUT").ok().map(std::path::PathBuf::from);
    let dump = |name: &str, v: serde_json::Value| {
        if let Some(d) = &out_dir {
            std::fs::write(d.join(name), serde_json::to_string(&v).unwrap()).unwrap();
        }
    };
    dump("project.json", serde_json::to_value(&project).unwrap());
    dump("catalog.json", serde_json::to_value(lib.list()).unwrap());
    dump("status.json", serde_json::to_value(ds.status()).unwrap());
    for id in ["mw", "uhf"] {
        let r = link::analyze(&project, id, &ds, &lib).unwrap();
        dump(&format!("link_{id}.json"), serde_json::to_value(&r).unwrap());
        println!("\n== {} ({} MHz, {:.2} km, model {})", id, r.frequency_mhz, r.distance_km, r.model);
        println!(
            "  A ground {:.1} m az {:.1}° el {:.2}° gain {:.1} dBi clutter class {}",
            r.site_a.ground_m,
            r.site_a.azimuth_deg,
            r.site_a.elevation_deg,
            r.site_a.antenna_gain_dbi,
            r.site_a.clutter_class
        );
        println!(
            "  B ground {:.1} m az {:.1}° el {:.2}° gain {:.1} dBi clutter class {}",
            r.site_b.ground_m,
            r.site_b.azimuth_deg,
            r.site_b.elevation_deg,
            r.site_b.antenna_gain_dbi,
            r.site_b.clutter_class
        );
        println!(
            "  loss: fspl {:.1} itm {:?} diff {:?} gas {:.2} clutter {:.1} => {:.1} dB",
            r.loss.fspl_db,
            r.loss.itm_db.map(|v| (v * 10.0).round() / 10.0),
            r.loss.diffraction_db.map(|v| (v * 10.0).round() / 10.0),
            r.loss.gas_db,
            r.loss.clutter_db,
            r.loss.total_db
        );
        println!(
            "  rx A->B {:.1} dBm margin {:.1} dB; field {:.1} dBuV/m",
            r.budget_ab.rx_level_dbm, r.budget_ab.fade_margin_db, r.budget_ab.field_dbuvm
        );
        println!(
            "  clearance worst {:.0}% F1 at {:.2} km, LOS {}",
            r.clearance.worst_ratio * 100.0,
            r.clearance.worst_distance_km,
            r.clearance.line_of_sight
        );
        if let Some(m) = &r.microwave {
            println!(
                "  availability {:.5}% (rain A0.01 {:.1} dB, R0.01 {} mm/h, outage {:.1} min/yr)",
                m.availability_pct, m.rain_atten001_db, m.r001, m.outage_min_per_year
            );
        }
        for w in &r.warnings {
            println!("  ! {w}");
        }
        assert!(r.loss.total_db > r.loss.fspl_db - 1.0);
        assert!(r.loss.fspl_db > 80.0 && r.loss.fspl_db < 140.0);
    }

    let (grid, sum) = coverage::run(&project, &ds, &lib, |_| {}).unwrap();
    let vals: Vec<f32> = grid.values.iter().copied().filter(|v| v.is_finite()).collect();
    let (mn, mx) = vals.iter().fold((f32::MAX, f32::MIN), |(a, b), &v| (a.min(v), b.max(v)));
    println!(
        "\ncoverage {}x{} in {} ms, {} valid cells, {:.1}..{:.1} dBm, covered {:.1}%, model {}",
        sum.width,
        sum.height,
        sum.compute_ms,
        vals.len(),
        mn,
        mx,
        sum.covered_pct,
        sum.model
    );
    for w in &sum.warnings {
        println!("  ! {w}");
    }
    assert!(vals.len() > sum.width * sum.height / 3);
    assert!(mx < 0.0 && mn > -250.0);
    dump("coverage.json", serde_json::to_value(&sum).unwrap());
    if let Ok(dir) = std::env::var("PATHLESS_E2E_OUT") {
        let b64 = sum.image_data_url.trim_start_matches("data:image/png;base64,");
        let png = base64::engine::general_purpose::STANDARD.decode(b64).unwrap();
        std::fs::write(std::path::Path::new(&dir).join("coverage.png"), png).unwrap();
    }
}
