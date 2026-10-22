//! Minimal pure-Rust GeoTIFF reader (no GDAL, no libtiff).
//!
//! Self-contained because the `tiff` crate (checked 0.11.3) rejects 8-bit palette images (as returned by
//! the NLCD WCS) and some GeoTIFF tag types. Supported:
//! * classic TIFF and BigTIFF, little and big endian
//! * tiled and stripped layouts; sparse chunks (offset/byte count 0 → `fill`)
//! * compression: none, LZW, Deflate (8 / 32946), PackBits
//! * predictor 1, 2 (horizontal) and 3 (floating point)
//! * samples: u8/i8/u16/i16/u32/i32/f32/f64; palette images return raw indices
//! * multi-sample images: only the first sample/band is read (chunky or planar)
//! * georeferencing via ModelTiepoint+ModelPixelScale or ModelTransformation, with
//!   PixelIsPoint handling, and the GDAL_NODATA tag (42113)
//!
//! Only geographic (lat/lon) CRSs are accepted; everything in this crate is EPSG:4326.

use std::collections::HashMap;

use crate::types::{GeoTransform, Raster};
use crate::{Error, Result};

/// Pixel types that GeoTIFF samples can be converted into.
pub trait Sample: Copy + Send + 'static {
    fn from_f64(v: f64) -> Self;
}

impl Sample for f32 {
    #[inline]
    fn from_f64(v: f64) -> Self {
        v as f32
    }
}
impl Sample for f64 {
    #[inline]
    fn from_f64(v: f64) -> Self {
        v
    }
}
impl Sample for u8 {
    #[inline]
    fn from_f64(v: f64) -> Self {
        v.clamp(0.0, 255.0) as u8
    }
}
impl Sample for i16 {
    #[inline]
    fn from_f64(v: f64) -> Self {
        v.clamp(i16::MIN as f64, i16::MAX as f64) as i16
    }
}
impl Sample for u16 {
    #[inline]
    fn from_f64(v: f64) -> Self {
        v.clamp(0.0, u16::MAX as f64) as u16
    }
}

/// Result of decoding a GeoTIFF: the first band converted to `T`, plus metadata.
#[derive(Debug, Clone)]
pub struct GeoTiff<T> {
    /// Raster with geotransform. `nodata` is set from GDAL_NODATA when present.
    pub raster: Raster<T>,
    /// Raw GDAL_NODATA value (before conversion to `T`).
    pub nodata_raw: Option<f64>,
    /// True if the image is a palette (colormap) image; values are raw indices.
    pub is_palette: bool,
    /// Number of chunks that were sparse (no data stored) and filled with `fill`.
    pub sparse_chunks: usize,
}

fn err(msg: impl Into<String>) -> Error {
    Error::Decode(format!("TIFF: {}", msg.into()))
}

// ---------------------------------------------------------------- low-level reading

struct Reader<'a> {
    b: &'a [u8],
    le: bool,
    big: bool,
}

#[derive(Clone, Copy)]
struct Entry {
    typ: u16,
    count: u64,
    /// Absolute offset of the value bytes.
    pos: usize,
}

fn type_size(t: u16) -> usize {
    match t {
        1 | 2 | 6 | 7 => 1,
        3 | 8 => 2,
        4 | 9 | 11 | 13 => 4,
        5 | 10 | 12 | 16 | 17 | 18 => 8,
        _ => 0,
    }
}

