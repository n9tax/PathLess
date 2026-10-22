//! Project file model. Field names (camelCase) match the UI's JavaScript
//! model one-to-one, so the frontend can send the whole project as JSON.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Project {
    #[serde(default = "one")]
    pub version: u32,
    pub name: String,
    /// Study-area polygon ring as `[lon, lat]` pairs (not closed).
    #[serde(default)]
    pub area: Vec<[f64; 2]>,
    pub data: DataSettings,
    pub env: Environment,
    #[serde(default)]
    pub sites: Vec<Site>,
    #[serde(default)]
    pub links: Vec<Link>,
    pub coverage: CoverageSettings,
}

fn one() -> u32 {
    1
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum DemChoice {
    ArcSec1,
    ArcSecThird,
    ArcSecNinth,
    Meter1,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DataSettings {
    pub dem_resolution: DemChoice,
    pub use_clutter: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ClimateChoice {
    Equatorial,
    ContinentalSubtropical,
    MaritimeSubtropical,
    Desert,
    ContinentalTemperate,
    MaritimeTemperateOverLand,
    MaritimeTemperateOverSea,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Variability {
    SingleMessage,
    Accidental,
    Mobile,
    Broadcast,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ModelChoice {
    /// ITM below 3 GHz, free space + diffraction (+ P.530 availability) above.
    Auto,
    Itm,
    Microwave,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Pol {
    Vertical,
    Horizontal,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Environment {
    pub climate: ClimateChoice,
    /// Surface refractivity, N-units.
    pub n0: f64,
    pub ground: String,
    pub epsilon: f64,
    pub sigma: f64,
    pub k_factor: f64,
    pub time_pct: f64,
    pub location_pct: f64,
    pub situation_pct: f64,
    pub variability: Variability,
    pub model: ModelChoice,
    /// NLCD class code (as string key) -> obstruction height in metres.
    #[serde(default)]
    pub clutter_heights: BTreeMap<String, f64>,
    #[serde(default)]
    pub terminal_clutter_loss: bool,
}

impl Environment {
    pub fn clutter_height(&self, class: u8) -> f64 {
        self.clutter_heights.get(&class.to_string()).copied().unwrap_or(0.0)
    }

    /// Lookup table indexed by NLCD class code.
    pub fn clutter_table(&self) -> [f32; 256] {
        let mut t = [0f32; 256];
        for (k, v) in &self.clutter_heights {
            if let Ok(c) = k.parse::<u8>() {
                t[c as usize] = *v as f32;
            }
        }
        t
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Site {
    pub id: String,
    pub name: String,
    pub lat: f64,
    pub lon: f64,
    #[serde(default)]
    pub color: String,
    pub antenna_height_m: f64,
    pub antenna_id: Option<String>,
    pub azimuth_deg: f64,
    pub downtilt_deg: f64,
    pub tx_power_dbm: f64,
    pub line_loss_db: f64,
    pub other_loss_db: f64,
    pub rx_sensitivity_dbm: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Link {
    pub id: String,
    pub name: String,
    pub site_a: String,
    pub site_b: String,
    pub frequency_mhz: f64,
    pub polarization: Pol,
    pub rain_zone: String,
    pub custom_r001: Option<f64>,
    pub dn1: f64,
    pub target_availability_pct: f64,
    /// Assume both antennas are aimed along the path (boresight gain), ignoring
    /// the sites' azimuth / downtilt settings.
    #[serde(default = "yes")]
    pub align_antennas: bool,
}

fn yes() -> bool {
    true
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum CoverageOutput {
    RxDbm,
    FieldDbuvm,
    PathLossDb,
}

impl CoverageOutput {
    pub fn unit(self) -> &'static str {
        match self {
            CoverageOutput::RxDbm => "dBm",
            CoverageOutput::FieldDbuvm => "dBµV/m",
            CoverageOutput::PathLossDb => "dB",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LegendEntry {
    pub value: f64,
    pub color: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CoverageSettings {
    pub site_id: Option<String>,
    pub frequency_mhz: f64,
    pub polarization: Pol,
    pub rx_height_m: f64,
    pub rx_gain_dbi: f64,
    pub rx_line_loss_db: f64,
    pub resolution_m: f64,
    pub max_radius_km: f64,
    pub output: CoverageOutput,
    pub legend: Vec<LegendEntry>,
}

impl Project {
    pub fn site(&self, id: &str) -> Option<&Site> {
        self.sites.iter().find(|s| s.id == id)
    }

    pub fn link(&self, id: &str) -> Option<&Link> {
        self.links.iter().find(|l| l.id == id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_ui_project_json() {
        let json = r##"{
          "version": 1, "name": "t", "area": [[-105.3, 39.9], [-105.2, 39.9], [-105.2, 40.0]],
          "data": {"demResolution": "arcSecThird", "useClutter": true},
          "env": {"climate": "continentalTemperate", "n0": 301, "ground": "average", "epsilon": 15,
                  "sigma": 0.005, "kFactor": 1.3333, "timePct": 50, "locationPct": 50, "situationPct": 90,
                  "variability": "accidental", "model": "auto", "clutterHeights": {"41": 15},
                  "terminalClutterLoss": true},
          "sites": [{"id": "s1", "name": "A", "lat": 39.95, "lon": -105.25, "color": "#fff",
                     "antennaHeightM": 30, "antennaId": null, "azimuthDeg": 0, "downtiltDeg": 0,
                     "txPowerDbm": 40, "lineLossDb": 2, "otherLossDb": 0, "rxSensitivityDbm": -100,
                     "_groundM": 1650.2}],
          "links": [],
          "coverage": {"siteId": "s1", "frequencyMhz": 450, "polarization": "vertical", "rxHeightM": 1.5,
                       "rxGainDbi": 0, "rxLineLossDb": 0, "resolutionM": 30, "maxRadiusKm": 25,
                       "output": "rxDbm", "legend": [{"value": -60, "color": "#1a9850"}]}
        }"##;
        let p: Project = serde_json::from_str(json).unwrap();
        assert_eq!(p.sites[0].name, "A");
        assert_eq!(p.env.clutter_height(41), 15.0);
        assert_eq!(p.env.clutter_table()[41], 15.0);
        assert_eq!(p.data.dem_resolution, DemChoice::ArcSecThird);
    }
}
