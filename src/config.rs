//! The TOML config file.

use crate::types::Level;
use serde::Deserialize;

/// Contents of a replysieve config file. Every field is optional.
///
/// ```toml
/// level = "medium"
/// preset = "launch-thread"
/// models = ["~typesafe/jev-latest"]
/// rules = ["No links to other products"]
/// stock_phrases = ["nice ship"]
/// ```
#[derive(Debug, Clone, Default, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Config {
    /// Default strictness level.
    pub level: Option<Level>,
    /// Built-in preset name.
    pub preset: Option<String>,
    /// OpenRouter decision models, tried in order.
    #[serde(default)]
    pub models: Vec<String>,
    /// Plain-English rules enforced at every level.
    #[serde(default)]
    pub rules: Vec<String>,
    /// Extra stock phrases for the free pre-check.
    #[serde(default)]
    pub stock_phrases: Vec<String>,
}

impl Config {
    /// Parse a config from TOML text.
    pub fn from_toml(text: &str) -> anyhow::Result<Self> {
        Ok(toml::from_str(text)?)
    }

    /// Read and parse a config file. Not available on WebAssembly; pass the
    /// file's text to [`Config::from_toml`] there.
    #[cfg(not(target_arch = "wasm32"))]
    pub fn load(path: &std::path::Path) -> anyhow::Result<Self> {
        use anyhow::Context;
        let text = std::fs::read_to_string(path)
            .with_context(|| format!("cannot read config file {}", path.display()))?;
        Self::from_toml(&text).with_context(|| format!("invalid config file {}", path.display()))
    }
}