impl<'a> Reader<'a> {
    fn slice(&self, pos: usize, n: usize) -> Result<&'a [u8]> {
        self.b
            .get(pos..pos.checked_add(n).ok_or_else(|| err("offset overflow"))?)
            .ok_or_else(|| err("unexpected end of file"))
    }
    fn u16(&self, pos: usize) -> Result<u16> {
        let s: [u8; 2] = self.slice(pos, 2)?.try_into().unwrap();
        Ok(if self.le { u16::from_le_bytes(s) } else { u16::from_be_bytes(s) })
    }
    fn u32(&self, pos: usize) -> Result<u32> {
        let s: [u8; 4] = self.slice(pos, 4)?.try_into().unwrap();
        Ok(if self.le { u32::from_le_bytes(s) } else { u32::from_be_bytes(s) })
    }
    fn u64(&self, pos: usize) -> Result<u64> {
        let s: [u8; 8] = self.slice(pos, 8)?.try_into().unwrap();
        Ok(if self.le { u64::from_le_bytes(s) } else { u64::from_be_bytes(s) })
    }

    fn first_ifd(&self) -> Result<HashMap<u16, Entry>> {
        let off = if self.big { self.u64(8)? as usize } else { self.u32(4)? as usize };
        let (n, mut p, esz) = if self.big {
            (self.u64(off)? as usize, off + 8, 20usize)
        } else {
            (self.u16(off)? as usize, off + 2, 12usize)
        };
        if n > 4096 {
            return Err(err("implausible IFD entry count"));
        }
        let inline = if self.big { 8 } else { 4 };
        let mut m = HashMap::with_capacity(n);
        for _ in 0..n {
            let tag = self.u16(p)?;
            let typ = self.u16(p + 2)?;
            let count = if self.big { self.u64(p + 4)? } else { self.u32(p + 4)? as u64 };
            let vpos = if self.big { p + 12 } else { p + 8 };
            let size = (type_size(typ) as u64).saturating_mul(count);
            let pos = if size <= inline as u64 {
                vpos
            } else if self.big {
                self.u64(vpos)? as usize
            } else {
                self.u32(vpos)? as usize
            };
            m.insert(tag, Entry { typ, count, pos });
            p += esz;
        }
        Ok(m)
    }

    fn values_f64(&self, e: Entry) -> Result<Vec<f64>> {
        let n = e.count as usize;
        if type_size(e.typ) == 0 {
            return Err(err(format!("unknown tag type {}", e.typ)));
        }
        self.slice(e.pos, n.saturating_mul(type_size(e.typ)))?; // bounds check
        let mut v = Vec::with_capacity(n);
        for i in 0..n {
            let x = match e.typ {
                1 | 7 | 2 => self.b[e.pos + i] as f64,
                6 => self.b[e.pos + i] as i8 as f64,
                3 => self.u16(e.pos + 2 * i)? as f64,
                8 => self.u16(e.pos + 2 * i)? as i16 as f64,
                4 | 13 => self.u32(e.pos + 4 * i)? as f64,
                9 => self.u32(e.pos + 4 * i)? as i32 as f64,
                11 => f32::from_bits(self.u32(e.pos + 4 * i)?) as f64,
                12 => f64::from_bits(self.u64(e.pos + 8 * i)?),
                16 | 18 => self.u64(e.pos + 8 * i)? as f64,
                17 => self.u64(e.pos + 8 * i)? as i64 as f64,
                5 => self.u32(e.pos + 8 * i)? as f64 / (self.u32(e.pos + 8 * i + 4)?.max(1)) as f64,
                10 => self.u32(e.pos + 8 * i)? as i32 as f64 / (self.u32(e.pos + 8 * i + 4)? as i32).max(1) as f64,
                _ => unreachable!(),
            };
            v.push(x);
        }
        Ok(v)
    }

    fn values_u64(&self, e: Entry) -> Result<Vec<u64>> {
        let n = e.count as usize;
        let mut v = Vec::with_capacity(n);
        for i in 0..n {
            v.push(match e.typ {
                1 | 7 => *self.b.get(e.pos + i).ok_or_else(|| err("eof"))? as u64,
                3 => self.u16(e.pos + 2 * i)? as u64,
                4 | 13 => self.u32(e.pos + 4 * i)? as u64,
                16 | 18 => self.u64(e.pos + 8 * i)?,
                t => return Err(err(format!("tag type {t} is not an unsigned integer"))),
            });
        }
        Ok(v)
    }

    fn string(&self, e: Entry) -> Result<String> {
        let s = self.slice(e.pos, e.count as usize)?;
        Ok(String::from_utf8_lossy(s).trim_matches(|c: char| c == '\0' || c.is_whitespace()).to_string())
    }
}

