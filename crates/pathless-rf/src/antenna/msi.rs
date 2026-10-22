//! MSI / Planet antenna pattern file format (`.msi`, `.pln`, `.txt`, `.ant`).
//!
//! # Format
//!
//! ```text
//! NAME Generic 65deg 18dBi sector
//! MAKE Generic
//! FREQUENCY 1950
//! H_WIDTH 65
//! V_WIDTH 7.5
//! FRONT_TO_BACK 28
//! GAIN 15.85 dBd
//! TILT ELECTRICAL
//! POLARIZATION +45
//! COMMENT any text
//! HORIZONTAL 360
//! 0 0.00
//! 1 0.01
//! ...
//! VERTICAL 360
//! 0 0.00
//! ...
//! ```
//!
//! Pattern values are attenuations in dB relative to the peak gain (positive numbers).
//! Horizontal angles run clockwise from boresight; vertical angles are degrees below the
//! horizon (0 horizon, 90 nadir, 180 back horizon, 270 zenith).
//!
//! # Gain unit
//!
//! The Planet convention is that `GAIN` is in **dBd** unless a unit is given; most files
//! write an explicit `dBd` or `dBi` suffix. Without a suffix this parser assumes dBd by
//! default ([`MsiParseOptions::default_gain_unit`]). The writer always writes an explicit
//! unit (dBi by default).
//!
//! # Leniency
//!
//! * keywords are case-insensitive; separators may be spaces, tabs, `;` or `=`;
//! * decimal commas are accepted (`12,5`), as are comma separated pairs (`12,0.5`);
//! * fewer (or more) than 360 samples and fractional angles are resampled to 1° with
//!   circular linear interpolation; a duplicate 360° sample is accepted;
//! * extra columns on data lines are ignored;
//! * if all pattern values are ≤ 0 (relative gain instead of attenuation) they are negated;
//! * `TILT ELECTRICAL|MECHANICAL [deg]`, `ELECTRICAL_TILT deg`, `MECHANICAL_TILT deg`
//!   are understood; unknown keywords are ignored.

use serde::{Deserialize, Serialize};

use super::AntennaPattern;
use crate::basic::DIPOLE_GAIN_DBI;
use crate::error::RfError;

/// Unit of the `GAIN` keyword.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum GainUnit {
    /// dB relative to a half-wave dipole (Planet default).
    #[default]
    Dbd,
    /// dB relative to an isotropic radiator.
    Dbi,
}

/// Options for [`parse_msi`].
#[derive(Debug, Clone, Copy, PartialEq, Default, Serialize, Deserialize)]
pub struct MsiParseOptions {
    /// Unit assumed when the GAIN line has no unit suffix (default dBd).
    pub default_gain_unit: GainUnit,
}

const KEYWORDS: [&str; 15] = [
    "NAME",
    "MAKE",
    "FREQUENCY",
    "H_WIDTH",
    "V_WIDTH",
    "FRONT_TO_BACK",
    "GAIN",
    "TILT",
    "ELECTRICAL_TILT",
    "MECHANICAL_TILT",
    "POLARIZATION",
    "COMMENT",
    "HORIZONTAL",
    "VERTICAL",
    "TYPE",
];

fn parse_num(tok: &str) -> Option<f64> {
    let t = tok.trim();
    if t.is_empty() {
        return None;
    }
    t.parse::<f64>().ok().or_else(|| {
        if t.contains(',') && !t.contains('.') {
            t.replace(',', ".").parse::<f64>().ok()
        } else {
            None
        }
    })
}

/// Split a data line into numbers, handling decimal commas and comma separated pairs.
fn data_numbers(line: &str) -> Vec<f64> {
    let toks: Vec<&str> = line.split(|c: char| c.is_whitespace() || c == ';').filter(|t| !t.is_empty()).collect();
    if toks.len() == 1 && toks[0].contains(',') {
        // "a,b" pair (comma as separator)
        return toks[0].split(',').filter_map(parse_num).collect();
    }
    let mut out = Vec::new();
    for t in toks {
        match parse_num(t) {
            Some(x) => out.push(x),
            None => {
                // "0,5,1,2" style or trailing junk: try splitting on commas
                let parts: Vec<f64> = t.split(',').filter_map(parse_num).collect();
                if parts.is_empty() {
                    break;
                }
                out.extend(parts);
            }
        }
    }
    out
}

