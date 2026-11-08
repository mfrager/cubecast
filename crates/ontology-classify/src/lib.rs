//! Classification engine: exact, normalized, token, fuzzy, rule, semantic, and
//! hybrid matching against SKOS concept schemes, with persistent classification
//! records, review states, manual overrides, and explanations.
//!
//! [`MatchingClassifier`] is pure matching logic; [`ClassificationService`]
//! adds instance loading, persistence, and reclassification.

mod classifier;
mod scoring;
pub mod service;

pub use classifier::MatchingClassifier;
pub use service::ClassificationService;