// ---------------------------------------------------------------- decompression

fn decompress(comp: u16, data: &[u8], expected: usize) -> Result<Vec<u8>> {
    match comp {
        1 => Ok(data.to_vec()),
        5 => {
            let mut out = vec![0u8; expected];
            let mut dec = weezl::decode::Decoder::with_tiff_size_switch(weezl::BitOrder::Msb, 8);
            let (mut ip, mut op) = (0usize, 0usize);
            while op < expected && ip < data.len() {
                let r = dec.decode_bytes(&data[ip..], &mut out[op..]);
                ip += r.consumed_in;
                op += r.consumed_out;
                match r.status {
                    Ok(weezl::LzwStatus::Ok) => {
                        if r.consumed_in == 0 && r.consumed_out == 0 {
                            break;
                        }
                    }
                    Ok(_) => break,
                    Err(e) => {
                        if op == 0 {
                            return Err(err(format!("LZW: {e:?}")));
                        }
                        break;
                    }
                }
            }
            out.truncate(op);
            Ok(out)
        }
        8 | 32946 => {
            use std::io::Read;
            let mut out = Vec::with_capacity(expected);
            flate2::read::ZlibDecoder::new(data).read_to_end(&mut out).map_err(|e| err(format!("deflate: {e}")))?;
            Ok(out)
        }
        32773 => {
            let mut out = Vec::with_capacity(expected);
            let mut i = 0;
            while i < data.len() && out.len() < expected {
                let n = data[i] as i8;
                i += 1;
                if n >= 0 {
                    let len = n as usize + 1;
                    out.extend_from_slice(data.get(i..i + len).ok_or_else(|| err("PackBits overrun"))?);
                    i += len;
                } else if n != -128 {
                    let b = *data.get(i).ok_or_else(|| err("PackBits overrun"))?;
                    out.extend(std::iter::repeat_n(b, (-(n as i16)) as usize + 1));
                    i += 1;
                }
            }
            Ok(out)
        }
        c => Err(err(format!("unsupported compression {c}"))),
    }
}

// ---------------------------------------------------------------- GeoTIFF

/// Parse GeoKeyDirectory into (key, value) pairs for short-valued keys.
fn geokeys(dir: &[f64]) -> Vec<(u16, u16)> {
    let mut out = Vec::new();
    if dir.len() < 4 {
        return out;
    }
    let n = dir[3] as usize;
    for k in 0..n {
        let base = 4 + k * 4;
        if base + 3 >= dir.len() {
            break;
        }
        let (key, loc, value) = (dir[base] as u16, dir[base + 1] as u16, dir[base + 3] as u16);
        if loc == 0 {
            out.push((key, value));
        }
    }
    out
}

