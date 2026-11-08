//! Embedding subsystem: pluggable providers, a vector index, and the
//! service that derives attribute and aggregate embeddings from instances.
//!
//! Vectors live in a [`ontology_core::semantic::VectorIndex`] backend, not in
//! Oxigraph; only embedding metadata is stored as RDF. The bundled
//! [`HashingEmbeddingProvider`] is a deterministic local provider, and
//! [`InMemoryVectorIndex`] is an in-process index suitable for a single node
//! or tests. Both are replaceable through their traits.

pub mod index;
pub mod provider;
pub mod service;

pub use index::InMemoryVectorIndex;
pub use provider::HashingEmbeddingProvider;
pub use service::EmbeddingService;
