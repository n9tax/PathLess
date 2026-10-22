# pathless-data

Terrain and clutter acquisition for PathLess. Given a bounding box (usually from the
polygon the user drew), `DataManager` works out which tiles of a fixed global grid are
needed, downloads the missing ones from public web services, caches them on disk and
returns one mosaic `Raster` covering the area. The crate also has geodesy helpers, fast
terrain/land-cover path profiles and the NLCD class legend.

It is pure Rust. GDAL and libtiff are not needed: `src/geotiff.rs` is a small GeoTIFF
decoder built on `weezl` (LZW) and `flate2` (Deflate).

## Usage

```rust
let area = BBox::from_polygon(&polygon).unwrap().expand_m(1_000.0);
let dm = DataManager::new(None)?;                    // ~/.cache/pathless
let plan = dm.plan_elevation(&area, DemResolution::ArcSecThird);   // tiles, cached, bytes
let dem = dm.fetch_elevation(&area, DemResolution::ArcSecThird, |p| ui.progress(p)).await?;
let lc  = dm.fetch_landcover(&area, |p| ui.progress(p)).await?;
let prof = profile::terrain_profile(&dem, tx, rx, 10.0);
let clutter = profile::landcover_profile(&lc, &prof.points);
```

In hot loops (coverage maps), use `profile::terrain_profile_n_into(&mut buf, &dem, a, b, n)`.
It reuses the caller's buffer and makes no allocations. It runs at about 35 ns per point
in release builds.

## Data sources

| Kind | Source | Endpoint | Coverage |
|------|--------|----------|----------|
| Elevation | USGS 3DEP | `elevation.nationalmap.gov/arcgis/rest/services/3DEPElevation/ImageServer/exportImage` | US, AK, HI, PR/VI, Guam, Am. Samoa |
| Land cover | NLCD (MRLC) | `www.mrlc.gov/geoserver/ows` WCS 2.0.1 `GetCoverage` | CONUS 2021, AK 2016, HI 2001, PR 2001 |

**3DEP.** One `exportImage` request is made per cache tile, using `bboxSR=imageSR=4326`.
The bbox is the tile's outer pixel edges, the size is `tile_px²`, the request asks for
`F32` with bilinear resampling, `noData=-9999` and `compression=LZ77` (the server sends an
LZW TIFF). The server returns the best available data (from 1 m lidar down to 1″)
resampled onto our grid. The decoder checks that the georeference matches the grid
exactly, and resamples if it does not. Heights are NAVD88 meters. Offshore or outside
coverage, the server returns sparse or nodata tiles. These become NaN; they are not an
error.

**NLCD.** WCS `GetCoverage` is called with `subsettingCrs=outputCrs=EPSG:4326` and a bbox
padded by 0.005° to avoid nodata at the reprojection edges. The server picks its own
~30 m grid (big-endian, 8-bit palette, georeferenced by `ModelTransformation`). We
resample it with nearest neighbour onto our 1″ grid. Every regional coverage whose
envelope intersects the tile is queried, and the first non-zero class wins. A tile
completely outside a coverage gets HTTP 404 `InvalidSubsetting`, which is treated as
nodata. In 2026-10, MRLC's WCS publishes no per-year land cover newer than 2021. Annual
NLCD is only available there as change summaries (`Annual_NLCD_Smy_LndCov_*`).

**Politeness.** All requests from one manager share a limit of 4 concurrent HTTP
requests. The User-Agent is `PathLess/0.1 (RF path analysis)`. Requests time out after
180 s and are retried with exponential backoff (1/2/4 s) on transport errors, 429 and
5xx. A 3DEP 200 response with a JSON error body is also retried.

## Tile grid

Each grid is defined by integer *pixels per degree* (`ppd`) and integer *tile size*
(`tile_px`):

- Global column `c` covers longitude `[-180 + c/ppd, -180 + (c+1)/ppd]`.
- Global row `r` covers latitude `[90 - (r+1)/ppd, 90 - r/ppd]`.
- Pixels are pixel-is-area.
- Tile `(tx, ty)` covers columns `tx*tile_px ..` and rows `ty*tile_px ..`.

Because every index is an integer, tiles fetched for different polygons line up
exactly. A mosaic is a row-wise `memcpy`, and the live test checks it bit-for-bit against
a single direct request.