/// Decode a GeoTIFF from memory (first image, first band). Pixels not covered by stored
/// data (sparse chunks) are set to `fill`. The GDAL_NODATA value (if any) is converted to
/// `T` and stored in `raster.nodata`.
pub fn read_geotiff<T: Sample>(bytes: &[u8], fill: T) -> Result<GeoTiff<T>> {
    if bytes.len() < 8 || !(bytes.starts_with(b"II") || bytes.starts_with(b"MM")) {
        let preview: String = String::from_utf8_lossy(&bytes[..bytes.len().min(300)]).into_owned();
        return Err(Error::Decode(format!("response is not a TIFF: {preview}")));
    }
    let mut rd = Reader { b: bytes, le: bytes[0] == b'I', big: false };
    match rd.u16(2)? {
        42 => {}
        43 => rd.big = true,
        v => return Err(err(format!("bad TIFF version {v}"))),
    }
    let ifd = rd.first_ifd()?;
    let get = |tag: u16| ifd.get(&tag).copied();
    let get_u = |tag: u16, default: u64| -> Result<u64> {
        match get(tag) {
            Some(e) => rd.values_u64(e)?.first().copied().ok_or_else(|| err(format!("empty tag {tag}"))),
            None => Ok(default),
        }
    };

    let w = get_u(256, 0)? as usize;
    let h = get_u(257, 0)? as usize;
    if w == 0 || h == 0 || w.saturating_mul(h) > 1_000_000_000 {
        return Err(err(format!("unreasonable dimensions {w}x{h}")));
    }
    let bps = get_u(258, 1)? as usize;
    let compression = get_u(259, 1)? as u16;
    let photometric = get_u(262, 1)?;
    let spp = get_u(277, 1)?.max(1) as usize;
    let planar = get_u(284, 1)?;
    let predictor = get_u(317, 1)?;
    let sample_format = get_u(339, 1)?;
    let is_palette = photometric == 3;
    if !matches!(bps, 8 | 16 | 32 | 64) {
        return Err(err(format!("unsupported bits per sample {bps}")));
    }
    let bytes_ps = bps / 8;
    let pixel_stride = if planar == 2 { 1 } else { spp }; // samples per pixel within a chunk

    // --- georeferencing ---
    let gk = match get(34735) {
        Some(e) => geokeys(&rd.values_f64(e)?),
        None => Vec::new(),
    };
    if gk.iter().any(|k| k.0 == 1024 && k.1 == 1) {
        return Err(err("projected CRS not supported (need geographic EPSG:4326)"));
    }
    let pixel_is_point = gk.iter().any(|k| k.0 == 1025 && k.1 == 2);
    let transform = get(34264).map(|e| rd.values_f64(e)).transpose()?;
    let tiepoint = get(33922).map(|e| rd.values_f64(e)).transpose()?;
    let scale = get(33550).map(|e| rd.values_f64(e)).transpose()?;
    let mut geo = if let Some(t) = transform.filter(|t| t.len() >= 8) {
        if t[1].abs() > 1e-12 || t[4].abs() > 1e-12 {
            return Err(err("rotated model transformation not supported"));
        }
        GeoTransform { origin_lon: t[3], origin_lat: t[7], px_lon: t[0], px_lat: t[5] }
    } else if let (Some(tp), Some(sc)) = (tiepoint.filter(|v| v.len() >= 6), scale.filter(|v| v.len() >= 2)) {
        GeoTransform {
            origin_lon: tp[3] - tp[0] * sc[0],
            origin_lat: tp[4] + tp[1] * sc[1],
            px_lon: sc[0],
            px_lat: -sc[1],
        }
    } else {
        return Err(err("no georeferencing tags (ModelTransformation or Tiepoint+PixelScale)"));
    };
    if pixel_is_point {
        geo.origin_lon -= 0.5 * geo.px_lon;
        geo.origin_lat -= 0.5 * geo.px_lat;
    }
    let nodata_raw = match get(42113) {
        Some(e) => rd.string(e)?.parse::<f64>().ok(),
        None => None,
    };

    // --- chunk layout ---
    let tiled = get(322).is_some();
    let (cw, ch, offsets, counts) = if tiled {
        let tw = get_u(322, 0)? as usize;
        let tl = get_u(323, 0)? as usize;
        let off = rd.values_u64(get(324).ok_or_else(|| err("missing TileOffsets"))?)?;
        let cnt = rd.values_u64(get(325).ok_or_else(|| err("missing TileByteCounts"))?)?;
        (tw, tl, off, cnt)
    } else {
        let rps = (get_u(278, h as u64)? as usize).min(h).max(1);
        let off = rd.values_u64(get(273).ok_or_else(|| err("missing StripOffsets"))?)?;
        let cnt = rd.values_u64(get(279).ok_or_else(|| err("missing StripByteCounts"))?)?;
        (w, rps, off, cnt)
    };
    if cw == 0 || ch == 0 {
        return Err(err("zero chunk size"));
    }
    let across = w.div_ceil(cw);
    let down = h.div_ceil(ch);
    let n_chunks = across * down; // band 0 only (for planar, band 0 chunks come first)
    if offsets.len() < n_chunks {
        return Err(err("too few chunk offsets"));
    }

    let row_bytes = cw * pixel_stride * bytes_ps;
    let convert = |raw: &[u8], i: usize| -> f64 {
        let p = i * bytes_ps;
        let s = &raw[p..p + bytes_ps];
        macro_rules! num {
            ($t:ty, $n:expr) => {{
                let a: [u8; $n] = s.try_into().unwrap();
                if rd.le {
                    <$t>::from_le_bytes(a)
                } else {
                    <$t>::from_be_bytes(a)
                }
            }};
        }
        match (sample_format, bytes_ps) {
            (3, 4) => num!(f32, 4) as f64,
            (3, 8) => num!(f64, 8),
            (2, 1) => s[0] as i8 as f64,
            (2, 2) => num!(i16, 2) as f64,
            (2, 4) => num!(i32, 4) as f64,
            (2, 8) => num!(i64, 8) as f64,
            (_, 1) => s[0] as f64,
            (_, 2) => num!(u16, 2) as f64,
            (_, 4) => num!(u32, 4) as f64,
            _ => num!(u64, 8) as f64,
        }
    };

    let mut data = vec![fill; w * h];
    let mut sparse_chunks = 0;
    let mut buf = Vec::new();
    for (ci, &off) in offsets.iter().enumerate().take(n_chunks) {
        let (off, cnt) = (off as usize, counts.get(ci).copied().unwrap_or(0) as usize);
        if off == 0 || cnt == 0 {
            sparse_chunks += 1;
            continue;
        }
        let (cx, cy) = (ci % across, ci / across);
        let (x0, y0) = (cx * cw, cy * ch);
        let rows_here = if tiled { ch } else { ch.min(h - y0) };
        let expected = row_bytes * rows_here;
        let raw = rd.slice(off, cnt)?;
        let mut dec = decompress(compression, raw, expected)?;
        let rows_avail = (dec.len() / row_bytes).min(rows_here);
        // undo predictor
        match predictor {
            1 => {}
            2 => {
                for r in 0..rows_avail {
                    let row = &mut dec[r * row_bytes..(r + 1) * row_bytes];
                    undo_horizontal(row, pixel_stride, bytes_ps, rd.le);
                }
            }
            3 => {
                buf.resize(row_bytes, 0);
                for r in 0..rows_avail {
                    let row = &mut dec[r * row_bytes..(r + 1) * row_bytes];
                    undo_float_predictor(row, &mut buf, bytes_ps, rd.le);
                }
            }
            p => return Err(err(format!("unsupported predictor {p}"))),
        }
        let copy_w = cw.min(w - x0);
        let copy_h = rows_avail.min(h - y0);
        for r in 0..copy_h {
            let base = r * cw * pixel_stride;
            let drow = (y0 + r) * w + x0;
            for c in 0..copy_w {
                data[drow + c] = T::from_f64(convert(&dec, base + c * pixel_stride));
            }
        }
    }

    Ok(GeoTiff {
        raster: Raster { width: w, height: h, data, geo, nodata: nodata_raw.map(T::from_f64) },
        nodata_raw,
        is_palette,
        sparse_chunks,
    })
}

