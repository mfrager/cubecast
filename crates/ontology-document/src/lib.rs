//! MinerU document extraction without the Python boundary.
//!
//! This crate owns everything that can be tested without CPython:
//!
//! - schema detection and parsing of MinerU artefacts (`middle.json`,
//!   structured content, content lists, legacy middle JSON),
//! - normalisation into the canonical [`ontology_core::document`] model,
//! - the continuous-text projection and Markdown-to-plain conversion,
//! - content-addressed run directories and the extraction cache,
//! - the Document RDF mapping (DoCO/BIBO/Schema.org/PROV-O),
//! - [`DocumentService`], which runs a [`DocumentExtractor`] on a blocking
//!   worker and persists the result.
//!
//! The PyO3 adapter lives in `ontology-mineru`, so this crate never links
//! against libpython.

pub mod artefacts;
pub mod ids;
pub mod normalize;
pub mod parse;
pub mod rdf;
pub mod service;
pub mod text;

#[cfg(test)]
pub(crate) mod tests_support;

pub use artefacts::ArtefactStore;
pub use parse::{detect_schema, parse_document, DetectedSchema, ParsedDocument, SchemaKind};
pub use rdf::{map_document, write_document, DocumentMapping};
pub use service::DocumentService;
