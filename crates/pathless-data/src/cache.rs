//! On-disk tile cache format.
//!
//! File layout (`.bin`):
//! ```text
//! b"PLTILE1\n"            8-byte magic
//! u32 LE                  length N of the JSON header
//! N bytes                 JSON `TileHeader`
//! rest                    zlib stream of the encoded payload (absent when `empty`)
//! ```
//! Payload encodings:
//! * `f32`: row-wise delta of the IEEE bit patterns (`u32` wrapping subtraction from the
//!   previous pixel in the row), then byte-shuffled into 4 planes, then zlib. Lossless;
//!   NaN = nodata.
//! * `u8`: raw bytes, then zlib. 0 = nodata.
//!
//! Tiles that contain no valid data at all are written with `empty: true` and no payload.
//! Writes are atomic: a uniquely-named temp file in the same directory is fully written,
//! fsync'd and renamed over the destination.

use std::fs;
use std::io::{Read, Write};
use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};

use flate2::read::ZlibDecoder;
use flate2::write::ZlibEncoder;
use flate2::Compression;
use serde::{Deserialize, Serialize};

use crate::{Error, Result};

const MAGIC: &[u8; 8] = b"PLTILE1\n";

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TileHeader {
    pub version: u32,
    /// "f32" or "u8".
    pub dtype: String,
    pub width: u32,
    pub height: u32,
    pub source: String,
    pub px_per_deg: u32,
    pub tile_px: u32,
    pub tx: i64,
    pub ty: i64,
    /// No valid data in this tile (outside coverage); payload omitted.
    pub empty: bool,
    /// Unix seconds when the tile was downloaded.
    pub created: u64,
}

fn now_unix() -> u64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0)
}

impl TileHeader {
    /// Header for a full `tile_px`×`tile_px` tile `(tx, ty)` of `grid`.
    pub fn new(dtype: &str, source: &str, grid: crate::grid::TileGrid, tx: i64, ty: i64) -> Self {
        TileHeader {
            version: 1,
            dtype: dtype.to_string(),
            width: grid.tile_px,
            height: grid.tile_px,
            source: source.to_string(),
            px_per_deg: grid.px_per_deg,
            tile_px: grid.tile_px,
            tx,
            ty,
            empty: false,
            created: now_unix(),
        }
    }
}

static TMP_COUNTER: AtomicU64 = AtomicU64::new(0);

/// Atomically write `bytes` to `path` (temp file + fsync + rename), creating parent dirs.
pub fn write_atomic(path: &Path, bytes: &[u8]) -> Result<()> {
    let dir = path.parent().ok_or_else(|| Error::Cache(format!("bad cache path {}", path.display())))?;
    fs::create_dir_all(dir)?;
    let n = TMP_COUNTER.fetch_add(1, Ordering::Relaxed);
    let nanos =
        std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.subsec_nanos()).unwrap_or(0);
    let tmp = dir.join(format!(
        ".{}.{}.{}.{}.tmp",
        path.file_name().and_then(|s| s.to_str()).unwrap_or("tile"),
        std::process::id(),
        n,
        nanos
    ));
    let res = (|| -> Result<()> {
        let mut f = fs::File::create(&tmp)?;
        f.write_all(bytes)?;
        f.sync_all()?;
        drop(f);
        fs::rename(&tmp, path)?;
        Ok(())
    })();
    if res.is_err() {
        let _ = fs::remove_file(&tmp);
    }
    res
}

fn assemble(header: &TileHeader, payload: Option<Vec<u8>>) -> Result<Vec<u8>> {
    let json = serde_json::to_vec(header).map_err(|e| Error::Cache(e.to_string()))?;
    let mut out = Vec::with_capacity(12 + json.len() + payload.as_ref().map_or(0, |p| p.len() / 2));
    out.extend_from_slice(MAGIC);
    out.extend_from_slice(&(json.len() as u32).to_le_bytes());
    out.extend_from_slice(&json);
    if let Some(p) = payload {
        let mut enc = ZlibEncoder::new(out, Compression::new(5));
        enc.write_all(&p)?;
        out = enc.finish()?;
    }
    Ok(out)
}

/// Encode an f32 tile. If every value is NaN the tile is written as `empty`.
pub fn encode_f32(mut header: TileHeader, data: &[f32]) -> Result<Vec<u8>> {
    let (w, h) = (header.width as usize, header.height as usize);
    if data.len() != w * h {
        return Err(Error::Cache("tile data size mismatch".into()));
    }
    header.dtype = "f32".into();
    if data.iter().all(|v| v.is_nan()) {
        header.empty = true;
        return assemble(&header, None);
    }
    header.empty = false;
    let n = data.len();
    let mut planes = vec![0u8; n * 4];
    for y in 0..h {
        let mut prev = 0u32;
        for x in 0..w {
            let i = y * w + x;
            let v = data[i];
            let bits = if v.is_nan() { f32::NAN.to_bits() } else { v.to_bits() };
            let d = bits.wrapping_sub(prev).to_le_bytes();
            prev = bits;
            planes[i] = d[0];
            planes[n + i] = d[1];
            planes[2 * n + i] = d[2];
            planes[3 * n + i] = d[3];
        }
    }
    assemble(&header, Some(planes))
}

