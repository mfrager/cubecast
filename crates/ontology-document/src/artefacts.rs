//! Content-addressed artefact storage and the extraction cache.
//!
//! Layout:
//!
//! ```text
//! <root>/sha256/<doc-hex>/<run-hex>/
//!     manifest.json     ExtractionOutput without the model bodies
//!     middle.json       raw MinerU artefact
//!     text.md           text projection
//!     structured.json   canonical model
//!     images/…
//! ```

use std::path::{Path, PathBuf};

use ontology_core::document::{
    ExtractionMode, ExtractionOutput, StructuredDocument, TextFormat, TextOutput,
};
use ontology_core::error::{Error, Result};

use crate::ids::hex_suffix;

/// Manifest file name inside a run directory.
pub const MANIFEST: &str = "manifest.json";
/// Canonical structured document file name.
pub const STRUCTURED: &str = "structured.json";
/// Markdown text file name.
pub const TEXT_MARKDOWN: &str = "text.md";
/// Plain text file name.
pub const TEXT_PLAIN: &str = "text.txt";

/// Stores extraction runs under one root directory.
#[derive(Debug, Clone)]
pub struct ArtefactStore {
    root: PathBuf,
}

impl ArtefactStore {
    /// Creates a store rooted at `root`.
    #[must_use]
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }

    /// Returns the root directory.
    #[must_use]
    pub fn root(&self) -> &Path {
        &self.root
    }

    /// Returns the directory holding every run of one document.
    ///
    /// # Errors
    ///
    /// Returns [`Error::Invalid`] for a malformed document id.
    pub fn document_dir(&self, document_id: &str) -> Result<PathBuf> {
        Ok(self.root.join("sha256").join(hex_suffix(document_id)?))
    }

    /// Creates the run directory.
    ///
    /// # Errors
    ///
    /// Returns [`Error::Io`] when the directory cannot be created.
    pub fn prepare_run(&self, document_id: &str, run_id: &str) -> Result<PathBuf> {
        let dir = crate::ids::run_directory(&self.root, document_id, run_id)?;
        std::fs::create_dir_all(&dir).map_err(|error| Error::Io {
            detail: format!("could not create {}: {error}", dir.display()),
        })?;
        Ok(dir)
    }

    /// Persists the model and text bodies plus the manifest.
    ///
    /// The output's `artefacts` field is updated to point at the written
    /// files.
    ///
    /// # Errors
    ///
    /// Returns [`Error::Io`] or [`Error::Serialization`] when a file cannot be
    /// written or the manifest cannot be encoded.
    pub fn persist(&self, output: &mut ExtractionOutput) -> Result<()> {
        let dir = output.artefacts.run_dir.clone();
        if dir.as_os_str().is_empty() {
            return Err(Error::Extraction {
                stage: "store",
                detail: "extraction produced no run directory".to_owned(),
            });
        }
        if !dir.exists() {
            std::fs::create_dir_all(&dir).map_err(|error| Error::Io {
                detail: format!("could not create {}: {error}", dir.display()),
            })?;
        }
        self.ensure_within_root(&dir)?;

        if let Some(document) = &output.document {
            let path = dir.join(STRUCTURED);
            write_json(&path, document)?;
            output.artefacts.structured_json = Some(path);
        }
        if let Some(text) = &output.text {
            let name = match text.format {
                TextFormat::Markdown => TEXT_MARKDOWN,
                TextFormat::Plain => TEXT_PLAIN,
            };
            let path = dir.join(name);
            std::fs::write(&path, text.content.as_bytes()).map_err(|error| Error::Io {
                detail: format!("could not write {}: {error}", path.display()),
            })?;
            output.artefacts.text = Some(path);
        }

        let mut manifest = output.clone();
        manifest.document = None;
        manifest.text = None;
        write_json(&dir.join(MANIFEST), &manifest)
    }

    /// Loads a stored run, restoring the model and text bodies.
    ///
    /// # Errors
    ///
    /// Returns [`Error::NotFound`] when the manifest is missing, or a
    /// serialization/IO error when it cannot be restored.
    pub fn load(&self, run_dir: &Path) -> Result<ExtractionOutput> {
        let manifest_path = run_dir.join(MANIFEST);
        let bytes = std::fs::read(&manifest_path).map_err(|error| Error::NotFound {
            kind: "extraction",
            id: format!("{} ({error})", manifest_path.display()),
        })?;
        let mut output: ExtractionOutput =
            serde_json::from_slice(&bytes).map_err(Error::serialization)?;
        if output.mode.wants_document() {
            let path = output
                .artefacts
                .structured_json
                .clone()
                .map(|path| resolve(run_dir, &path))
                .unwrap_or_else(|| run_dir.join(STRUCTURED));
            let bytes = std::fs::read(&path).map_err(|error| Error::NotFound {
                kind: "structured document",
                id: format!("{} ({error})", path.display()),
            })?;
            let document: StructuredDocument =
                serde_json::from_slice(&bytes).map_err(Error::serialization)?;
            output.document = Some(document);
        }
        if output.mode.wants_text() {
            let path = output
                .artefacts
                .text
                .clone()
                .map(|path| resolve(run_dir, &path))
                .unwrap_or_else(|| run_dir.join(TEXT_MARKDOWN));
            let content = std::fs::read_to_string(&path).map_err(|error| Error::NotFound {
                kind: "extraction text",
                id: format!("{} ({error})", path.display()),
            })?;
            output.text = Some(TextOutput::new(content, output.provenance.text_format));
        }
        output.provenance.cached = true;
        Ok(output)
    }

    /// Returns a cached run matching the request parameters, if one exists.
    ///
    /// # Errors
    ///
    /// Propagates IO errors from listing the document directory; individual
    /// unreadable manifests are treated as cache misses.
    pub fn find_cached(
        &self,
        document_id: &str,
        mode: ExtractionMode,
        format: TextFormat,
        engine: Option<(&str, &str)>,
    ) -> Result<Option<ExtractionOutput>> {
        let dir = self.document_dir(document_id)?;
        let entries = match std::fs::read_dir(&dir) {
            Ok(entries) => entries,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(error) => {
                return Err(Error::Io {
                    detail: format!("could not list {}: {error}", dir.display()),
                })
            }
        };
        let mut runs: Vec<PathBuf> = entries
            .filter_map(|entry| entry.ok())
            .filter(|entry| entry.path().is_dir())
            .map(|entry| entry.path())
            .collect();
        runs.sort();
        for run in runs {
            let manifest = match std::fs::read(run.join(MANIFEST)) {
                Ok(bytes) => bytes,
                Err(_) => continue,
            };
            let cached: ExtractionOutput = match serde_json::from_slice(&manifest) {
                Ok(output) => output,
                Err(_) => continue,
            };
            if cached.mode != mode || cached.provenance.text_format != format {
                continue;
            }
            if let Some((name, version)) = engine {
                if cached.provenance.engine != name || cached.provenance.engine_version != version {
                    continue;
                }
            }
            if let Ok(output) = self.load(&run) {
                if output.validate().is_ok() {
                    return Ok(Some(output));
                }
            }
        }
        Ok(None)
    }

    /// Loads the most recently extracted run for a document.
    ///
    /// # Errors
    ///
    /// Returns [`Error::NotFound`] when the document has no stored runs.
    pub fn latest(&self, document_id: &str) -> Result<ExtractionOutput> {
        let dir = self.document_dir(document_id)?;
        let entries = std::fs::read_dir(&dir).map_err(|error| Error::NotFound {
            kind: "extraction",
            id: format!("{document_id} ({error})"),
        })?;
        let mut newest: Option<(chrono::DateTime<chrono::Utc>, PathBuf)> = None;
        for entry in entries.filter_map(|entry| entry.ok()) {
            let path = entry.path();
            if !path.is_dir() {
                continue;
            }
            let manifest = match std::fs::read(path.join(MANIFEST)) {
                Ok(bytes) => bytes,
                Err(_) => continue,
            };
            let output: ExtractionOutput = match serde_json::from_slice(&manifest) {
                Ok(output) => output,
                Err(_) => continue,
            };
            let newer = newest
                .as_ref()
                .map(|(time, _)| output.provenance.extracted_at >= *time)
                .unwrap_or(true);
            if newer {
                newest = Some((output.provenance.extracted_at, path));
            }
        }
        let (_, path) = newest.ok_or_else(|| Error::NotFound {
            kind: "extraction",
            id: document_id.to_owned(),
        })?;
        self.load(&path)
    }

    fn ensure_within_root(&self, dir: &Path) -> Result<()> {
        let root = std::fs::canonicalize(&self.root).unwrap_or_else(|_| self.root.clone());
        let dir = std::fs::canonicalize(dir).map_err(|error| Error::Io {
            detail: format!("could not resolve {}: {error}", dir.display()),
        })?;
        if !dir.starts_with(&root) {
            return Err(Error::Extraction {
                stage: "store",
                detail: format!(
                    "run directory {} escapes the extraction root {}",
                    dir.display(),
                    root.display()
                ),
            });
        }
        Ok(())
    }
}

fn resolve(run_dir: &Path, path: &Path) -> PathBuf {
    if path.is_absolute() {
        path.to_path_buf()
    } else {
        run_dir.join(path)
    }
}

fn write_json<T: serde::Serialize>(path: &Path, value: &T) -> Result<()> {
    let bytes = serde_json::to_vec_pretty(value).map_err(Error::serialization)?;
    std::fs::write(path, bytes).map_err(|error| Error::Io {
        detail: format!("could not write {}: {error}", path.display()),
    })
}
