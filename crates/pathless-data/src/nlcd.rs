//! NLCD land-cover classes: names and the official NLCD legend colors.
//!
//! Classes 51, 72, 73, 74 occur only in Alaska. Code 0 is nodata in this crate's rasters.

use serde::{Deserialize, Serialize};

/// One NLCD class.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct NlcdClass {
    pub code: u8,
    pub name: &'static str,
    /// Official NLCD legend color (sRGB).
    pub rgb: [u8; 3],
    /// Present only in the Alaska product.
    pub alaska_only: bool,
}

const fn c(code: u8, name: &'static str, rgb: [u8; 3], alaska_only: bool) -> NlcdClass {
    NlcdClass { code, name, rgb, alaska_only }
}

/// All 20 NLCD land-cover classes in code order.
pub const NLCD_CLASSES: [NlcdClass; 20] = [
    c(11, "Open Water", [0x46, 0x6b, 0x9f], false),
    c(12, "Perennial Ice/Snow", [0xd1, 0xde, 0xf8], false),
    c(21, "Developed, Open Space", [0xde, 0xc5, 0xc5], false),
    c(22, "Developed, Low Intensity", [0xd9, 0x92, 0x82], false),
    c(23, "Developed, Medium Intensity", [0xeb, 0x00, 0x00], false),
    c(24, "Developed, High Intensity", [0xab, 0x00, 0x00], false),
    c(31, "Barren Land (Rock/Sand/Clay)", [0xb3, 0xac, 0x9f], false),
    c(41, "Deciduous Forest", [0x68, 0xab, 0x5f], false),
    c(42, "Evergreen Forest", [0x1c, 0x5f, 0x2c], false),
    c(43, "Mixed Forest", [0xb5, 0xc5, 0x8f], false),
    c(51, "Dwarf Scrub", [0xaf, 0x96, 0x3c], true),
    c(52, "Shrub/Scrub", [0xcc, 0xb8, 0x79], false),
    c(71, "Grassland/Herbaceous", [0xdf, 0xdf, 0xc2], false),
    c(72, "Sedge/Herbaceous", [0xd1, 0xd1, 0x82], true),
    c(73, "Lichens", [0xa3, 0xcc, 0x51], true),
    c(74, "Moss", [0x82, 0xba, 0x9e], true),
    c(81, "Pasture/Hay", [0xdc, 0xd9, 0x39], false),
    c(82, "Cultivated Crops", [0xab, 0x6c, 0x28], false),
    c(90, "Woody Wetlands", [0xb8, 0xd9, 0xeb], false),
    c(95, "Emergent Herbaceous Wetlands", [0x6c, 0x9f, 0xb8], false),
];

/// Look up a class by code.
pub fn class(code: u8) -> Option<&'static NlcdClass> {
    NLCD_CLASSES.iter().find(|c| c.code == code)
}

/// Class name, or "No Data" for 0 / "Unknown" for unrecognised codes.
pub fn class_name(code: u8) -> &'static str {
    match code {
        0 => "No Data",
        _ => class(code).map_or("Unknown", |c| c.name),
    }
}

/// Legend color; transparent-ish black `[0,0,0]` for nodata/unknown.
pub fn class_color(code: u8) -> [u8; 3] {
    class(code).map_or([0, 0, 0], |c| c.rgb)
}

/// 256-entry RGBA lookup table (alpha 0 for nodata/unknown) for fast map rendering.
pub fn rgba_lut() -> [[u8; 4]; 256] {
    let mut lut = [[0u8; 4]; 256];
    for c in NLCD_CLASSES.iter() {
        lut[c.code as usize] = [c.rgb[0], c.rgb[1], c.rgb[2], 255];
    }
    lut
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn table() {
        assert_eq!(NLCD_CLASSES.len(), 20);
        assert_eq!(class_name(42), "Evergreen Forest");
        assert_eq!(class_name(0), "No Data");
        assert_eq!(class_name(99), "Unknown");
        assert_eq!(class_color(11), [70, 107, 159]);
        assert_eq!(NLCD_CLASSES.iter().filter(|c| c.alaska_only).count(), 4);
        assert_eq!(rgba_lut()[95][3], 255);
        assert_eq!(rgba_lut()[0][3], 0);
    }
}
