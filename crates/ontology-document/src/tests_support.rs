//! Shared test helpers for the crate's unit tests.

use chrono::{TimeZone, Utc};
use ontology_core::document::{ExtractionMode, ExtractionProvenance, TextFormat};

/// A deterministic provenance block used by model tests.
pub(crate) fn provenance() -> ExtractionProvenance {
    ExtractionProvenance {
        engine: "mineru".to_owned(),
        engine_version: "test".to_owned(),
        shim_version: "1".to_owned(),
        python_version: "3.12".to_owned(),
        backend: "pipeline".to_owned(),
        tier: "default".to_owned(),
        input_schema: "docvortex.middle".to_owned(),
        input_schema_version: "2.0".to_owned(),
        mode: ExtractionMode::Both,
        text_format: TextFormat::Markdown,
        source_sha256: "urn:sha256:abc".to_owned(),
        source_bytes: 1,
        extracted_at: Utc
            .with_ymd_and_hms(2026, 1, 1, 0, 0, 0)
            .single()
            .expect("valid timestamp"),
        duration_ms: 1,
        cached: false,
    }
}