/// Undo TIFF predictor 2 on one row (integer horizontal differencing, per sample).
fn undo_horizontal(row: &mut [u8], spp: usize, bps: usize, le: bool) {
    let n = row.len() / bps;
    match bps {
        1 => {
            for i in spp..n {
                row[i] = row[i].wrapping_add(row[i - spp]);
            }
        }
        2 => {
            let rd = |r: &[u8], i: usize| {
                if le {
                    u16::from_le_bytes([r[2 * i], r[2 * i + 1]])
                } else {
                    u16::from_be_bytes([r[2 * i], r[2 * i + 1]])
                }
            };
            for i in spp..n {
                let v = rd(row, i).wrapping_add(rd(row, i - spp));
                let b = if le { v.to_le_bytes() } else { v.to_be_bytes() };
                row[2 * i..2 * i + 2].copy_from_slice(&b);
            }
        }
        4 => {
            let rd = |r: &[u8], i: usize| {
                let a: [u8; 4] = r[4 * i..4 * i + 4].try_into().unwrap();
                if le {
                    u32::from_le_bytes(a)
                } else {
                    u32::from_be_bytes(a)
                }
            };
            for i in spp..n {
                let v = rd(row, i).wrapping_add(rd(row, i - spp));
                let b = if le { v.to_le_bytes() } else { v.to_be_bytes() };
                row[4 * i..4 * i + 4].copy_from_slice(&b);
            }
        }
        _ => {
            let rd = |r: &[u8], i: usize| {
                let a: [u8; 8] = r[8 * i..8 * i + 8].try_into().unwrap();
                if le {
                    u64::from_le_bytes(a)
                } else {
                    u64::from_be_bytes(a)
                }
            };
            for i in spp..n {
                let v = rd(row, i).wrapping_add(rd(row, i - spp));
                let b = if le { v.to_le_bytes() } else { v.to_be_bytes() };
                row[8 * i..8 * i + 8].copy_from_slice(&b);
            }
        }
    }
}

