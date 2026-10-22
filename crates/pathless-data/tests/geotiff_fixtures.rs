//! Offline decoding tests against small responses captured from the real services.

use pathless_data::geotiff::{read_elevation_geotiff, read_geotiff};
use pathless_data::{nlcd, LatLon};

const DEM: &[u8] = include_bytes!("data/dem_boulder_50.tif");
const OCEAN: &[u8] = include_bytes!("data/dem_ocean_sparse.tif");
const NLCD: &[u8] = include_bytes!("data/nlcd_boulder.tif");

#[test]
fn usgs_3dep_little_endian_tiled_lzw_f32() {
    // bbox -105.30,39.95,-105.25,40.00 at 50x50 -> 0.001 deg pixels, tiepoint+pixelscale
    let r = read_elevation_geotiff(DEM).unwrap();
    assert_eq!((r.width, r.height), (50, 50));
    assert!((r.geo.origin_lon - -105.30).abs() < 1e-9);
    assert!((r.geo.origin_lat - 40.00).abs() < 1e-9);
    assert!((r.geo.px_lon - 0.001).abs() < 1e-12);
    assert!((r.geo.px_lat + 0.001).abs() < 1e-12);
    let (lo, hi) = r.min_max().unwrap();
    assert!(lo > 1550.0 && lo < 1700.0, "min {lo}");
    assert!(hi > 2400.0 && hi < 2700.0, "max {hi}");
    assert_eq!(r.nodata_count(), 0);
    // west (foothills) higher than east (plains)
    let west = r.sample_bilinear(LatLon::new(39.975, -105.295)).unwrap();
    let east = r.sample_bilinear(LatLon::new(39.975, -105.255)).unwrap();
    assert!(west > east + 200.0, "west {west} east {east}");
}

#[test]
fn usgs_3dep_sparse_ocean_is_nodata() {
    let r = read_elevation_geotiff(OCEAN).unwrap();
    assert_eq!((r.width, r.height), (100, 100));
    assert_eq!(r.nodata_count(), 100 * 100);
}

#[test]
fn nlcd_big_endian_palette_model_transformation() {
    let g = read_geotiff::<u8>(NLCD, 0).unwrap();
    assert!(g.is_palette);
    let r = g.raster;
    assert_eq!((r.width, r.height), (159, 199));
    assert!((r.geo.origin_lon - -105.306458).abs() < 1e-5);
    assert!((r.geo.origin_lat - 40.003586).abs() < 1e-5);
    assert!((r.geo.px_lon - 0.000395925).abs() < 1e-8);
    assert!(r.geo.px_lat < 0.0);
    assert_eq!(r.nodata, Some(0));
    // all values must be valid NLCD codes or 0
    let mut seen = std::collections::BTreeSet::new();
    for &v in &r.data {
        assert!(v == 0 || nlcd::class(v).is_some(), "bad class {v}");
        seen.insert(v);
    }
    // Boulder: developed + forest + grassland present
    assert!(seen.contains(&42) && seen.contains(&71) && seen.iter().any(|v| (21..=24).contains(v)), "{seen:?}");
    // pixel in the bottom row of tiles (tile 192 rows tall, image 199) decoded
    assert!(r.data[198 * 159..].iter().any(|&v| v != 0));
}