/// Resample (angle, value) pairs to 360 samples at integer degrees (circular linear
/// interpolation).
fn resample_360(points: &[(f64, f64)]) -> Vec<f32> {
    let mut pts: Vec<(f64, f64)> = points.iter().map(|(a, v)| (a.rem_euclid(360.0), *v)).collect();
    pts.sort_by(|a, b| a.0.total_cmp(&b.0));
    pts.dedup_by(|b, a| (a.0 - b.0).abs() < 1e-9);
    if pts.is_empty() {
        return vec![0.0; 360];
    }
    if pts.len() == 1 {
        return vec![pts[0].1 as f32; 360];
    }
    let n = pts.len();
    (0..360)
        .map(|deg| {
            let x = deg as f64;
            // find segment [k, k+1] (circular) containing x
            let k = match pts.iter().rposition(|p| p.0 <= x) {
                Some(k) => k,
                None => n - 1, // before the first point: wrap from last
            };
            let (a0, v0) = pts[k];
            let (a1, v1) = pts[(k + 1) % n];
            let mut span = a1 - a0;
            let mut dx = x - a0;
            if span <= 0.0 {
                span += 360.0;
            }
            if dx < 0.0 {
                dx += 360.0;
            }
            let t = if span > 0.0 { dx / span } else { 0.0 };
            (v0 + (v1 - v0) * t) as f32
        })
        .collect()
}

fn split_keyword(line: &str) -> (String, &str) {
    let trimmed = line.trim();
    let end = trimmed.find(|c: char| c.is_whitespace() || c == '=' || c == ';' || c == ':').unwrap_or(trimmed.len());
    let kw = trimmed[..end].to_ascii_uppercase();
    let rest = trimmed[end..].trim_start_matches(|c: char| c.is_whitespace() || c == '=' || c == ';' || c == ':');
    (kw, rest.trim())
}

