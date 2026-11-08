//! OWL schema management: classes, properties, restrictions, hierarchy,
//! validation, and ontology versioning, persisted as RDF.
//!
//! [`OwlManager`] implements [`ontology_core::owl::OntologyManager`] on top of
//! the engine-independent `RdfStore`, so the schema layer works with any
//! storage backend that provides the trait.

mod encoding;
pub mod manager;
mod validate;

pub use manager::OwlManager;
