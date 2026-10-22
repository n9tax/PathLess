//! Built-in generic antenna catalog and user catalog loading.
//!
//! The built-in catalog lives in `data/antenna_catalog.json` (embedded with
//! `include_str!`) so that it can be reviewed and extended without touching code. Each
//! entry is an [`AntennaSpec`] whose [`AntennaKind`] holds generator parameters; the actual
//! pattern is produced on demand for the analysis frequency with
//! [`AntennaSpec::pattern_at`].
//!
//! JSON example:
//!
//! ```json
//! { "id": "sector-3500-65deg", "name": "Sector 65deg 18 dBi, 3.5 GHz", "category": "Sector",
//!   "min_mhz": 3300, "max_mhz": 3800,
//!   "kind": { "type": "sector", "gain_dbi": 18.0, "h_beamwidth_deg": 65.0, "front_to_back_db": 28.0 },
//!   "notes": "..." }
//! ```

use std::path::Path;

use serde::{Deserialize, Serialize};

use super::generators::{self, DishParams, OmniParams, SectorParams, YagiParams};
use super::msi::{parse_msi, MsiParseOptions};
use super::AntennaPattern;
use crate::error::RfError;

const BUILTIN_JSON: &str = include_str!("../../data/antenna_catalog.json");

/// Antenna model: generator parameters or a measured pattern.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum AntennaKind {
    /// Isotropic radiator.
    Isotropic,
    /// Vertical half-wave dipole (also used for unity-gain whips / ground planes).
    Dipole,
    /// F.1336 omnidirectional antenna.
    Omni(OmniParams),
    /// F.1336 sector / panel antenna.
    Sector(SectorParams),
    /// Empirical yagi.
    Yagi(YagiParams),
    /// Parabolic dish (F.699 / F.1245).
    Dish(DishParams),
    /// A fixed measured pattern (e.g. loaded from an MSI file); not frequency scaled.
    Measured {
        /// The pattern.
        pattern: Box<AntennaPattern>,
    },
}

/// A catalog entry.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AntennaSpec {
    /// Unique identifier (user MSI files get `user:<file stem>`).
    pub id: String,
    /// Display name.
    pub name: String,
    /// Category: "Omni", "Sector", "Yagi", "Dish", "Dipole" or "Panel".
    pub category: String,
    /// Lower end of the intended frequency range (MHz).
    pub min_mhz: f64,
    /// Upper end of the intended frequency range (MHz).
    pub max_mhz: f64,
    /// Model parameters.
    pub kind: AntennaKind,
    /// Free-text notes.
    #[serde(default)]
    pub notes: String,
}

impl AntennaSpec {
    /// True if `f_mhz` lies within the spec's frequency range.
    pub fn covers(&self, f_mhz: f64) -> bool {
        f_mhz >= self.min_mhz && f_mhz <= self.max_mhz
    }

    /// Generate the pattern at `f_mhz`. Frequencies outside `min_mhz..=max_mhz` are
    /// accepted (the generators are frequency-generic) – check [`AntennaSpec::covers`] in
    /// the UI if needed. Measured patterns are returned unchanged.
    pub fn pattern_at(&self, f_mhz: f64) -> AntennaPattern {
        let mut p = match &self.kind {
            AntennaKind::Isotropic => generators::isotropic(f_mhz),
            AntennaKind::Dipole => generators::half_wave_dipole(f_mhz),
            AntennaKind::Omni(o) => generators::omni_collinear(f_mhz, o),
            AntennaKind::Sector(s) => generators::sector(f_mhz, s),
            AntennaKind::Yagi(y) => generators::yagi(f_mhz, y),
            AntennaKind::Dish(d) => generators::parabolic_dish(f_mhz, d),
            AntennaKind::Measured { pattern } => return (**pattern).clone(),
        };
        p.name = self.name.clone();
        if !self.notes.is_empty() {
            p.comment = format!("{} | {}", p.comment, self.notes);
        }
        p
    }
}

/// The built-in catalog of generic antennas (parsed from the embedded JSON).
pub fn builtin_catalog() -> Vec<AntennaSpec> {
    serde_json::from_str(BUILTIN_JSON).expect("embedded antenna_catalog.json is valid")
}

/// Find a spec by id.
pub fn find_spec<'a>(catalog: &'a [AntennaSpec], id: &str) -> Option<&'a AntennaSpec> {
    catalog.iter().find(|s| s.id == id)
}