/// Parse MSI / Planet pattern text.
pub fn parse_msi(text: &str, opts: &MsiParseOptions) -> Result<AntennaPattern, RfError> {
    let mut p = AntennaPattern::flat("", 0.0, 0.0);
    p.make.clear();
    p.h_beamwidth_deg = 0.0;
    p.v_beamwidth_deg = 0.0;
    let mut horizontal: Option<Vec<(f64, f64)>> = None;
    let mut vertical: Option<Vec<(f64, f64)>> = None;
    let mut section: Option<(bool, usize)> = None; // (is_horizontal, line of header)
    let mut current: Vec<(f64, f64)> = Vec::new();
    let mut have_fb = false;
    let mut have_hbw = false;
    let mut have_vbw = false;
    let mut first_content = true;

    let finish = |section: &mut Option<(bool, usize)>,
                  current: &mut Vec<(f64, f64)>,
                  horizontal: &mut Option<Vec<(f64, f64)>>,
                  vertical: &mut Option<Vec<(f64, f64)>>|
     -> Result<(), RfError> {
        if let Some((is_h, line)) = section.take() {
            if current.is_empty() {
                return Err(RfError::PatternParse {
                    line,
                    message: format!("{} section has no data", if is_h { "HORIZONTAL" } else { "VERTICAL" }),
                });
            }
            let data = std::mem::take(current);
            if is_h {
                *horizontal = Some(data);
            } else {
                *vertical = Some(data);
            }
        }
        Ok(())
    };

    for (lineno, raw) in text.lines().enumerate() {
        let lineno = lineno + 1;
        let line = raw.trim_start_matches('\u{feff}').trim();
        if line.is_empty() || line.starts_with('#') || line.starts_with("//") {
            continue;
        }
        if section.is_some() {
            let nums = data_numbers(line);
            if nums.len() >= 2
                && parse_num(line.split(|c: char| c.is_whitespace() || c == ';' || c == ',').next().unwrap_or(""))
                    .is_some()
            {
                current.push((nums[0], nums[1]));
                continue;
            }
            finish(&mut section, &mut current, &mut horizontal, &mut vertical)?;
        }
        let (kw, rest) = split_keyword(line);
        let is_known = KEYWORDS.contains(&kw.as_str());
        if !is_known && first_content && p.name.is_empty() {
            // Some files start with a bare name line.
            p.name = line.to_string();
            first_content = false;
            continue;
        }
        first_content = false;
        let num = || rest.split_whitespace().next().and_then(parse_num);
        let bad =
            |what: &str| RfError::PatternParse { line: lineno, message: format!("invalid {what} value '{rest}'") };
        match kw.as_str() {
            "NAME" => p.name = rest.to_string(),
            "MAKE" => p.make = rest.to_string(),
            "COMMENT" => {
                if !p.comment.is_empty() {
                    p.comment.push('\n');
                }
                p.comment.push_str(rest);
            }
            "POLARIZATION" => p.polarization = rest.to_string(),
            "FREQUENCY" => {
                let mut f = num().ok_or_else(|| bad("FREQUENCY"))?;
                let unit = rest.split_whitespace().nth(1).unwrap_or("").to_ascii_uppercase();
                if unit == "GHZ" {
                    f *= 1000.0;
                } else if unit == "KHZ" {
                    f /= 1000.0;
                }
                p.frequency_mhz = f;
            }
            "H_WIDTH" => {
                p.h_beamwidth_deg = num().ok_or_else(|| bad("H_WIDTH"))?;
                have_hbw = true;
            }
            "V_WIDTH" => {
                p.v_beamwidth_deg = num().ok_or_else(|| bad("V_WIDTH"))?;
                have_vbw = true;
            }
            "FRONT_TO_BACK" => {
                p.front_to_back_db = num().ok_or_else(|| bad("FRONT_TO_BACK"))?;
                have_fb = true;
            }
            "GAIN" => {
                let toks: Vec<&str> = rest.split_whitespace().collect();
                let g = toks.first().and_then(|t| {
                    // allow "17.5dBi"
                    let lower = t.to_ascii_lowercase();
                    let numeric = lower.trim_end_matches("dbi").trim_end_matches("dbd");
                    parse_num(numeric)
                });
                let g = g.ok_or_else(|| bad("GAIN"))?;
                let joined = rest.to_ascii_lowercase();
                let unit = if joined.contains("dbi") {
                    GainUnit::Dbi
                } else if joined.contains("dbd") {
                    GainUnit::Dbd
                } else {
                    opts.default_gain_unit
                };
                p.gain_dbi = match unit {
                    GainUnit::Dbi => g,
                    GainUnit::Dbd => g + DIPOLE_GAIN_DBI,
                };
            }
            "TILT" => {
                let upper = rest.to_ascii_uppercase();
                let value = rest.split_whitespace().find_map(parse_num);
                if upper.starts_with("MECH") {
                    // mechanical tilt is a property of the installation, not of the pattern
                } else if let Some(v) = value {
                    p.electrical_tilt_deg = v;
                }
            }
            "ELECTRICAL_TILT" => p.electrical_tilt_deg = num().ok_or_else(|| bad("ELECTRICAL_TILT"))?,
            "MECHANICAL_TILT" | "TYPE" => {}
            "HORIZONTAL" | "VERTICAL" => {
                section = Some((kw == "HORIZONTAL", lineno));
                current.clear();
            }
            _ => {
                if parse_num(&kw).is_some() {
                    return Err(RfError::PatternParse {
                        line: lineno,
                        message: "numeric data outside of a HORIZONTAL/VERTICAL section".into(),
                    });
                }
                // unknown keyword: ignore
            }
        }
    }
    finish(&mut section, &mut current, &mut horizontal, &mut vertical)?;

    let h = horizontal.ok_or(RfError::PatternParse { line: 0, message: "missing HORIZONTAL section".into() })?;
    let v = vertical.ok_or(RfError::PatternParse { line: 0, message: "missing VERTICAL section".into() })?;
    if h.iter().chain(v.iter()).any(|(a, x)| !a.is_finite() || !x.is_finite()) {
        return Err(RfError::PatternParse { line: 0, message: "non-finite pattern value".into() });
    }
    // relative gain (≤ 0) instead of attenuation?
    let all_nonpos = h.iter().chain(v.iter()).all(|(_, x)| *x <= 0.0);
    let any_neg = h.iter().chain(v.iter()).any(|(_, x)| *x < 0.0);
    let sign = if all_nonpos && any_neg { -1.0 } else { 1.0 };
    let h: Vec<(f64, f64)> = h.into_iter().map(|(a, x)| (a, sign * x)).collect();
    let v: Vec<(f64, f64)> = v.into_iter().map(|(a, x)| (a, sign * x)).collect();
    p.horizontal = resample_360(&h);
    p.vertical = resample_360(&v);
    if !have_hbw {
        p.h_beamwidth_deg = p.measured_h_beamwidth_deg();
    }
    if !have_vbw {
        p.v_beamwidth_deg = p.measured_v_beamwidth_deg();
    }
    if !have_fb {
        p.front_to_back_db = p.measured_front_to_back_db();
    }
    Ok(p)
}

