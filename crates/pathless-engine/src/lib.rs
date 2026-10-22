//! PathLess analysis engine: ties terrain/clutter data, propagation models and
//! antenna patterns together into point-to-point link reports and area
//! coverage grids.

pub mod antennas;
pub mod clutter;
pub mod coverage;
pub mod dataset;
pub mod link;
pub mod project;
pub mod propagation;
pub mod render;

pub use pathless_data::{DataManager, Progress};
