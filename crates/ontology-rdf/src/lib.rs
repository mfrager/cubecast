//! Oxigraph-backed implementation of the RDF storage layer.
//!
//! [`OxigraphStore`] wraps Oxigraph's RocksDB-backed `Store` and implements the
//! engine-independent [`ontology_core::rdf::RdfStore`] trait. Parsing,
//! serialization, bulk loading, backups, and transactions are exposed here;
//! every Oxigraph type stays inside this crate.

pub mod convert;
pub mod format;
pub mod schema;
pub mod store;
pub mod transaction;

pub use format::{parse_quads, RdfFormat};
pub use store::OxigraphStore;
pub use transaction::OxigraphTransaction;