fn fmt_num(x: f64) -> String {
    let s = format!("{x:.3}");
    let s = s.trim_end_matches('0').trim_end_matches('.').to_string();
    if s == "-0" {
        "0".into()
    } else {
        s
    }
}

/// Write a pattern as MSI / Planet text. The gain is written with an explicit unit.
pub fn write_msi(p: &AntennaPattern, gain_unit: GainUnit) -> String {
    use std::fmt::Write;
    let mut s = String::new();
    let one_line = |x: &str| x.replace(['\r', '\n'], " ");
    let _ = writeln!(s, "NAME {}", one_line(&p.name));
    if !p.make.is_empty() {
        let _ = writeln!(s, "MAKE {}", one_line(&p.make));
    }
    let _ = writeln!(s, "FREQUENCY {}", fmt_num(p.frequency_mhz));
    let _ = writeln!(s, "H_WIDTH {}", fmt_num(p.h_beamwidth_deg));
    let _ = writeln!(s, "V_WIDTH {}", fmt_num(p.v_beamwidth_deg));
    let _ = writeln!(s, "FRONT_TO_BACK {}", fmt_num(p.front_to_back_db));
    match gain_unit {
        GainUnit::Dbi => {
            let _ = writeln!(s, "GAIN {} dBi", fmt_num(p.gain_dbi));
        }
        GainUnit::Dbd => {
            let _ = writeln!(s, "GAIN {} dBd", fmt_num(p.gain_dbi - DIPOLE_GAIN_DBI));
        }
    }
    let _ = writeln!(s, "TILT ELECTRICAL");
    if p.electrical_tilt_deg != 0.0 {
        let _ = writeln!(s, "ELECTRICAL_TILT {}", fmt_num(p.electrical_tilt_deg));
    }
    if !p.polarization.is_empty() {
        let _ = writeln!(s, "POLARIZATION {}", one_line(&p.polarization));
    }
    for c in p.comment.lines() {
        let _ = writeln!(s, "COMMENT {c}");
    }
    for (name, cut) in [("HORIZONTAL", &p.horizontal), ("VERTICAL", &p.vertical)] {
        let _ = writeln!(s, "{name} {}", cut.len());
        for (i, a) in cut.iter().enumerate() {
            let _ = writeln!(s, "{i} {:.2}", a);
        }
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::antenna::generators;

    #[test]
    fn roundtrip() {
        let mut p = generators::sector(
            1950.0,
            &generators::SectorParams { gain_dbi: 18.0, h_beamwidth_deg: 65.0, ..Default::default() },
        );
        p.comment = "line one\nline two".into();
        p.electrical_tilt_deg = 2.0;
        let txt = write_msi(&p, GainUnit::Dbi);
        let q = parse_msi(&txt, &MsiParseOptions::default()).unwrap();
        assert_eq!(q.name, p.name);
        assert!((q.gain_dbi - p.gain_dbi).abs() < 1e-3);
        assert!((q.frequency_mhz - 1950.0).abs() < 1e-9);
        assert_eq!(q.comment, p.comment);
        assert_eq!(q.electrical_tilt_deg, 2.0);
        for i in 0..360 {
            assert!((q.horizontal[i] - p.horizontal[i]).abs() < 0.006);
            assert!((q.vertical[i] - p.vertical[i]).abs() < 0.006);
        }
        let txt_dbd = write_msi(&p, GainUnit::Dbd);
        assert!(txt_dbd.contains("dBd"));
        let q2 = parse_msi(&txt_dbd, &MsiParseOptions::default()).unwrap();
        assert!((q2.gain_dbi - p.gain_dbi).abs() < 1e-3);
    }

    #[test]
    fn lenient_parsing() {
        let txt = "\u{feff}name  Test Antenna 72pt\r\n\
            make\tACME\n\
            Frequency 2.4 GHz\n\
            gain 15,5\n\
            tilt electrical 4\n\
            polarization vertical\n\
            some_unknown_keyword 42\n\
            horizontal 72\n";
        let mut s = txt.to_string();
        for i in 0..72 {
            let a = i * 5;
            s.push_str(&format!("{a}\t{},0  extra\n", if a <= 180 { a / 10 } else { (360 - a) / 10 }));
        }
        s.push_str("VERTICAL 4\n0 0\n90 20\n180 25\n270 20\n");
        let p = parse_msi(&s, &MsiParseOptions::default()).unwrap();
        assert_eq!(p.name, "Test Antenna 72pt");
        assert_eq!(p.make, "ACME");
        assert_eq!(p.frequency_mhz, 2400.0);
        // no unit → dBd default
        assert!((p.gain_dbi - 17.65).abs() < 1e-9);
        assert_eq!(p.electrical_tilt_deg, 4.0);
        assert_eq!(p.horizontal.len(), 360);
        // linear interpolation between 5° samples: at 7° → 0 + (1-0)*2/5 = 0.4 (values 0 at 0,0 at 5? 5/10=0)
        assert!((p.horizontal[10] - 1.0).abs() < 1e-6);
        assert!((p.horizontal[12] - 1.0).abs() < 1e-6 || p.horizontal[12] > 1.0);
        assert!((p.vertical[45] - 10.0).abs() < 1e-6);
        assert!((p.vertical[315] - 10.0).abs() < 1e-6); // wraps 270→360(=0)
        assert!(p.h_beamwidth_deg > 0.0);
        let opts = MsiParseOptions { default_gain_unit: GainUnit::Dbi };
        assert!((parse_msi(&s, &opts).unwrap().gain_dbi - 15.5).abs() < 1e-9);
    }

    #[test]
    fn comma_pairs_and_negative_values() {
        let mut s = String::from("NAME x\nGAIN 10 dBi\nHORIZONTAL 360\n");
        for i in 0..360 {
            s.push_str(&format!("{i},-{}.5\n", (i % 10)));
        }
        s.push_str("VERTICAL 360\n");
        for i in 0..=360 {
            s.push_str(&format!("{i} -1\n"));
        }
        let p = parse_msi(&s, &MsiParseOptions::default()).unwrap();
        assert!((p.horizontal[3] - 3.5).abs() < 1e-6, "{}", p.horizontal[3]);
        assert!((p.vertical[100] - 1.0).abs() < 1e-6);
        assert_eq!(p.gain_dbi, 10.0);
    }

    #[test]
    fn errors() {
        assert!(matches!(
            parse_msi("NAME a\nHORIZONTAL 360\n0 0\n", &MsiParseOptions::default()),
            Err(RfError::PatternParse { .. })
        ));
        assert!(parse_msi("NAME a\nVERTICAL 2\n0 0\n180 3\n", &MsiParseOptions::default()).is_err());
        assert!(
            parse_msi("NAME a\nGAIN abc\nHORIZONTAL 1\n0 0\nVERTICAL 1\n0 0\n", &MsiParseOptions::default()).is_err()
        );
        assert!(parse_msi("NAME a\nHORIZONTAL 360\nVERTICAL 1\n0 0\n", &MsiParseOptions::default()).is_err());
    }
}
