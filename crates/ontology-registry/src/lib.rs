//! Semantic registry: catalogs, immutable releases, analysis, validation, and
//! publication over the engine-independent RDF store.

pub mod analysis;
pub mod artifact;
pub mod identity;
pub mod repository;
pub mod service;
pub mod source;

pub use artifact::{ArtifactStore, FilesystemArtifactStore};
pub use identity::{asset_key, content_hash, release_key, sha256_hex, term_key};
pub use repository::RegistryRepository;
pub use service::RegistryService;