/// Result of [`load_catalog_from_dir`].
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CatalogLoad {
    /// Built-in specs merged with the user specs (user entries replace built-ins with the
    /// same id).
    pub specs: Vec<AntennaSpec>,
    /// Non-fatal problems: `"<file>: <error>"` for files that could not be parsed.
    pub warnings: Vec<String>,
}

fn guess_category(p: &AntennaPattern) -> &'static str {
    let max_h = p.horizontal.iter().fold(0.0f32, |m, x| m.max(*x));
    if max_h < 3.0 || p.h_beamwidth_deg >= 300.0 {
        "Omni"
    } else if p.gain_dbi >= 24.0 {
        "Dish"
    } else if p.h_beamwidth_deg >= 45.0 {
        "Sector"
    } else {
        "Panel"
    }
}

/// Build a catalog entry from a measured MSI pattern.
pub fn spec_from_pattern(id: impl Into<String>, pattern: AntennaPattern) -> AntennaSpec {
    let f = pattern.frequency_mhz;
    let (min_mhz, max_mhz) = if f > 0.0 { (f * 0.9, f * 1.1) } else { (0.0, 1.0e6) };
    AntennaSpec {
        id: id.into(),
        name: pattern.name.clone(),
        category: guess_category(&pattern).into(),
        min_mhz,
        max_mhz,
        notes: pattern.comment.clone(),
        kind: AntennaKind::Measured { pattern: Box::new(pattern) },
    }
}

fn merge(specs: &mut Vec<AntennaSpec>, s: AntennaSpec) {
    if let Some(existing) = specs.iter_mut().find(|e| e.id == s.id) {
        *existing = s;
    } else {
        specs.push(s);
    }
}

