//! Asynchronous extraction orchestration over a blocking extractor.

use std::path::Path;
use std::sync::Arc;

use ontology_core::document::{
    DocumentExtractor, ExtractionContext, ExtractionMode, ExtractionOutput, ExtractionRequest,
};
use ontology_core::error::{Error, Result};
use ontology_core::rdf::RdfStore;
use tokio::sync::Semaphore;

use crate::artefacts::ArtefactStore;
use crate::ids;
use crate::rdf::{self, DocumentMapping};

/// Orchestrates extraction: source validation, cache lookup, blocking worker,
/// persistence, and the optional RDF mapping.
pub struct DocumentService {
    extractor: Arc<dyn DocumentExtractor>,
    config: ontology_core::config::ExtractionConfig,
    store: ArtefactStore,
    semaphore: Arc<Semaphore>,
    rdf: Option<Arc<dyn RdfStore>>,
}

impl DocumentService {
    /// Creates a service around an extractor.
    ///
    /// # Errors
    ///
    /// Returns [`Error::Config`] when a numeric limit is zero.
    pub fn new(
        extractor: Arc<dyn DocumentExtractor>,
        config: ontology_core::config::ExtractionConfig,
    ) -> Result<Self> {
        if config.max_input_bytes == 0 {
            return Err(Error::Config {
                detail: "extraction.max_input_bytes must be greater than zero".to_owned(),
            });
        }
        if config.max_concurrency == 0 {
            return Err(Error::Config {
                detail: "extraction.max_concurrency must be greater than zero".to_owned(),
            });
        }
        if config.timeout_seconds == 0 {
            return Err(Error::Config {
                detail: "extraction.timeout_seconds must be greater than zero".to_owned(),
            });
        }
        let store = ArtefactStore::new(config.output_dir.clone());
        let semaphore = Arc::new(Semaphore::new(config.max_concurrency));
        Ok(Self {
            extractor,
            config,
            store,
            semaphore,
            rdf: None,
        })
    }

    /// Writes the RDF mapping after every successful structured extraction.
    #[must_use]
    pub fn with_rdf(mut self, store: Arc<dyn RdfStore>) -> Self {
        self.rdf = Some(store);
        self
    }

    /// Returns the extraction settings in use.
    #[must_use]
    pub fn config(&self) -> &ontology_core::config::ExtractionConfig {
        &self.config
    }

    /// Returns the artefact store.
    #[must_use]
    pub fn store(&self) -> &ArtefactStore {
        &self.store
    }

    /// Extracts one document, reusing a matching cached run unless `force` is
    /// set.
    ///
    /// # Errors
    ///
    /// Returns [`Error::Invalid`] for an unusable source, propagates extractor
    /// failures, and fails when the result does not match the requested mode.
    pub async fn extract(&self, request: ExtractionRequest) -> Result<ExtractionOutput> {
        let source = &request.source;
        validate_source(source, self.config.max_input_bytes)?;
        let document_id = ids::document_id(source)?;
        let mode = request.mode.unwrap_or(self.config.mode);
        let format = request.text_format.unwrap_or(self.config.text_format);
        let output_dir = request
            .output_dir
            .clone()
            .unwrap_or_else(|| self.config.output_dir.clone());
        let store = ArtefactStore::new(&output_dir);

        if !request.force {
            let identity = self.extractor.cache_identity();
            let engine = identity
                .as_ref()
                .map(|identity| (identity.name.as_str(), identity.version.as_str()));
            if let Some(cached) = store.find_cached(&document_id, mode, format, engine)? {
                // A cached artefact is still authoritative for a store that has
                // just been opened, so the mapping is refreshed idempotently.
                if let (Some(rdf), Some(document)) = (&self.rdf, &cached.document) {
                    rdf::write_document(rdf.as_ref(), document)?;
                }
                return Ok(cached);
            }
        }

        let context = ExtractionContext {
            source: source.clone(),
            output_dir: output_dir.clone(),
            document_id,
            mode,
            text_format: format,
            force: request.force,
        };
        let permit = Arc::clone(&self.semaphore)
            .acquire_owned()
            .await
            .map_err(|_| Error::extraction("worker", "extraction worker pool closed"))?;
        let extractor = Arc::clone(&self.extractor);
        let mut output = tokio::task::spawn_blocking(move || {
            let _permit = permit;
            extractor.extract(&context)
        })
        .await
        .map_err(|error| Error::extraction("worker", error))??;

        store.persist(&mut output)?;
        output.validate()?;
        if let Some(rdf) = &self.rdf {
            if let Some(document) = &output.document {
                rdf::write_document(rdf.as_ref(), document)?;
            }
        }
        Ok(output)
    }

