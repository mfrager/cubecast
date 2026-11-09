//! Domain error type shared by every layer.

use std::path::PathBuf;

/// Convenient result alias for fallible domain operations.
pub type Result<T> = std::result::Result<T, Error>;

/// The single error type returned by the library.
///
/// Variants are intentionally coarse: each interface layer translates them into
/// its own response shape (HTTP status, CLI exit code, MCP error object)
/// without inspecting storage-specific details.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// A supplied identifier is not a valid IRI.
    #[error("invalid IRI {value:?}: {detail}")]
    InvalidIri {
        /// The offending value.
        value: String,
        /// Why it was rejected.
        detail: String,
    },

    /// A prefixed name used a prefix that is not registered.
    #[error("unknown namespace prefix {prefix:?}")]
    UnknownPrefix {
        /// The prefix that could not be resolved.
        prefix: String,
    },

    /// The storage engine could not be opened at the requested location.
    #[error("could not open RDF store at {}: {detail}", path.display())]
    StoreOpen {
        /// Requested database directory.
        path: PathBuf,
        /// Engine-level explanation.
        detail: String,
    },

    /// A storage operation failed.
    #[error("RDF storage error: {detail}")]
    Storage {
        /// Engine-level explanation.
        detail: String,
    },

    /// RDF input could not be parsed.
    #[error("RDF parse error: {detail}")]
    Parse {
        /// Parser explanation.
        detail: String,
    },

    /// RDF output could not be serialized.
    #[error("RDF serialization error: {detail}")]
    Serialization {
        /// Serializer explanation.
        detail: String,
    },

    /// A SPARQL query was invalid or failed during evaluation.
    #[error("SPARQL query error: {detail}")]
    Query {
        /// Evaluator explanation.
        detail: String,
    },

    /// A SPARQL update was invalid or failed during evaluation.
    #[error("SPARQL update error: {detail}")]
    Update {
        /// Evaluator explanation.
        detail: String,
    },

    /// The requested resource does not exist.
    #[error("{kind} {id:?} was not found")]
    NotFound {
        /// Resource kind, e.g. `"instance"`.
        kind: &'static str,
        /// Requested identifier.
        id: String,
    },

    /// The resource already exists and the operation refused to overwrite it.
    #[error("{kind} {id:?} already exists")]
    Conflict {
        /// Resource kind, e.g. `"instance"`.
        kind: &'static str,
        /// Conflicting identifier.
        id: String,
    },

    /// A domain invariant was violated.
    #[error("validation failed: {detail}")]
    Validation {
        /// What failed.
        detail: String,
    },

    /// The request itself was malformed.
    #[error("invalid request: {detail}")]
    Invalid {
        /// What was wrong with the request.
        detail: String,
    },

    /// Configuration could not be read or is inconsistent.
    #[error("configuration error: {detail}")]
    Config {
        /// What failed.
        detail: String,
    },

    /// An I/O operation failed.
    #[error("I/O error: {detail}")]
    Io {
        /// Underlying explanation.
        detail: String,
    },

    /// The backing implementation does not provide this capability.
    #[error("operation {operation:?} is not supported by this implementation")]
    Unsupported {
        /// Name of the unsupported operation.
        operation: String,
    },

    /// Document extraction failed.
    #[error("document extraction failed at {stage}: {detail}")]
    Extraction {
        /// Pipeline stage that failed.
        stage: &'static str,
        /// Explanation, including any Python traceback.
        detail: String,
    },

    /// The Python runtime or MinerU installation is unavailable.
    #[error("Python/MinerU unavailable: {detail}")]
    Python {
        /// What could not be initialised or found.
        detail: String,
    },

    /// The graph registry file or its contents are invalid.
    #[error("graph registry error: {detail}")]
    GraphRegistry {
        /// What failed, including every validation conflict that was found.
        detail: String,
    },

    /// A graph store could not be opened, paired with its registry entry, or
    /// used because it is unavailable.
    #[error("could not open graph store at {}: {detail}", path.display())]
    GraphOpen {
        /// Store directory.
        path: PathBuf,
        /// Explanation, including marker mismatches.
        detail: String,
    },

    /// A write targeted a graph whose store is registered read-only.
    #[error("graph {graph:?} is read-only in store {store:?}")]
    GraphReadOnly {
        /// Graph IRI.
        graph: String,
        /// Owning store identifier.
        store: String,
    },

    /// A cross-store update failed after some groups had already committed.
    #[error(
        "cross-store update partially applied: {reason}; applied {applied:?}, failed {failed:?}"
    )]
    GraphPartial {
        /// Stores whose group committed.
        applied: Vec<String>,
        /// Store whose group failed.
        failed: String,
        /// Underlying failure.
        reason: String,
    },
}

impl Error {
    /// Builds a [`Error::Storage`] from any displayable engine error.
    pub fn storage(detail: impl std::fmt::Display) -> Self {
        Self::Storage {
            detail: detail.to_string(),
        }
    }

    /// Builds a [`Error::Parse`] from any displayable parser error.
    pub fn parse(detail: impl std::fmt::Display) -> Self {
        Self::Parse {
            detail: detail.to_string(),
        }
    }

    /// Builds a [`Error::Serialization`] from any displayable serializer error.
    pub fn serialization(detail: impl std::fmt::Display) -> Self {
        Self::Serialization {
            detail: detail.to_string(),
        }
    }

    /// Builds a [`Error::Query`] from any displayable evaluator error.
    pub fn query(detail: impl std::fmt::Display) -> Self {
        Self::Query {
            detail: detail.to_string(),
        }
    }

    /// Builds a [`Error::Update`] from any displayable evaluator error.
    pub fn update(detail: impl std::fmt::Display) -> Self {
        Self::Update {
            detail: detail.to_string(),
        }
    }

    /// Builds a [`Error::Validation`] with a static-friendly message.
    pub fn validation(detail: impl std::fmt::Display) -> Self {
        Self::Validation {
            detail: detail.to_string(),
        }
    }

    /// Builds a [`Error::Invalid`] with a static-friendly message.
    pub fn invalid(detail: impl std::fmt::Display) -> Self {
        Self::Invalid {
            detail: detail.to_string(),
        }
    }

    /// Builds a [`Error::Extraction`] for a pipeline stage.
    pub fn extraction(stage: &'static str, detail: impl std::fmt::Display) -> Self {
        Self::Extraction {
            stage,
            detail: detail.to_string(),
        }
    }

    /// Builds a [`Error::Python`] from a displayable error.
    pub fn python(detail: impl std::fmt::Display) -> Self {
        Self::Python {
            detail: detail.to_string(),
        }
    }
}

impl From<std::io::Error> for Error {
    fn from(error: std::io::Error) -> Self {
        Self::Io {
            detail: error.to_string(),
        }
    }
}
