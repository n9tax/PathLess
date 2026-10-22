//! Turn a coverage grid into a colour-banded PNG data URL for the map.

use base64::Engine as _;

use crate::project::LegendEntry;

#[derive(Debug, Clone, Copy)]
struct Band {
    value: f32,
    rgba: [u8; 4],
}

fn parse_hex(c: &str) -> [u8; 4] {
    let h = c.trim_start_matches('#');
    let v = |i: usize| u8::from_str_radix(h.get(i..i + 2).unwrap_or("80"), 16).unwrap_or(128);
    [v(0), v(2), v(4), 255]
}

/// Prepare legend bands so the first matching band wins.
/// `higher_is_better`: a cell gets the first band with `value >= band.value`
/// (bands sorted descending); otherwise `value <= band.value` (ascending).
fn bands(legend: &[LegendEntry], higher_is_better: bool) -> Vec<Band> {
    let mut b: Vec<Band> = legend.iter().map(|e| Band { value: e.value as f32, rgba: parse_hex(&e.color) }).collect();
    if higher_is_better {
        b.sort_by(|x, y| y.value.total_cmp(&x.value));
    } else {
        b.sort_by(|x, y| x.value.total_cmp(&y.value));
    }
    b
}

/// Colourise `values` (row-major, north-up, NaN = no data) and encode as PNG.
/// Returns the data URL and the fraction of valid cells that fall in a band.
pub fn coverage_png(
    values: &[f32],
    width: usize,
    height: usize,
    legend: &[LegendEntry],
    higher_is_better: bool,
) -> anyhow::Result<(String, f64)> {
    let bands = bands(legend, higher_is_better);
    let mut rgba = vec![0u8; width * height * 4];
    let (mut valid, mut covered) = (0usize, 0usize);
    for (i, &v) in values.iter().enumerate() {
        if !v.is_finite() {
            continue;
        }
        valid += 1;
        let hit = bands.iter().find(|b| if higher_is_better { v >= b.value } else { v <= b.value });
        if let Some(b) = hit {
            covered += 1;
            rgba[i * 4..i * 4 + 4].copy_from_slice(&b.rgba);
        }
    }

    let mut buf = Vec::new();
    {
        let mut enc = png::Encoder::new(&mut buf, width as u32, height as u32);
        enc.set_color(png::ColorType::Rgba);
        enc.set_depth(png::BitDepth::Eight);
        enc.set_compression(png::Compression::Fast);
        let mut w = enc.write_header()?;
        w.write_image_data(&rgba)?;
    }
    let url = format!("data:image/png;base64,{}", base64::engine::general_purpose::STANDARD.encode(&buf));
    let frac = if valid > 0 { covered as f64 / valid as f64 } else { 0.0 };
    Ok((url, frac))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn legend() -> Vec<LegendEntry> {
        vec![
            LegendEntry { value: -90.0, color: "#ff0000".into() },
            LegendEntry { value: -60.0, color: "#00ff00".into() },
        ]
    }

    #[test]
    fn bands_pick_strongest_matching_level() {
        let b = bands(&legend(), true);
        assert_eq!(b[0].value, -60.0);
        let (url, frac) = coverage_png(&[-50.0, -70.0, -120.0, f32::NAN], 2, 2, &legend(), true).unwrap();
        assert!(url.starts_with("data:image/png;base64,"));
        assert!((frac - 2.0 / 3.0).abs() < 1e-9);
    }
}
