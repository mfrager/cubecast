//! The small JSON contract the Python shim returns.
//!
//! Artefacts stay on disk; only paths, versions, and warnings cross the FFI
//! boundary.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

/// One extracted image.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ManifestAsset {
    /// Path relative to the run directory.
    pub path: PathBuf,
    /// MIME type when known.
    #[serde(default)]
    pub mime: String,
    /// Page the asset belongs to.
    #[serde(default)]
    pub page: Option<u32>,
}

/// What the shim writes and reports.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Manifest {
    /// Shim contract version.
    pub shim_version: String,
    /// MinerU version.
    pub mineru_version: String,
    /// CPython version.
    #[serde(default)]
    pub python_version: String,
    /// MinerU backend.
    #[serde(default = "default_backend")]
    pub backend: String,
    /// MinerU model tier.
    #[serde(default = "default_tier")]
    pub tier: String,
    /// Parsed schema identifier.
    pub schema: String,
    /// Parsed schema version.
    pub schema_version: String,
    /// `middle.json`, when produced.
    #[serde(default)]
    pub middle_json: Option<PathBuf>,
    /// NLP Markdown, when produced.
    #[serde(default)]
    pub markdown: Option<PathBuf>,
    /// Content list, when the release only produces that.
    #[serde(default)]
    pub content_list: Option<PathBuf>,
    /// Extracted images.
    #[serde(default)]
    pub assets: Vec<ManifestAsset>,
    /// Non-fatal findings from the shim.
    #[serde(default)]
    pub warnings: Vec<String>,
}

fn default_backend() -> String {
    "pipeline".to_owned()
}

fn default_tier() -> String {
    "default".to_owned()
}

impl Manifest {
    /// Checks the fields the Rust side cannot proceed without.
    ///
    /// # Errors
    ///
    /// Returns [`ontology_core::error::Error::Extraction`] when a required
    /// field is empty or no artefact path was reported.
    pub fn validate(&self) -> ontology_core::error::Result<()> {
        if self.schema.trim().is_empty() || self.schema_version.trim().is_empty() {
            return Err(ontology_core::error::Error::extraction(
                "manifest",
                "shim reported an empty schema identifier or version",
            ));
        }
        if self.mineru_version.trim().is_empty() {
            return Err(ontology_core::error::Error::extraction(
                "manifest",
                "shim reported an empty MinerU version",
            ));
        }
        if self.middle_json.is_none() && self.content_list.is_none() {
            return Err(ontology_core::error::Error::extraction(
                "manifest",
                "shim reported neither middle_json nor content_list",
            ));
        }
        Ok(())
    }
}
