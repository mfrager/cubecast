//! External ontology registry: importing published ontologies, preserving
//! their original IRIs in dedicated graphs, tracking metadata and versions,
//! synchronizing from the recorded source, and recording validated mappings to
//! the internal schema.
//!
//! [`ExternalOntologyManager`] implements
//! [`ontology_core::external::ExternalOntologyRegistry`] over the
//! engine-independent `RdfStore`. Format parsing is delegated to
//! `ontology_rdf::parse_quads`, so only the adapter knows the engine.

pub mod manager;

pub use manager::ExternalOntologyManager;
