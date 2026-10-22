//! Antenna library: the built-in generic catalog plus user-imported MSI files.

use std::path::{Path, PathBuf};

use anyhow::Context as _;
use pathless_rf::antenna::{builtin_catalog, load_catalog_from_dir, AntennaPattern, AntennaSpec};
use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AntennaSummary {
    pub id: String,
    pub name: String,
    pub category: String,
    pub min_mhz: f64,
    pub max_mhz: f64,
    pub notes: String,
    /// "builtin" or "user".
    pub source: &'static str,
}

#[derive(Debug, Clone)]
pub struct AntennaLibrary {
    specs: Vec<AntennaSpec>,
    user_dir: Option<PathBuf>,
}

impl AntennaLibrary {
    /// Built-in catalog merged with patterns/specs found in `user_dir`.
    pub fn load(user_dir: Option<PathBuf>) -> anyhow::Result<Self> {
        let specs = match &user_dir {
            Some(dir) if dir.is_dir() => {
                let load = load_catalog_from_dir(dir)?;
                for w in &load.warnings {
                    tracing::warn!("antenna library: {w}");
                }
                load.specs
            }
            _ => builtin_catalog(),
        };
        Ok(AntennaLibrary { specs, user_dir })
    }

    pub fn list(&self) -> Vec<AntennaSummary> {
        self.specs
            .iter()
            .map(|s| AntennaSummary {
                id: s.id.clone(),
                name: s.name.clone(),
                category: s.category.clone(),
                min_mhz: s.min_mhz,
                max_mhz: s.max_mhz,
                notes: s.notes.clone(),
                source: if s.id.starts_with("user:") { "user" } else { "builtin" },
            })
            .collect()
    }

    pub fn spec(&self, id: &str) -> Option<&AntennaSpec> {
        self.specs.iter().find(|s| s.id == id)
    }

    /// Pattern for `id` at `f_mhz`; `None` (or an unknown id) gives an isotropic radiator.
    pub fn pattern(&self, id: Option<&str>, f_mhz: f64) -> anyhow::Result<AntennaPattern> {
        match id {
            None | Some("") => Ok(pathless_rf::antenna::generators::isotropic(f_mhz)),
            Some(id) => self.spec(id).map(|s| s.pattern_at(f_mhz)).with_context(|| format!("Unknown antenna '{id}'")),
        }
    }

    /// Validate an MSI/Planet file, copy it into the user library and reload.
    pub fn import(&mut self, path: &Path) -> anyhow::Result<AntennaSummary> {
        let dir = self.user_dir.clone().context("No user antenna directory configured")?;
        let text = std::fs::read_to_string(path).with_context(|| format!("Reading {}", path.display()))?;
        AntennaPattern::from_msi_str(&text).with_context(|| format!("Parsing {}", path.display()))?;
        std::fs::create_dir_all(&dir)?;
        let file_name = path.file_name().context("Invalid file name")?;
        std::fs::copy(path, dir.join(file_name))?;
        *self = AntennaLibrary::load(Some(dir))?;
        let stem = path.file_stem().and_then(|s| s.to_str()).unwrap_or_default();
        let id = format!("user:{stem}");
        self.list()
            .into_iter()
            .find(|a| a.id == id)
            .with_context(|| format!("Imported pattern '{id}' not found after reload"))
    }
}
