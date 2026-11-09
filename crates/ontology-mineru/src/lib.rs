//! MinerU adapter behind PyO3.
//!
//! The adapter embeds CPython, imports the repository-owned shim
//! (`python/onto_mineru`), and calls it once per extraction. The shim writes
//! artefacts to a temporary directory and returns a small manifest; Rust then
//! parses those artefacts without crossing the FFI boundary again.
//!
//! Building this crate with the `python` feature is the only place in the
//! workspace that links libpython. Without the feature, [`MinerUExtractor`]
//! reports the capability as unsupported with enable instructions, so the
//! default workspace build and test run stay Python-free.

pub mod manifest;

#[cfg(feature = "python")]
mod bridge;

use std::sync::Mutex;

use ontology_core::config::ExtractionConfig;
use ontology_core::document::{
    CacheIdentity, DocumentExtractor, ExtractionContext, ExtractionMode, ExtractionOutput,
};
use ontology_core::error::{Error, Result};

#[cfg(feature = "python")]
use chrono::Utc;
#[cfg(feature = "python")]
use ontology_core::document::{
    AssetRef, ExtractionProvenance, ExtractionWarning, TextFormat, TextOutput,
};
#[cfg(feature = "python")]
use ontology_document::{detect_schema, ids, parse_document, text};
#[cfg(feature = "python")]
use serde_json::Value;
#[cfg(feature = "python")]
use std::path::{Component, Path, PathBuf};
#[cfg(feature = "python")]
use std::time::Instant;

/// A MinerU extractor driven through the Python shim.
pub struct MinerUExtractor {
    config: ExtractionConfig,
    observed: Mutex<Option<CacheIdentity>>,
}

impl MinerUExtractor {
    /// Creates an extractor from the `[extraction]` section.
    ///
    /// # Errors
    ///
    /// Returns [`Error::Config`] when extraction is disabled or invalid.
    pub fn from_config(config: &ExtractionConfig) -> Result<Self> {
        if !config.enabled {
            return Err(Error::Config {
                detail: "extraction.enabled must be true to build a MinerU extractor".to_owned(),
            });
        }
        if config.max_input_bytes == 0 || config.max_concurrency == 0 || config.timeout_seconds == 0
        {
            return Err(Error::Config {
                detail: "extraction limits must be greater than zero".to_owned(),
            });
        }
        Ok(Self {
            config: config.clone(),
            observed: Mutex::new(None),
        })
    }

    /// Creates an extractor with an already-validated configuration.
    #[must_use]
    pub fn new(config: ExtractionConfig) -> Self {
        Self {
            config,
            observed: Mutex::new(None),
        }
    }

    /// Returns the settings in use.
    #[must_use]
    pub fn config(&self) -> &ExtractionConfig {
        &self.config
    }
}

impl DocumentExtractor for MinerUExtractor {
    #[cfg(feature = "python")]
    fn extract(&self, context: &ExtractionContext) -> Result<ExtractionOutput> {
        extract_with_python(self, context)
    }

    #[cfg(not(feature = "python"))]
    fn extract(&self, _context: &ExtractionContext) -> Result<ExtractionOutput> {
        Err(Error::Unsupported {
            operation: "MinerU extraction: rebuild ontology-mineru with `--features python` \
                        and a venv containing MinerU (set PYO3_PYTHON to its interpreter)"
                .to_owned(),
        })
    }

    fn cache_identity(&self) -> Option<CacheIdentity> {
        self.observed
            .lock()
            .ok()
            .and_then(|identity| identity.clone())
    }
}