/// Load the built-in catalog and merge user antennas from `dir`:
///
/// * `*.msi`, `*.pln`, `*.ant`, `*.txt` – MSI/Planet pattern files (id `user:<stem>`);
/// * `*.json` – one [`AntennaSpec`] or an array of them.
///
/// Unparseable files are reported in [`CatalogLoad::warnings`]; only failure to read the
/// directory itself is an error. Files are processed in name order.
pub fn load_catalog_from_dir(dir: &Path) -> Result<CatalogLoad, RfError> {
    let mut specs = builtin_catalog();
    let mut warnings = Vec::new();
    let mut entries: Vec<_> =
        std::fs::read_dir(dir)?.filter_map(|e| e.ok()).map(|e| e.path()).filter(|p| p.is_file()).collect();
    entries.sort();
    for path in entries {
        let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("").to_ascii_lowercase();
        let display = path.display().to_string();
        match ext.as_str() {
            "msi" | "pln" | "ant" | "txt" => {
                let text = match std::fs::read(&path) {
                    Ok(b) => String::from_utf8_lossy(&b).into_owned(),
                    Err(e) => {
                        warnings.push(format!("{display}: {e}"));
                        continue;
                    }
                };
                match parse_msi(&text, &MsiParseOptions::default()) {
                    Ok(mut p) => {
                        let stem = path.file_stem().and_then(|s| s.to_str()).unwrap_or("pattern").to_string();
                        if p.name.is_empty() {
                            p.name = stem.clone();
                        }
                        merge(&mut specs, spec_from_pattern(format!("user:{stem}"), p));
                    }
                    Err(e) => warnings.push(format!("{display}: {e}")),
                }
            }
            "json" => {
                let text = match std::fs::read_to_string(&path) {
                    Ok(t) => t,
                    Err(e) => {
                        warnings.push(format!("{display}: {e}"));
                        continue;
                    }
                };
                let parsed: Result<Vec<AntennaSpec>, _> = serde_json::from_str::<Vec<AntennaSpec>>(&text)
                    .or_else(|_| serde_json::from_str::<AntennaSpec>(&text).map(|s| vec![s]));
                match parsed {
                    Ok(list) => list.into_iter().for_each(|s| merge(&mut specs, s)),
                    Err(e) => warnings.push(format!("{display}: {e}")),
                }
            }
            _ => {}
        }
    }
    Ok(CatalogLoad { specs, warnings })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builtin_catalog_is_valid() {
        let cat = builtin_catalog();
        assert!(cat.len() >= 80, "{}", cat.len());
        let mut ids: Vec<&str> = cat.iter().map(|s| s.id.as_str()).collect();
        ids.sort();
        ids.dedup();
        assert_eq!(ids.len(), cat.len(), "duplicate ids");
        for s in &cat {
            assert!(["Omni", "Sector", "Yagi", "Dish", "Dipole", "Panel"].contains(&s.category.as_str()), "{}", s.id);
            assert!(s.min_mhz < s.max_mhz, "{}", s.id);
            let f = (s.min_mhz * s.max_mhz).sqrt();
            let p = s.pattern_at(f);
            p.validate().unwrap();
            assert_eq!(p.name, s.name);
            assert!(p.horizontal.iter().chain(p.vertical.iter()).all(|x| *x >= 0.0 && *x < 80.0), "{}", s.id);
            // the minimum attenuation of each cut should be ~0 (peak)
            let hmin = p.horizontal.iter().fold(f32::INFINITY, |m, x| m.min(*x));
            assert!(hmin < 0.01, "{}: {hmin}", s.id);
            assert!(p.gain_dbi > -1.0 && p.gain_dbi < 70.0, "{}", s.id);
        }
        // spot checks
        let d = find_spec(&cat, "dish-0.6m").unwrap().pattern_at(23_000.0);
        assert!((d.gain_dbi - 40.6).abs() < 0.1);
        let o = find_spec(&cat, "omni-uhf-6dbi").unwrap().pattern_at(450.0);
        assert_eq!(o.gain_dbi, 6.0);
        let g = find_spec(&cat, "grid-2g4-0.9m").unwrap().pattern_at(2450.0);
        assert!((g.gain_dbi - 24.0).abs() < 1.0, "{}", g.gain_dbi);
        assert!(find_spec(&cat, "dish-5g-0.6m").unwrap().covers(5800.0));
    }

    #[test]
    fn serde_roundtrip() {
        let cat = builtin_catalog();
        let json = serde_json::to_string(&cat).unwrap();
        let back: Vec<AntennaSpec> = serde_json::from_str(&json).unwrap();
        assert_eq!(back, cat);
        let m = spec_from_pattern("user:x", generators::isotropic(100.0));
        let j = serde_json::to_string(&m).unwrap();
        assert!(j.contains("\"type\":\"measured\""));
        let back: AntennaSpec = serde_json::from_str(&j).unwrap();
        assert_eq!(back.pattern_at(5000.0).frequency_mhz, 100.0);
        assert_eq!(back.category, "Omni");
    }

    #[test]
    fn load_user_dir() {
        let dir = std::env::temp_dir().join(format!("pathless-rf-cat-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let p = generators::yagi(450.0, &YagiParams { gain_dbi: 12.0, ..Default::default() });
        std::fs::write(dir.join("my_yagi.msi"), super::super::msi::write_msi(&p, super::super::GainUnit::Dbi)).unwrap();
        std::fs::write(dir.join("broken.pln"), "NAME broken\n").unwrap();
        std::fs::write(
            dir.join("custom.json"),
            r#"[{"id":"omni-vhf-3dbi","name":"Overridden","category":"Omni","min_mhz":136,"max_mhz":174,
                "kind":{"type":"omni","gain_dbi":4.5}},
               {"id":"my-dish","name":"My dish","category":"Dish","min_mhz":10000,"max_mhz":12000,
                "kind":{"type":"dish","diameter_m":1.0}}]"#,
        )
        .unwrap();
        std::fs::write(dir.join("ignore.bin"), [0u8, 1, 2]).unwrap();
        let load = load_catalog_from_dir(&dir).unwrap();
        std::fs::remove_dir_all(&dir).ok();
        assert_eq!(load.warnings.len(), 1, "{:?}", load.warnings);
        let y = find_spec(&load.specs, "user:my_yagi").unwrap();
        assert!(matches!(y.kind, AntennaKind::Measured { .. }));
        assert!((y.pattern_at(450.0).gain_dbi - 12.0).abs() < 1e-3);
        assert_eq!(find_spec(&load.specs, "omni-vhf-3dbi").unwrap().name, "Overridden");
        let d = find_spec(&load.specs, "my-dish").unwrap();
        if let AntennaKind::Dish(dp) = &d.kind {
            assert_eq!(dp.efficiency, 0.55);
        } else {
            panic!("wrong kind");
        }
        assert_eq!(load.specs.len(), builtin_catalog().len() + 2);
        assert!(load_catalog_from_dir(Path::new("/nonexistent/pathless")).is_err());
    }
}