/// Undo TIFF predictor 3 (floating point): byte-wise cumulative sum over the row, then
/// un-shuffle the byte planes (plane 0 = most significant byte) into file byte order.
fn undo_float_predictor(row: &mut [u8], tmp: &mut [u8], bps: usize, le: bool) {
    for i in 1..row.len() {
        row[i] = row[i].wrapping_add(row[i - 1]);
    }
    let n = row.len() / bps;
    tmp[..row.len()].copy_from_slice(row);
    for i in 0..n {
        for b in 0..bps {
            let v = tmp[b * n + i]; // b-th most significant byte
            let dst = if le { bps - 1 - b } else { b };
            row[i * bps + dst] = v;
        }
    }
}

/// Decode an elevation GeoTIFF into a `Raster<f32>` where every nodata value (GDAL_NODATA,
/// sparse chunks, or absurd values beyond ±1e30) becomes NaN and `nodata` is `None`.
pub fn read_elevation_geotiff(bytes: &[u8]) -> Result<Raster<f32>> {
    let g = read_geotiff::<f32>(bytes, f32::NAN)?;
    let mut r = g.raster;
    let nd = g.nodata_raw.map(|v| v as f32);
    for v in r.data.iter_mut() {
        if Some(*v) == nd || !(-1e30..=1e30).contains(v) {
            *v = f32::NAN;
        }
    }
    r.nodata = None;
    Ok(r)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn predictors() {
        // predictor 2, u16 LE, 1 sample: [1, +2, +3] -> [1, 3, 6]
        let mut row = vec![1, 0, 2, 0, 3, 0];
        undo_horizontal(&mut row, 1, 2, true);
        assert_eq!(row, vec![1, 0, 3, 0, 6, 0]);
        // predictor 3 round trip on two f32 values (LE)
        let vals = [1623.5f32, -2.25];
        let n = vals.len();
        let mut planes = vec![0u8; n * 4];
        for (i, v) in vals.iter().enumerate() {
            let be = v.to_be_bytes();
            for b in 0..4 {
                planes[b * n + i] = be[b];
            }
        }
        for i in (1..planes.len()).rev() {
            planes[i] = planes[i].wrapping_sub(planes[i - 1]);
        }
        let mut tmp = vec![0u8; planes.len()];
        undo_float_predictor(&mut planes, &mut tmp, 4, true);
        let out: Vec<f32> = planes.chunks(4).map(|c| f32::from_le_bytes(c.try_into().unwrap())).collect();
        assert_eq!(out, vals);
    }

    #[test]
    fn packbits() {
        let enc = [0xFEu8, 0xAA, 0x02, 0x80, 0x00, 0x2A];
        assert_eq!(decompress(32773, &enc, 6).unwrap(), vec![0xAA, 0xAA, 0xAA, 0x80, 0x00, 0x2A]);
    }
}