| Grid | px/deg | pixel (N–S) | tile | tile px | raw tile |
|------|--------|-------------|------|---------|----------|
| `DemResolution::ArcSec1` | 3600 | 30.9 m | 0.5° | 1800 | 13 MB |
| `DemResolution::ArcSecThird` | 10800 | 10.3 m | 0.125° | 1350 | 7.3 MB |
| `DemResolution::ArcSecNinth` | 32400 | 3.4 m | 0.05° | 1620 | 10.5 MB |
| `DemResolution::Meter1` | 97200 | 1.14 m | 0.02° | 1944 | 15 MB |
| NLCD land cover | 3600 | 30.9 m | 0.25° | 900 | 0.8 MB |

East–west pixel size shrinks with cos(lat). Tiles are kept to 2000 px or less per side,
well under the 3DEP limit of 8000. Smaller tiles waste less download for small polygons.

**Memory guard.** Before downloading, `fetch_*` computes the size of the output mosaic.
If it is larger than `max_mosaic_pixels` (default 400 M, which is 1.6 GB of f32), it
returns `Error::TooLarge { suggestion }`. The suggestion is the finest resolution that
fits, from `DemResolution::finest_within`. Resolution is never coarsened silently.
`TilePlan::exceeds_limit` lets the UI warn before the fetch starts.

## Cache layout

```
<cache_dir>/                       default: ProjectDirs("", "", "pathless").cache_dir()
  3dep/1s/   n39.50000_w105.50000.bin
  3dep/13s/  n39.87500_w105.37500.bin
  3dep/19s/  ...
  3dep/1m/   ...
  nlcd/1s/   n39.75000_w105.50000.bin
```

Each file is named after the tile's south-west corner. It contains `b"PLTILE1\n"`, a u32
LE header length, a JSON `TileHeader` (dtype, size, grid, tx/ty, source, created, empty),
and then a zlib payload:

- **f32:** row-wise delta of the bit patterns, byte-shuffled. This is lossless; 1/3″
  tiles compress to about 50 %.
- **u8:** raw bytes.

A tile with no valid data is written header-only (`empty: true`, about 300 B). Writes are
atomic: a temp file, then fsync, then rename. A corrupt tile found while building a
mosaic is deleted and reported, so the next fetch downloads it again. `clear_cache()`
removes only the `3dep/` and `nlcd/` directories.

Tiles never expire. If a source's content changes (for example a new NLCD year), bump
the cache directory name (`LandCoverSource::id()` or the resolution id) so old tiles are
not mixed in.

## Adding a source

1. Add a variant to `ElevationSource` or `LandCoverSource` in `src/sources/mod.rs`. Give
   it an `id()`, which becomes the cache directory, and a coverage or size estimate. For
   land cover, also give it a `grid()`.
2. Add `src/sources/<name>.rs` with
   `async fn fetch_tile(http: &Http, grid: TileGrid, tx, ty) -> Result<(Vec<T>, u64)>`.
   It returns exactly `tile_px²` row-major values aligned to `grid.tile_bbox(tx, ty)` and
   the number of bytes downloaded. Use NaN (f32) or 0 (u8) for nodata. Decode with
   `geotiff::read_geotiff` and, if the upstream grid differs, use
   `sources::resample_nearest` or a bilinear equivalent for DEMs. Do the decoding inside
   `spawn_blocking`.
3. Add the variant to the `match` in `DataManager::download_tile` and to `clear_cache`.
4. Add an `#[ignore]` live test.

Planned sources (see the TODOs in `sources/mod.rs`):

- **Copernicus GLO-30.** Bucket `copernicus-dem-30m`, 1° COG tiles named
  `Copernicus_DSM_COG_10_N39_00_W106_00_DEM.tif`. These are pixel-is-point, and their
  longitude spacing widens above 50° latitude, so they must be resampled into the
  `ArcSec1` grid.
- **ESA WorldCover 10 m.** Bucket `esa-worldcover`, 3°×3° COGs
  `ESA_WorldCover_10m_2021_v200_N39W106_Map.tif`. Needs a mapping from its classes to
  NLCD-like classes.

## Tests

- `cargo test -p pathless-data` runs offline. This includes decoding of real responses
  captured in `tests/data/`.
- `cargo test -p pathless-data -- --ignored --nocapture` runs the live downloads:
  Boulder 1/3″ DEM, NLCD and a profile; offshore nodata; a seam check of the mosaic
  against a direct request; Alaska and Hawaii. It also runs the profile benchmark.