/// Encode a u8 tile. If every value equals `nodata` the tile is written as `empty`.
pub fn encode_u8(mut header: TileHeader, data: &[u8], nodata: u8) -> Result<Vec<u8>> {
    if data.len() != header.width as usize * header.height as usize {
        return Err(Error::Cache("tile data size mismatch".into()));
    }
    header.dtype = "u8".into();
    if data.iter().all(|&v| v == nodata) {
        header.empty = true;
        return assemble(&header, None);
    }
    header.empty = false;
    assemble(&header, Some(data.to_vec()))
}

/// Split a tile file into header and (compressed) payload slice.
pub fn parse_header(bytes: &[u8]) -> Result<(TileHeader, &[u8])> {
    if bytes.len() < 12 || &bytes[..8] != MAGIC {
        return Err(Error::Cache("not a PathLess tile file".into()));
    }
    let n = u32::from_le_bytes(bytes[8..12].try_into().unwrap()) as usize;
    if bytes.len() < 12 + n {
        return Err(Error::Cache("truncated tile header".into()));
    }
    let header: TileHeader = serde_json::from_slice(&bytes[12..12 + n]).map_err(|e| Error::Cache(e.to_string()))?;
    Ok((header, &bytes[12 + n..]))
}

fn inflate(payload: &[u8], expected: usize) -> Result<Vec<u8>> {
    let mut out = Vec::with_capacity(expected);
    ZlibDecoder::new(payload).read_to_end(&mut out)?;
    if out.len() != expected {
        return Err(Error::Cache(format!("tile payload size {} != expected {expected}", out.len())));
    }
    Ok(out)
}

/// Decode an f32 tile file (empty tiles decode to all-NaN).
pub fn decode_f32(bytes: &[u8]) -> Result<(TileHeader, Vec<f32>)> {
    let (header, payload) = parse_header(bytes)?;
    if header.dtype != "f32" {
        return Err(Error::Cache(format!("expected f32 tile, found {}", header.dtype)));
    }
    let (w, h) = (header.width as usize, header.height as usize);
    let n = w * h;
    if header.empty {
        return Ok((header, vec![f32::NAN; n]));
    }
    let planes = inflate(payload, n * 4)?;
    let mut data = vec![0f32; n];
    for y in 0..h {
        let mut prev = 0u32;
        for x in 0..w {
            let i = y * w + x;
            let d = u32::from_le_bytes([planes[i], planes[n + i], planes[2 * n + i], planes[3 * n + i]]);
            let bits = prev.wrapping_add(d);
            prev = bits;
            data[i] = f32::from_bits(bits);
        }
    }
    Ok((header, data))
}

/// Decode a u8 tile file (empty tiles decode to all-`nodata`).
pub fn decode_u8(bytes: &[u8], nodata: u8) -> Result<(TileHeader, Vec<u8>)> {
    let (header, payload) = parse_header(bytes)?;
    if header.dtype != "u8" {
        return Err(Error::Cache(format!("expected u8 tile, found {}", header.dtype)));
    }
    let n = header.width as usize * header.height as usize;
    if header.empty {
        return Ok((header, vec![nodata; n]));
    }
    let data = inflate(payload, n)?;
    Ok((header, data))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip_f32() {
        let h = TileHeader {
            width: 3,
            height: 2,
            ..TileHeader::new("f32", "test", crate::grid::TileGrid { px_per_deg: 3600, tile_px: 3 }, 1, 2)
        };
        let data = vec![1600.5, 1601.25, f32::NAN, -3.0, 0.0, 8848.86];
        let bytes = encode_f32(h.clone(), &data).unwrap();
        let (h2, d2) = decode_f32(&bytes).unwrap();
        assert_eq!(h2.tx, 1);
        assert!(!h2.empty);
        for (a, b) in data.iter().zip(&d2) {
            assert!((a.is_nan() && b.is_nan()) || a == b);
        }
        let bytes = encode_f32(h, &[f32::NAN; 6]).unwrap();
        let (h3, d3) = decode_f32(&bytes).unwrap();
        assert!(h3.empty && d3.iter().all(|v| v.is_nan()));
    }

    #[test]
    fn roundtrip_u8_and_atomic_write() {
        let h = TileHeader::new("u8", "test", crate::grid::TileGrid { px_per_deg: 3600, tile_px: 2 }, 0, 0);
        let data = vec![11, 42, 0, 95];
        let bytes = encode_u8(h, &data, 0).unwrap();
        let dir = std::env::temp_dir().join(format!("pathless-cache-test-{}", std::process::id()));
        let path = dir.join("a/b/t.bin");
        write_atomic(&path, &bytes).unwrap();
        let (_, d2) = decode_u8(&fs::read(&path).unwrap(), 0).unwrap();
        assert_eq!(d2, data);
        fs::remove_dir_all(&dir).unwrap();
    }
}
