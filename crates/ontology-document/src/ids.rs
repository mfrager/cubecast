//! Deterministic identifiers and run directory layout.

use std::io::Read;
use std::path::{Path, PathBuf};

use ontology_core::document::{ExtractionMode, TextFormat};
use ontology_core::error::{Error, Result};
use sha2::{Digest, Sha256};

/// Hashes a byte slice and returns lowercase hex.
#[must_use]
pub fn sha256_hex(bytes: &[u8]) -> String {
    hex(&Sha256::digest(bytes))
}

/// Hashes a file without loading it entirely and returns `(hex, size)`.
///
/// # Errors
///
/// Returns [`Error::Io`] when the file cannot be read.
pub fn sha256_file(path: &Path) -> Result<(String, u64)> {
    let mut file = std::fs::File::open(path).map_err(|error| Error::Io {
        detail: format!("could not read {}: {error}", path.display()),
    })?;
    let mut hasher = Sha256::new();
    let mut buffer = [0_u8; 64 * 1024];
    let mut size = 0_u64;
    loop {
        let read = file.read(&mut buffer).map_err(|error| Error::Io {
            detail: format!("could not read {}: {error}", path.display()),
        })?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
        size += read as u64;
    }
    Ok((hex(&hasher.finalize()), size))
}

/// Returns the `urn:sha256:` identifier for a file.
///
/// # Errors
///
/// Propagates read failures from [`sha256_file`].
pub fn document_id(path: &Path) -> Result<String> {
    let (hash, _) = sha256_file(path)?;
    Ok(format!("urn:sha256:{hash}"))
}

/// Extracts the hex suffix from an `urn:` identifier.
///
/// # Errors
///
/// Returns [`Error::Invalid`] when the value is not a `urn:<scheme>:<part>`
/// identifier with a non-empty suffix.
pub fn hex_suffix(value: &str) -> Result<&str> {
    let suffix = value.rsplit(':').next().unwrap_or_default();
    if !value.starts_with("urn:") || suffix.is_empty() || suffix.contains('/') {
        return Err(Error::invalid(format!("{value:?} is not a urn identifier")));
    }
    Ok(suffix)
}

/// Derives the deterministic run identifier.
///
/// The identity covers the source, the engine and its version, the backend and
/// tier, the parsed schema, and the requested projections, so a different
/// configuration never silently reuses a cached run.
#[allow(clippy::too_many_arguments)]
#[must_use]
pub fn run_id(
    document_id: &str,
    engine: &str,
    engine_version: &str,
    shim_version: &str,
    backend: &str,
    tier: &str,
    schema: &str,
    schema_version: &str,
    mode: ExtractionMode,
    format: TextFormat,
) -> String {
    let parts = [
        document_id,
        engine,
        engine_version,
        shim_version,
        backend,
        tier,
        schema,
        schema_version,
        mode.as_str(),
        format.as_str(),
    ];
    let joined = parts.join("\u{1f}");
    format!("urn:onto:extraction:{}", sha256_hex(joined.as_bytes()))
}

/// Returns the run directory for a document and run identifier.
///
/// # Errors
///
/// Returns [`Error::Invalid`] when either identifier is malformed.
pub fn run_directory(root: &Path, document_id: &str, run_id: &str) -> Result<PathBuf> {
    let document = hex_suffix(document_id)?;
    let run = hex_suffix(run_id)?;
    Ok(root.join("sha256").join(document).join(run))
}

fn hex(bytes: &[u8]) -> String {
    use std::fmt::Write as _;
    let mut value = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        let _ = write!(value, "{byte:02x}");
    }
    value
}
