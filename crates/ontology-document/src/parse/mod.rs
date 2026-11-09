//! Schema detection and the canonical-model builder.
//!
//! MinerU artefact formats are identified by an explicit discriminator, never
//! by a version number alone (see `design/document_extractor1.md` §2). Unknown
//! schemas fail closed with a list of the formats that were tried.

pub(crate) mod middle;

use serde_json::Value;

use ontology_core::document::{
    BlockKind, DocumentMetadata, ExtractionProvenance, ExtractionWarning, Page, StructuredDocument,
};
use ontology_core::error::{Error, Result};

use crate::normalize;

/// A supported MinerU artefact family.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SchemaKind {
    /// `docvortex.middle` / `pdf_info` middle JSON.
    Middle,
    /// Middle JSON without a schema or version marker.
    LegacyMiddle,
    /// `structured_content.json` (page-grouped blocks).
    StructuredContent,
    /// `content_list.json` V2 (flat items with page indices).
    ContentList,
}

impl SchemaKind {
    /// Stable schema identifier recorded in provenance.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Middle => "docvortex.middle",
            Self::LegacyMiddle => "docvortex.middle.legacy",
            Self::StructuredContent => "docvortex.structured_content",
            Self::ContentList => "docvortex.content_list",
        }
    }
}

/// The detected schema and its version.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DetectedSchema {
    /// Artefact family.
    pub kind: SchemaKind,
    /// Version string, or `"unknown"` for legacy artefacts.
    pub version: String,
}

/// Identifies the artefact family from its content.
///
/// # Errors
///
/// Returns [`Error::Unsupported`] when no supported discriminator is present.
pub fn detect_schema(value: &Value) -> Result<DetectedSchema> {
    if let Some(schema) = value.get("schema").and_then(Value::as_str) {
        let version = value
            .get("schema_version")
            .or_else(|| value.get("schemaVersion"))
            .and_then(Value::as_str)
            .unwrap_or("unknown")
            .to_owned();
        let kind = if schema.contains("middle") {
            SchemaKind::Middle
        } else if schema.contains("structured") {
            SchemaKind::StructuredContent
        } else if schema.contains("content_list") || schema.contains("content-list") {
            SchemaKind::ContentList
        } else {
            return Err(unsupported(schema));
        };
        return Ok(DetectedSchema { kind, version });
    }
    if value.get("pdf_info").is_some() {
        let version = value
            .get("_version_name")
            .or_else(|| value.get("version"))
            .and_then(Value::as_str)
            .map_or_else(|| "legacy".to_owned(), str::to_owned);
        let kind = if version == "legacy" {
            SchemaKind::LegacyMiddle
        } else {
            SchemaKind::Middle
        };
        return Ok(DetectedSchema { kind, version });
    }
    if let Some(pages) = value.get("pages").and_then(Value::as_array) {
        if pages.iter().any(|page| {
            page.get("blocks").is_some()
                || page.get("para_blocks").is_some()
                || page.get("preproc_blocks").is_some()
        }) {
            return Ok(DetectedSchema {
                kind: SchemaKind::StructuredContent,
                version: "1".to_owned(),
            });
        }
    }
    if value.is_array() || value.get("content_list").is_some() {
        return Ok(DetectedSchema {
            kind: SchemaKind::ContentList,
            version: "2".to_owned(),
        });
    }
    Err(Error::Unsupported {
        operation: "document artefact schema (tried docvortex.middle, \
                    docvortex.structured_content, content_list v2, legacy middle)"
            .to_owned(),
    })
}

fn unsupported(schema: &str) -> Error {
    Error::Unsupported {
        operation: format!("document artefact schema {schema:?}"),
    }
}

/// Inputs the builder needs that are not part of the artefact.
pub struct ParseInput<'a> {
    /// `urn:sha256:<hex>` of the source.
    pub document_id: &'a str,
    /// Original file name.
    pub source_name: Option<&'a str>,
    /// Source size in bytes.
    pub source_bytes: u64,
    /// Engine facts, including the schema identifiers.
    pub provenance: ExtractionProvenance,
}

/// A parsed document with its non-fatal findings.
#[derive(Debug, Clone)]
pub struct ParsedDocument {
    /// Canonical model.
    pub document: StructuredDocument,
    /// Non-fatal findings.
    pub warnings: Vec<ExtractionWarning>,
}

