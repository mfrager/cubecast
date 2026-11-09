//! Source resolution and remote-source policy.

use std::fs;
use std::path::Path;

use ontology_core::config::RegistryConfig;
use ontology_core::error::{Error, Result};
use ontology_core::format::RdfFormat;
use ontology_core::registry::IngestRequest;

/// Resolved source bytes and parsing metadata.
#[derive(Debug, Clone)]
pub struct SourceDocument {
    /// Exact bytes.
    pub bytes: Vec<u8>,
    /// Original source label or URL.
    pub source: String,
    /// RDF syntax.
    pub format: RdfFormat,
    /// Base IRI for relative references.
    pub base: Option<String>,
    /// Original filename, when known.
    pub filename: Option<String>,
}

/// Reads a local or allow-listed HTTP source.
///
/// # Errors
///
/// Returns [`Error::Limit`] when the source exceeds the configured cap,
/// [`Error::Unsupported`] for remote access when disabled, and
/// [`Error::Validation`] for disallowed hosts or unknown formats.
pub async fn resolve(request: &IngestRequest, config: &RegistryConfig) -> Result<SourceDocument> {
    let (bytes, base, filename) = if let Some(data) = &request.data {
        (data.as_bytes().to_vec(), None, Some(request.source.clone()))
    } else if request.source.starts_with("http://") || request.source.starts_with("https://") {
        if !config.allow_remote {
            return Err(Error::Unsupported {
                operation: "remote registry ingestion is disabled".to_owned(),
            });
        }
        let url = reqwest::Url::parse(&request.source).map_err(|error| {
            Error::validation(format!("invalid source URL {}: {error}", request.source))
        })?;
        let host = url
            .host_str()
            .ok_or_else(|| Error::validation("source URL has no host"))?;
        if !config
            .remote_hosts
            .iter()
            .any(|allowed| allowed == "*" || allowed.eq_ignore_ascii_case(host))
        {
            return Err(Error::validation(format!(
                "source host {host:?} is not in registry.remote_hosts"
            )));
        }
        let client = reqwest::Client::builder()
            .redirect(reqwest::redirect::Policy::none())
            .build()
            .map_err(|error| Error::Registry {
                stage: "source",
                detail: error.to_string(),
            })?;
        let response = client
            .get(url.clone())
            .send()
            .await
            .map_err(|error| Error::Registry {
                stage: "source",
                detail: format!("could not fetch {}: {error}", request.source),
            })?;
        if !response.status().is_success() {
            return Err(Error::Registry {
                stage: "source",
                detail: format!("source returned HTTP {}", response.status()),
            });
        }
        if let Some(length) = response.content_length() {
            if length > config.max_source_bytes {
                return Err(Error::Limit {
                    limit: config.max_source_bytes,
                    actual: length,
                });
            }
        }
        let mut response = response;
        let mut bytes = Vec::new();
        while let Some(chunk) = response.chunk().await.map_err(|error| Error::Registry {
            stage: "source",
            detail: error.to_string(),
        })? {
            bytes.extend_from_slice(&chunk);
            if bytes.len() as u64 > config.max_source_bytes {
                return Err(Error::Limit {
                    limit: config.max_source_bytes,
                    actual: bytes.len() as u64,
                });
            }
        }
        let filename = url
            .path_segments()
            .and_then(|mut segments| segments.rfind(|part| !part.is_empty()))
            .map(ToOwned::to_owned);
        (bytes, Some(url.to_string()), filename)
    } else {
        let path = request
            .source
            .strip_prefix("file://")
            .unwrap_or(&request.source);
        let bytes = fs::read(path).map_err(|error| Error::Registry {
            stage: "source",
            detail: format!("cannot read {path}: {error}"),
        })?;
        if bytes.len() as u64 > config.max_source_bytes {
            return Err(Error::Limit {
                limit: config.max_source_bytes,
                actual: bytes.len() as u64,
            });
        }
        (
            bytes,
            None,
            Path::new(path)
                .file_name()
                .and_then(|value| value.to_str())
                .map(ToOwned::to_owned),
        )
    };
    if bytes.len() as u64 > config.max_source_bytes {
        return Err(Error::Limit {
            limit: config.max_source_bytes,
            actual: bytes.len() as u64,
        });
    }
    let format = match &request.format {
        Some(value) => value.parse::<RdfFormat>()?,
        None => filename
            .as_deref()
            .and_then(RdfFormat::from_path)
            .or_else(|| RdfFormat::from_path(&request.source))
            .ok_or_else(|| {
                Error::validation(
                    "cannot infer RDF format; set `format` explicitly (turtle, ntriples, nquads, trig, rdfxml, jsonld)",
                )
            })?,
    };
    Ok(SourceDocument {
        bytes,
        source: request.source.clone(),
        format,
        base,
        filename,
    })
}
