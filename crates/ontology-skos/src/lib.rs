//! SKOS taxonomy management: concept schemes, concepts, collections,
//! hierarchical and associative relations, search, validation, and versioning,
//! persisted as RDF.
//!
//! [`SkosManager`] implements [`ontology_core::skos::TaxonomyManager`] on top of
//! the engine-independent `RdfStore` trait.
//!
//! Storage is canonical: only `skos:broader` edges and one direction of each
//! symmetric relation are written, and inverse readings are derived on load, so
//! a broader/narrower pair can never disagree.

mod encoding;
pub mod manager;

pub use manager::SkosManager;