#[cfg(feature = "python")]
fn extract_with_python(
    extractor: &MinerUExtractor,
    context: &ExtractionContext,
) -> Result<ExtractionOutput> {
    let config = &extractor.config;
    let started = Instant::now();
    let metadata = std::fs::metadata(&context.source).map_err(|error| Error::Invalid {
        detail: format!(
            "source {} is not readable: {error}",
            context.source.display()
        ),
    })?;
    if metadata.len() > config.max_input_bytes {
        return Err(Error::Invalid {
            detail: format!(
                "source is {} bytes, above the {} byte limit",
                metadata.len(),
                config.max_input_bytes
            ),
        });
    }

    let document_hex = ids::hex_suffix(&context.document_id)?;
    let temp = context
        .output_dir
        .join("_tmp")
        .join(document_hex)
        .join(format!(
            "{}-{}",
            std::process::id(),
            chrono::Utc::now().timestamp_nanos_opt().unwrap_or_default()
        ));
    std::fs::create_dir_all(&temp).map_err(|error| Error::Io {
        detail: format!("could not create {}: {error}", temp.display()),
    })?;

    let outcome = (|| -> Result<ExtractionOutput> {
        let options = bridge::BridgeOptions {
            output_dir: temp.clone(),
            shim_path: config.shim_path.clone(),
            venv: config.venv.clone(),
            backend: config.backend.clone(),
            tier: config.tier.clone(),
            model_source: config.model_source.clone(),
            timeout_seconds: config.timeout_seconds,
        };
        let manifest = bridge::run(&context.source, &options)?;
        manifest.validate()?;

        let run_id = ids::run_id(
            &context.document_id,
            "mineru",
            &manifest.mineru_version,
            &manifest.shim_version,
            &manifest.backend,
            &manifest.tier,
            &manifest.schema,
            &manifest.schema_version,
            context.mode,
            context.text_format,
        );
        let final_dir = ids::run_directory(&context.output_dir, &context.document_id, &run_id)?;
        if let Some(parent) = final_dir.parent() {
            std::fs::create_dir_all(parent).map_err(|error| Error::Io {
                detail: format!("could not create {}: {error}", parent.display()),
            })?;
        }
        if final_dir.exists() {
            std::fs::remove_dir_all(&final_dir).map_err(|error| Error::Io {
                detail: format!("could not clear {}: {error}", final_dir.display()),
            })?;
        }
        let middle_rel = manifest
            .middle_json
            .as_deref()
            .map(|path| contained(path, &temp))
            .transpose()?;
        let content_rel = manifest
            .content_list
            .as_deref()
            .map(|path| contained(path, &temp))
            .transpose()?;
        let markdown_rel = manifest
            .markdown
            .as_deref()
            .map(|path| contained(path, &temp))
            .transpose()?;
        let asset_rels = manifest
            .assets
            .iter()
            .map(|asset| contained(&asset.path, &temp).map(|path| (path, asset.mime.clone())))
            .collect::<Result<Vec<_>>>()?;
        std::fs::rename(&temp, &final_dir).map_err(|error| Error::Io {
            detail: format!(
                "could not move {} to {}: {error}",
                temp.display(),
                final_dir.display()
            ),
        })?;

        let middle = middle_rel
            .map(|path| final_dir.join(path))
            .filter(|path| path.exists());
        let content_list = content_rel
            .map(|path| final_dir.join(path))
            .filter(|path| path.exists());
        let markdown = markdown_rel
            .map(|path| final_dir.join(path))
            .filter(|path| path.exists());

        let artefact = middle.as_ref().or(content_list.as_ref()).ok_or_else(|| {
            Error::extraction("manifest", "shim artefact paths do not exist after the run")
        })?;
        let bytes = std::fs::read(artefact).map_err(|error| Error::Io {
            detail: format!("could not read {}: {error}", artefact.display()),
        })?;
        let value: Value = serde_json::from_slice(&bytes).map_err(Error::serialization)?;
        let detected = detect_schema(&value)?;

        let mut warnings: Vec<ExtractionWarning> = manifest
            .warnings
            .iter()
            .map(|warning| ExtractionWarning::new("mineru", warning.clone()))
            .collect();
        let provenance = ExtractionProvenance {
            engine: "mineru".to_owned(),
            engine_version: manifest.mineru_version.clone(),
            shim_version: manifest.shim_version.clone(),
            python_version: manifest.python_version.clone(),
            backend: manifest.backend.clone(),
            tier: manifest.tier.clone(),
            input_schema: detected.kind.name().to_owned(),
            input_schema_version: detected.version.clone(),
            mode: context.mode,
            text_format: context.text_format,
            source_sha256: context.document_id.clone(),
            source_bytes: metadata.len(),
            extracted_at: Utc::now(),
            duration_ms: started.elapsed().as_millis().min(u64::MAX as u128) as u64,
            cached: false,
        };
        let parsed = parse_document(
            &value,
            &detected,
            &ontology_document::parse::ParseInput {
                document_id: &context.document_id,
                source_name: context.source.file_name().and_then(|name| name.to_str()),
                source_bytes: metadata.len(),
                provenance: provenance.clone(),
            },
        )?;
        warnings.extend(parsed.warnings.clone());

        let document = if context.mode.wants_document() {
            Some(parsed.document.clone())
        } else {
            None
        };
        let text_output = if context.mode.wants_text() {
            let markdown_text = match &markdown {
                Some(path) => std::fs::read_to_string(path).map_err(|error| Error::Io {
                    detail: format!("could not read {}: {error}", path.display()),
                })?,
                None => text::render_markdown(&parsed.document),
            };
            let content = match context.text_format {
                TextFormat::Markdown => markdown_text,
                TextFormat::Plain => text::to_plain(&markdown_text),
            };
            Some(TextOutput::new(content, context.text_format))
        } else {
            None
        };

        let assets = asset_rels
            .iter()
            .map(|(relative, mime)| {
                let path = final_dir.join(relative);
                AssetRef {
                    path: relative.to_string_lossy().to_string(),
                    mime: mime.clone(),
                    sha256: std::fs::read(&path)
                        .ok()
                        .map(|bytes| format!("sha256:{}", ids::sha256_hex(&bytes)))
                        .unwrap_or_default(),
                }
            })
            .collect();

        let output = ExtractionOutput {
            document_id: context.document_id.clone(),
            run_id,
            mode: context.mode,
            document,
            text: text_output,
            artefacts: ontology_core::document::ArtefactSet {
                run_dir: final_dir,
                middle_json: middle,
                markdown,
                structured_json: None,
                text: None,
                assets,
            },
            provenance,
            warnings,
        };
        output.validate()?;
        if let Ok(mut observed) = extractor.observed.lock() {
            *observed = Some(CacheIdentity {
                name: "mineru".to_owned(),
                version: manifest.mineru_version.clone(),
            });
        }
        Ok(output)
    })();

    if outcome.is_err() {
        let _ = std::fs::remove_dir_all(&temp);
    }
    outcome
}

#[cfg(feature = "python")]
fn contained(path: &Path, root: &Path) -> Result<PathBuf> {
    let relative = if path.is_absolute() {
        path.strip_prefix(root).map_err(|_| {
            Error::extraction(
                "manifest",
                format!(
                    "reported path {} escapes the run directory {}",
                    path.display(),
                    root.display()
                ),
            )
        })?
    } else {
        path
    };
    if relative.as_os_str().is_empty()
        || relative
            .components()
            .any(|component| !matches!(component, Component::Normal(_)))
    {
        return Err(Error::extraction(
            "manifest",
            format!(
                "reported path {} escapes the run directory {}",
                path.display(),
                root.display()
            ),
        ));
    }
    Ok(relative.to_path_buf())
}

/// Returns whether this build can run MinerU.
#[must_use]
pub const fn python_enabled() -> bool {
    cfg!(feature = "python")
}

/// Returns the extraction modes this adapter supports.
#[must_use]
pub fn supported_modes() -> [ExtractionMode; 3] {
    [
        ExtractionMode::Structured,
        ExtractionMode::Text,
        ExtractionMode::Both,
    ]
}
