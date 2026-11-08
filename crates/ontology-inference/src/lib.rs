//! Inference and enrichment: OWL subclass, subproperty, domain, and range
//! entailment, external mapping application, classification-driven enrichment,
//! and attribute rules.
//!
//! [`RuleInferenceEngine`] always previews before writing and applies results
//! only under an explicit [`ontology_core::semantic::ApplyPolicy`]. Formal
//! entailments and heuristic recommendations are marked separately.

pub mod engine;

pub use engine::RuleInferenceEngine;