/// Builds the canonical model from a parsed MinerU artefact.
///
/// # Errors
///
/// Returns [`Error::Extraction`] for malformed artefacts and
/// [`Error::Validation`] when the resulting model violates an invariant.
pub fn parse_document(
    value: &Value,
    detected: &DetectedSchema,
    input: &ParseInput<'_>,
) -> Result<ParsedDocument> {
    let mut warnings = Vec::new();
    let raw_pages = middle::extract_pages(value, detected.kind, &mut warnings)?;
    if raw_pages.is_empty() {
        return Err(Error::extraction(
            "parse",
            "artefact contains no pages with blocks",
        ));
    }

    let base = format!(
        "urn:onto:document:{}",
        crate::ids::hex_suffix(input.document_id)?
    );
    let mut block_counter = 0_u32;
    let mut pages: Vec<Page> = Vec::with_capacity(raw_pages.len());
    for raw in raw_pages {
        let mut blocks = Vec::with_capacity(raw.blocks.len() + raw.discarded.len());
        for (order, raw_block) in raw.blocks.into_iter().chain(raw.discarded).enumerate() {
            block_counter += 1;
            let mut block = raw_block.into_block(&base, block_counter, order as u32);
            if matches!(block.kind, BlockKind::Unknown { .. }) {
                warnings.push(
                    ExtractionWarning::new(
                        "unknown_block_type",
                        format!("preserved unclassified block {:?}", block.text),
                    )
                    .with_page(raw.index)
                    .with_block(block.id.clone()),
                );
            }
            if block.kind == BlockKind::Figure && block.asset.is_none() && block.caption.is_none() {
                warnings.push(
                    ExtractionWarning::new(
                        "uncaptioned_figure",
                        "figure has neither caption nor asset",
                    )
                    .with_page(raw.index)
                    .with_block(block.id.clone()),
                );
            }
            if block.kind == BlockKind::Table
                && block
                    .table
                    .as_ref()
                    .map(|table| table.rows.is_empty() && table.html.is_none())
                    .unwrap_or(true)
            {
                warnings.push(
                    ExtractionWarning::new("empty_table", "table has no rows or HTML")
                        .with_page(raw.index)
                        .with_block(block.id.clone()),
                );
            }
            block.reading_order = order as u32;
            blocks.push(block);
        }
        pages.push(Page {
            index: raw.index,
            width: raw.width,
            height: raw.height,
            unit: raw.unit,
            blocks,
        });
    }
    pages.sort_by_key(|page| page.index);

    let sections = normalize::build_sections(&mut pages, &base);
    let references = normalize::extract_references(&mut pages, &sections, &base);
    let citations = normalize::extract_citations(&pages, &references, &base);
    let metadata = normalize::derive_metadata(&pages, input.source_name, value, input.source_bytes);

    let document = StructuredDocument {
        document_id: input.document_id.to_owned(),
        metadata,
        pages,
        sections,
        references,
        citations,
        provenance: input.provenance.clone(),
    };
    document.validate()?;
    let _ = detected;
    Ok(ParsedDocument { document, warnings })
}

/// Number of textual blocks, used by tests and diagnostics.
#[must_use]
pub fn textual_blocks(document: &StructuredDocument) -> usize {
    document
        .pages
        .iter()
        .flat_map(|page| &page.blocks)
        .filter(|block| block.kind.is_textual())
        .count()
}

/// Convenience for metadata-only documents.
#[must_use]
pub fn empty_metadata() -> DocumentMetadata {
    DocumentMetadata::default()
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    fn input(document_id: &str) -> ParseInput<'_> {
        ParseInput {
            document_id,
            source_name: Some("fixture.pdf"),
            source_bytes: 1,
            provenance: crate::tests_support::provenance(),
        }
    }

    #[test]
    fn unknown_schema_fails_closed() {
        let error = detect_schema(&json!({ "something": "else" })).expect_err("unsupported");
        assert!(matches!(error, Error::Unsupported { .. }));
    }

    #[test]
    fn content_lists_are_grouped_by_page() {
        let document_id = format!("urn:sha256:{}", "c".repeat(64));
        let value = json!([
            { "type": "text", "text": "Page zero body.", "page_idx": 0 },
            { "type": "text", "text": "Heading", "text_level": 1, "page_idx": 0 },
            { "type": "text", "text": "Page one body.", "page_idx": 1 }
        ]);
        let detected = detect_schema(&value).expect("content list");
        assert_eq!(detected.kind, SchemaKind::ContentList);
        let parsed = parse_document(&value, &detected, &input(&document_id)).expect("parse");
        assert_eq!(parsed.document.pages.len(), 2);
        assert_eq!(parsed.document.pages[0].blocks.len(), 2);
        assert_eq!(parsed.document.pages[1].index, 1);
        assert!(parsed
            .document
            .sections
            .iter()
            .any(|section| { section.title.as_deref() == Some("Heading") }));
    }

    #[test]
    fn unclassified_blocks_are_preserved_with_a_warning() {
        use ontology_core::document::BlockKind;

        let document_id = format!("urn:sha256:{}", "e".repeat(64));
        let value = json!({
            "pdf_info": [{
                "page_idx": 0,
                "para_blocks": [{
                    "type": "mystery",
                    "content": "Kept text",
                    "bbox": [0, 0, 10, 10]
                }]
            }]
        });
        let detected = detect_schema(&value).expect("middle");
        let parsed = parse_document(&value, &detected, &input(&document_id)).expect("parse");
        let block = &parsed.document.pages[0].blocks[0];
        assert!(matches!(block.kind, BlockKind::Unknown { .. }));
        assert_eq!(block.text, "Kept text");
        assert!(parsed
            .warnings
            .iter()
            .any(|warning| warning.code == "unknown_block_type"));
    }

    #[test]
    fn heading_levels_drive_section_nesting() {
        let document_id = format!("urn:sha256:{}", "d".repeat(64));
        let value = json!({
            "pdf_info": [{
                "page_idx": 0,
                "para_blocks": [
                    { "type": "h1", "content": "Top" },
                    { "type": "text", "content": "Body." },
                    { "type": "h2", "content": "Child" },
                    { "type": "text", "content": "More." }
                ]
            }]
        });
        let detected = detect_schema(&value).expect("middle");
        let parsed = parse_document(&value, &detected, &input(&document_id)).expect("parse");
        let sections = &parsed.document.sections;
        assert_eq!(sections.len(), 2);
        assert!(sections[1].parent.as_deref() == Some(sections[0].id.as_str()));
        assert_eq!(sections[1].level, 2);
    }
}
