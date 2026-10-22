//! Error types for `pathless-rf`.

use thiserror::Error;

/// Errors returned by fallible functions of this crate.
#[derive(Debug, Error)]
pub enum RfError {
    /// A numeric input was outside its valid domain (negative distance, NaN, …).
    #[error("invalid input: {0}")]
    InvalidInput(String),

    /// A terrain profile was malformed (length mismatch, too few points, not increasing).
    #[error("invalid terrain profile: {0}")]
    InvalidProfile(String),

    /// Unknown ITU-R P.837-1 rain climate zone letter.
    #[error("unknown ITU-R P.837-1 rain zone '{0}' (valid: A B C D E F G H J K L M N P Q)")]
    UnknownRainZone(char),

    /// Error while parsing an MSI / Planet antenna pattern file.
    #[error("antenna pattern parse error at line {line}: {message}")]
    PatternParse {
        /// 1-based line number (0 when not attributable to a line).
        line: usize,
        /// Human readable description.
        message: String,
    },

    /// Error while parsing an antenna catalog JSON document.
    #[error("antenna catalog JSON error: {0}")]
    CatalogJson(#[from] serde_json::Error),

    /// I/O error (reading user catalog directories, pattern files, …).
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
}

impl RfError {
    pub(crate) fn invalid(msg: impl Into<String>) -> Self {
        RfError::InvalidInput(msg.into())
    }
}

/// Validate that `x` is finite and strictly positive.
pub(crate) fn require_positive(name: &str, x: f64) -> Result<(), RfError> {
    if x.is_finite() && x > 0.0 {
        Ok(())
    } else {
        Err(RfError::invalid(format!("{name} must be finite and > 0 (got {x})")))
    }
}

/// Validate that `x` is finite.
pub(crate) fn require_finite(name: &str, x: f64) -> Result<(), RfError> {
    if x.is_finite() {
        Ok(())
    } else {
        Err(RfError::invalid(format!("{name} must be finite (got {x})")))
    }
}