    /// Loads the most recent stored run for a document.
    ///
    /// # Errors
    ///
    /// Returns [`Error::NotFound`] when nothing has been stored.
    pub async fn get(&self, document_id: &str) -> Result<ExtractionOutput> {
        let store = self.store.clone();
        let document_id = document_id.to_owned();
        tokio::task::spawn_blocking(move || store.latest(&document_id))
            .await
            .map_err(|error| Error::extraction("store", error))?
    }

    /// Maps a stored result to RDF.
    ///
    /// # Errors
    ///
    /// Returns [`Error::Unsupported`] for text-only results and propagates
    /// mapping or storage failures.
    pub fn map(&self, output: &ExtractionOutput) -> Result<DocumentMapping> {
        let store = self.rdf.as_ref().ok_or_else(|| Error::Unsupported {
            operation: "document RDF mapping without a store".to_owned(),
        })?;
        let document = output.document.as_ref().ok_or_else(|| Error::Unsupported {
            operation: "document RDF mapping for a text-only extraction".to_owned(),
        })?;
        rdf::write_document(store.as_ref(), document)
    }
}

fn validate_source(path: &Path, max_bytes: u64) -> Result<()> {
    let metadata = std::fs::metadata(path).map_err(|error| Error::Invalid {
        detail: format!("source {} is not readable: {error}", path.display()),
    })?;
    if !metadata.is_file() {
        return Err(Error::Invalid {
            detail: format!("source {} is not a file", path.display()),
        });
    }
    if metadata.len() > max_bytes {
        return Err(Error::Invalid {
            detail: format!(
                "source {} is {} bytes, above the {} byte limit",
                path.display(),
                metadata.len(),
                max_bytes
            ),
        });
    }
    if metadata.len() == 0 {
        return Err(Error::Invalid {
            detail: format!("source {} is empty", path.display()),
        });
    }
    let extension = path
        .extension()
        .and_then(|extension| extension.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase();
    if !matches!(
        extension.as_str(),
        "pdf" | "png" | "jpg" | "jpeg" | "webp" | "tif" | "tiff"
    ) {
        return Err(Error::Invalid {
            detail: format!(
                "source {} has unsupported extension {extension:?}",
                path.display()
            ),
        });
    }
    if extension == "pdf" {
        use std::io::Read as _;
        let mut file = std::fs::File::open(path).map_err(|error| Error::Invalid {
            detail: format!("could not open {}: {error}", path.display()),
        })?;
        let mut magic = [0_u8; 5];
        let read = file.read(&mut magic).map_err(|error| Error::Invalid {
            detail: format!("could not read {}: {error}", path.display()),
        })?;
        if read < 5 || &magic != b"%PDF-" {
            return Err(Error::Invalid {
                detail: format!("source {} is not a PDF", path.display()),
            });
        }
    }
    Ok(())
}

/// The mode a request will execute under, used by interfaces for previews.
#[must_use]
pub fn resolved_mode(
    request: &ExtractionRequest,
    config: &ontology_core::config::ExtractionConfig,
) -> ExtractionMode {
    request.mode.unwrap_or(config.mode)
}
