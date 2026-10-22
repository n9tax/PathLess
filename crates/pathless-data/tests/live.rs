//! Live-network tests against USGS 3DEP and MRLC NLCD. Ignored by default:
//! `cargo test -p pathless-data -- --ignored --nocapture`

use std::sync::atomic::{AtomicUsize, Ordering};

use pathless_data::profile::{landcover_profile, terrain_profile, terrain_profile_into};
use pathless_data::{nlcd, BBox, DataManager, DemResolution, LatLon};

fn manager(tag: &str) -> DataManager {
    let dir = std::env::temp_dir().join(format!("pathless-live-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    DataManager::new(Some(dir)).unwrap()
}

#[tokio::test]
#[ignore = "network"]
async fn live_boulder_elevation_landcover_profile() {
    let m = manager("boulder");
    let area = BBox::new(-105.30, 39.95, -105.25, 40.00);

    // --- elevation, 1/3 arc-second ---
    let plan = m.plan_elevation(&area, DemResolution::ArcSecThird);
    println!(
        "elevation plan: {} tiles, {} cached, est {} bytes, mosaic {}x{}",
        plan.tiles.len(),
        plan.cached,
        plan.estimated_bytes,
        plan.mosaic_width,
        plan.mosaic_height
    );
    assert_eq!(plan.cached, 0);
    let calls = AtomicUsize::new(0);
    let t0 = std::time::Instant::now();
    let dem = m
        .fetch_elevation(&area, DemResolution::ArcSecThird, |p| {
            calls.fetch_add(1, Ordering::Relaxed);
            println!("  [{}] {}/{} {} B - {}", p.stage, p.done, p.total, p.bytes, p.message);
        })
        .await
        .unwrap();
    println!("elevation fetched in {:.1?}", t0.elapsed());
    assert!(calls.load(Ordering::Relaxed) >= 3);
    assert_eq!((dem.width, dem.height), (540, 540));
    let (lo, hi) = dem.min_max().unwrap();
    println!("elevation range {lo:.1} .. {hi:.1} m, nodata {}", dem.nodata_count());
    assert!(lo > 1550.0 && lo < 1700.0);
    assert!(hi > 2400.0 && hi < 2700.0);
    assert_eq!(dem.nodata_count(), 0);

    // second fetch is served from cache
    assert_eq!(m.plan_elevation(&area, DemResolution::ArcSecThird).to_download, 0);
    let t1 = std::time::Instant::now();
    let dem2 = m.fetch_elevation(&area, DemResolution::ArcSecThird, |_| {}).await.unwrap();
    println!("cached re-fetch in {:.1?}", t1.elapsed());
    assert_eq!(dem.data.len(), dem2.data.len());
    assert!(dem.data.iter().zip(&dem2.data).all(|(a, b)| a == b));

    // --- profile west (foothills) -> east (plains) ---
    let a = LatLon::new(39.975, -105.299);
    let b = LatLon::new(39.975, -105.251);
    let p = terrain_profile(&dem, a, b, 30.0);
    println!("profile: {} pts, step {:.2} m, {:.0} m", p.elevations_m.len(), p.step_m, p.distance_m);
    for i in (0..p.elevations_m.len()).step_by(p.elevations_m.len() / 10) {
        println!("  {:7.0} m  {:7.1} m", p.distances_m[i], p.elevations_m[i]);
    }
    let first = p.elevations_m[0];
    let last = *p.elevations_m.last().unwrap();
    assert!(first > 2000.0, "west end {first}");
    assert!(last < 1750.0, "east end {last}");
    assert_eq!(p.nodata_filled, 0);
    let mut buf = Vec::new();
    let info = terrain_profile_into(&mut buf, &dem, a, b, 30.0);
    assert_eq!(info.n, p.elevations_m.len());

    // --- land cover ---
    let lplan = m.plan_landcover(&area);
    println!("landcover plan: {} tiles, mosaic {}x{}", lplan.tiles.len(), lplan.mosaic_width, lplan.mosaic_height);
    let lc = m
        .fetch_landcover(&area, |p| println!("  [{}] {}/{} {} B - {}", p.stage, p.done, p.total, p.bytes, p.message))
        .await
        .unwrap();
    assert_eq!((lc.width, lc.height), (180, 180));
    let zeros = lc.data.iter().filter(|&&v| v == 0).count();
    assert_eq!(zeros, 0, "land cover has nodata pixels");
    let mut hist = std::collections::BTreeMap::new();
    for &v in &lc.data {
        *hist.entry(v).or_insert(0usize) += 1;
    }
    for (k, n) in &hist {
        println!("  {:2} {:30} {:5.1}%", k, nlcd::class_name(*k), 100.0 * *n as f64 / lc.data.len() as f64);
        assert!(nlcd::class(*k).is_some());
    }
    let classes = landcover_profile(&lc, &p.points);
    println!("profile classes (every 10th): {:?}", classes.iter().step_by(10).collect::<Vec<_>>());
    assert!(classes.iter().all(|&c| c != 0));

    println!("cache size {} bytes in {}", m.cache_size_bytes(), m.cache_dir().display());
    m.clear_cache().unwrap();
    assert_eq!(m.cache_size_bytes(), 0);
    let _ = std::fs::remove_dir_all(m.cache_dir());
}

#[tokio::test]
#[ignore = "network"]
async fn live_outside_coverage_is_nodata_not_error() {
    let m = manager("ocean");
    // Pacific, far offshore: 3DEP returns nodata, NLCD returns 404 InvalidSubsetting.
    let area = BBox::new(-140.0, 30.0, -139.9, 30.1);
    let dem = m.fetch_elevation(&area, DemResolution::ArcSec1, |_| {}).await.unwrap();
    assert_eq!(dem.nodata_count(), dem.data.len());
    let lc = m.fetch_landcover(&area, |_| {}).await.unwrap();
    assert!(lc.data.iter().all(|&v| v == 0));
    // empty tiles are cached as tiny header-only files
    let size = m.cache_size_bytes();
    println!("ocean cache size {size} bytes");
    assert!(size < 4096);
    let _ = std::fs::remove_dir_all(m.cache_dir());
}

#[tokio::test]
#[ignore = "network"]
async fn live_tile_boundary_mosaic_is_seamless() {
    // Area straddling a 1/3" tile corner at (-105.25, 40.0): 4 tiles. Compare the mosaic
    // against a single direct exportImage request on the same pixel grid.
    let m = manager("seam");
    let area = BBox::new(-105.26, 39.99, -105.24, 40.01);
    assert_eq!(m.plan_elevation(&area, DemResolution::ArcSecThird).tiles.len(), 4);
    let dem = m.fetch_elevation(&area, DemResolution::ArcSecThird, |_| {}).await.unwrap();
    assert_eq!(dem.nodata_count(), 0);
    let b = dem.bbox();
    let url = format!(
        "https://elevation.nationalmap.gov/arcgis/rest/services/3DEPElevation/ImageServer/exportImage?bbox={:.10},{:.10},{:.10},{:.10}&bboxSR=4326&imageSR=4326&size={},{}&format=tiff&pixelType=F32&interpolation=RSP_BilinearInterpolation&noData=-9999&f=image",
        b.min_lon, b.min_lat, b.max_lon, b.max_lat, dem.width, dem.height
    );
    let bytes = reqwest::Client::builder()
        .user_agent(pathless_data::USER_AGENT)
        .build()
        .unwrap()
        .get(url)
        .send()
        .await
        .unwrap()
        .bytes()
        .await
        .unwrap();
    let reference = pathless_data::geotiff::read_elevation_geotiff(&bytes).unwrap();
    assert_eq!((reference.width, reference.height), (dem.width, dem.height));
    assert!((reference.geo.origin_lon - dem.geo.origin_lon).abs() < 1e-9);
    assert!((reference.geo.origin_lat - dem.geo.origin_lat).abs() < 1e-9);
    let max_diff = dem.data.iter().zip(&reference.data).map(|(a, b)| (a - b).abs()).fold(0f32, f32::max);
    let mean_diff =
        dem.data.iter().zip(&reference.data).map(|(a, b)| (a - b).abs() as f64).sum::<f64>() / dem.data.len() as f64;
    println!("mosaic vs direct request: max |diff| {max_diff:.3} m, mean {mean_diff:.4} m");
    assert!(max_diff < 1.0, "mosaic misaligned: max diff {max_diff}");
    let _ = std::fs::remove_dir_all(m.cache_dir());
}

#[tokio::test]
#[ignore = "network"]
async fn live_alaska_hawaii_landcover_and_dem() {
    let m = manager("akhi");
    // Anchorage (AK 2016 coverage) and Honolulu (HI 2001 coverage)
    for (name, area) in [
        ("Anchorage", BBox::new(-149.92, 61.19, -149.86, 61.23)),
        ("Honolulu", BBox::new(-157.86, 21.29, -157.82, 21.32)),
    ] {
        let lc = m.fetch_landcover(&area, |_| {}).await.unwrap();
        let valid = lc.data.iter().filter(|&&v| v != 0).count();
        let mut classes: Vec<u8> = lc.data.iter().copied().filter(|&v| v != 0).collect();
        classes.sort_unstable();
        classes.dedup();
        println!(
            "{name}: land cover {}x{}, {:.1}% valid, classes {classes:?}",
            lc.width,
            lc.height,
            100.0 * valid as f64 / lc.data.len() as f64
        );
        assert!(valid as f64 > 0.5 * lc.data.len() as f64);
        assert!(classes.iter().all(|&c| nlcd::class(c).is_some()));
        let dem = m.fetch_elevation(&area, DemResolution::ArcSec1, |_| {}).await.unwrap();
        let mm = dem.min_max().unwrap();
        println!("{name}: DEM {}x{}, {:.0}..{:.0} m, nodata {}", dem.width, dem.height, mm.0, mm.1, dem.nodata_count());
        assert!(dem.nodata_count() < dem.data.len() / 2);
    }
    let _ = std::fs::remove_dir_all(m.cache_dir());
}
