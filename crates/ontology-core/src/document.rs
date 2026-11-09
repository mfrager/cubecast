//! Document extraction model, shared by every layer.
//!
//! These types describe what a caller asks for (`ExtractionMode`) and what an
//! extractor returns. They are deliberately engine-neutral: no PyO3 or MinerU
//! type appears here, so interfaces and the domain stay independent of the
//! Python boundary.
//!
//! Serialization follows the crate convention: snake_case fields and tagged
//! enums.

use std::path::PathBuf;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::error::{Error, Result};

/// What a caller wants back from one extraction.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ExtractionMode {
    /// Page/block/line hierarchy only.
    Structured,
    /// One continuous text block only.
    Text,
    /// Both projections, derived from a single parse.
    #[default]
    Both,
}

impl ExtractionMode {
    /// Stable name used in configuration and provenance.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Structured => "structured",
            Self::Text => "text",
            Self::Both => "both",
        }
    }

    /// Whether the mode requires a structured document.
    #[must_use]
    pub const fn wants_document(self) -> bool {
        matches!(self, Self::Structured | Self::Both)
    }

    /// Whether the mode requires a single text block.
    #[must_use]
    pub const fn wants_text(self) -> bool {
        matches!(self, Self::Text | Self::Both)
    }
}

/// Format of [`TextOutput::content`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TextFormat {
    /// MinerU NLP Markdown: headings and paragraphs retained, figures and
    /// tables omitted.
    #[default]
    Markdown,
    /// Markdown formatting stripped, plain UTF-8 text.
    Plain,
}

impl TextFormat {
    /// Stable name used in provenance.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Markdown => "markdown",
            Self::Plain => "plain",
        }
    }
}

/// One extraction request, fully describing the work so a failed job can be
/// retried without the caller.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ExtractionRequest {
    /// PDF (or MinerU-supported image) to extract.
    pub source: PathBuf,
    /// Overrides `[extraction].mode`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mode: Option<ExtractionMode>,
    /// Overrides `[extraction].text_format`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub text_format: Option<TextFormat>,
    /// Overrides `[extraction].output_dir`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub output_dir: Option<PathBuf>,
    /// Re-run even when a matching cached run exists.
    #[serde(default)]
    pub force: bool,
}

impl ExtractionRequest {
    /// Creates a request using configuration defaults for mode and format.
    #[must_use]
    pub fn new(source: impl Into<PathBuf>) -> Self {
        Self {
            source: source.into(),
            mode: None,
            text_format: None,
            output_dir: None,
            force: false,
        }
    }

    /// Sets the extraction mode.
    #[must_use]
    pub fn with_mode(mut self, mode: ExtractionMode) -> Self {
        self.mode = Some(mode);
        self
    }

    /// Sets the text format.
    #[must_use]
    pub fn with_text_format(mut self, format: TextFormat) -> Self {
        self.text_format = Some(format);
        self
    }

    /// Sets the output directory.
    #[must_use]
    pub fn with_output_dir(mut self, dir: impl Into<PathBuf>) -> Self {
        self.output_dir = Some(dir.into());
        self
    }
}

/// A request with every default resolved and the source already identified.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ExtractionContext {
    /// Source path.
    pub source: PathBuf,
    /// Resolved output directory.
    pub output_dir: PathBuf,
    /// `urn:sha256:<hex>` of the source bytes.
    pub document_id: String,
    /// Resolved mode.
    pub mode: ExtractionMode,
    /// Resolved text format.
    pub text_format: TextFormat,
    /// Whether a matching cached run must be ignored.
    pub force: bool,
}

/// One continuous text block.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TextOutput {
    /// The text itself.
    pub content: String,
    /// How the text was rendered.
    pub format: TextFormat,
    /// Number of characters in `content`.
    pub char_count: usize,
}

impl TextOutput {
    /// Wraps text, recording its format and length.
    #[must_use]
    pub fn new(content: impl Into<String>, format: TextFormat) -> Self {
        let content = content.into();
        Self {
            char_count: content.chars().count(),
            content,
            format,
        }
    }
}

/// Result of one extraction run.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ExtractionOutput {
    /// `urn:sha256:<hex>` of the source bytes.
    pub document_id: String,
    /// `urn:onto:extraction:<hex>` of this run.
    pub run_id: String,
    /// Mode that was actually executed.
    pub mode: ExtractionMode,
    /// Present for `structured` and `both`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub document: Option<StructuredDocument>,
    /// Present for `text` and `both`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub text: Option<TextOutput>,
    /// Files written for this run.
    pub artefacts: ArtefactSet,
    /// Engine, versions, schema identifiers, and timing.
    pub provenance: ExtractionProvenance,
    /// Non-fatal findings.
    #[serde(default)]
    pub warnings: Vec<ExtractionWarning>,
}

impl ExtractionOutput {
    /// Checks that the result matches the requested mode exactly.
    ///
    /// # Errors
    ///
    /// Returns [`Error::Extraction`] when a required projection is missing, an
    /// unrequested projection is present, or provenance disagrees with the
    /// result.
    pub fn validate(&self) -> Result<()> {
        match self.mode {
            ExtractionMode::Structured => {
                if self.document.is_none() {
                    return Err(Error::extraction(
                        "mode",
                        "structured mode produced no document",
                    ));
                }
                if self.text.is_some() {
                    return Err(Error::extraction(
                        "mode",
                        "structured mode must not include a text block",
                    ));
                }
            }
            ExtractionMode::Text => {
                if self.text.is_none() {
                    return Err(Error::extraction(
                        "mode",
                        "text mode produced no text block",
                    ));
                }
                if self.document.is_some() {
                    return Err(Error::extraction(
                        "mode",
                        "text mode must not include a structured document",
                    ));
                }
            }
            ExtractionMode::Both => {
                if self.document.is_none() || self.text.is_none() {
                    return Err(Error::extraction(
                        "mode",
                        "both mode requires a document and a text block",
                    ));
                }
            }
        }
        if let Some(document) = &self.document {
            document.validate()?;
            if document.document_id != self.document_id {
                return Err(Error::extraction(
                    "mode",
                    "document id differs from the extraction result",
                ));
            }
        }
        if let Some(text) = &self.text {
            if text.format != self.provenance.text_format {
                return Err(Error::extraction(
                    "mode",
                    "text format differs from provenance",
                ));
            }
        }
        if self.provenance.mode != self.mode {
            return Err(Error::extraction(
                "mode",
                "provenance mode differs from the result mode",
            ));
        }
        Ok(())
    }
}

/// Files written for one run.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ArtefactSet {
    /// Directory holding every file for the run.
    pub run_dir: PathBuf,
    /// Raw MinerU `middle.json`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub middle_json: Option<PathBuf>,
    /// NLP Markdown rendered by MinerU.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub markdown: Option<PathBuf>,
    /// Canonical structured document.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub structured_json: Option<PathBuf>,
    /// Stored text block.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub text: Option<PathBuf>,
    /// Extracted images.
    #[serde(default)]
    pub assets: Vec<AssetRef>,
}

/// A reference to an extracted binary asset.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AssetRef {
    /// Path relative to the run directory.
    pub path: String,
    /// MIME type when known.
    #[serde(default)]
    pub mime: String,
    /// `sha256:<hex>` of the file.
    #[serde(default)]
    pub sha256: String,
}

/// Engine, version, and configuration facts for one run.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ExtractionProvenance {
    /// Extractor name, e.g. `"mineru"`.
    pub engine: String,
    /// Engine version.
    pub engine_version: String,
    /// Bridge/shim version.
    pub shim_version: String,
    /// CPython version used by the bridge.
    #[serde(default)]
    pub python_version: String,
    /// MinerU backend (`pipeline`, `vlm`, …).
    pub backend: String,
    /// MinerU model tier.
    pub tier: String,
    /// Parsed input schema identifier.
    pub input_schema: String,
    /// Parsed input schema version.
    pub input_schema_version: String,
    /// Mode that was executed.
    pub mode: ExtractionMode,
    /// Requested text format.
    pub text_format: TextFormat,
    /// `urn:sha256:<hex>` of the source.
    pub source_sha256: String,
    /// Source size in bytes.
    pub source_bytes: u64,
    /// When the run finished.
    pub extracted_at: DateTime<Utc>,
    /// Wall-clock duration.
    pub duration_ms: u64,
    /// Whether the result came from the cache.
    #[serde(default)]
    pub cached: bool,
}

/// A non-fatal extraction finding.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExtractionWarning {
    /// Stable warning code.
    pub code: String,
    /// Human-readable message.
    pub message: String,
    /// Page the finding belongs to.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub page: Option<u32>,
    /// Block the finding belongs to.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub block: Option<String>,
}

impl ExtractionWarning {
    /// Creates a warning.
    #[must_use]
    pub fn new(code: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            code: code.into(),
            message: message.into(),
            page: None,
            block: None,
        }
    }

    /// Attaches a page index.
    #[must_use]
    pub fn with_page(mut self, page: u32) -> Self {
        self.page = Some(page);
        self
    }

    /// Attaches a block identifier.
    #[must_use]
    pub fn with_block(mut self, block: impl Into<String>) -> Self {
        self.block = Some(block.into());
        self
    }
}

/// The canonical document model.
///
/// Physical structure (`pages`) and logical structure (`sections`) coexist;
/// sections reference blocks by id instead of owning copies.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct StructuredDocument {
    /// Same id as [`ExtractionOutput::document_id`].
    pub document_id: String,
    /// Bibliographic metadata found in the document.
    pub metadata: DocumentMetadata,
    /// Pages in physical order.
    pub pages: Vec<Page>,
    /// Logical sections.
    #[serde(default)]
    pub sections: Vec<Section>,
    /// Bibliographic references.
    #[serde(default)]
    pub references: Vec<Reference>,
    /// In-text citation occurrences.
    #[serde(default)]
    pub citations: Vec<Citation>,
    /// Engine facts.
    pub provenance: ExtractionProvenance,
}

impl StructuredDocument {
    /// Checks the model invariants that the RDF mapping and text projections
    /// rely on.
    ///
    /// # Errors
    ///
    /// Returns [`Error::Validation`] with the first violated invariant.
    pub fn validate(&self) -> Result<()> {
        let mut seen_pages = std::collections::BTreeSet::new();
        let mut block_ids = std::collections::BTreeSet::new();
        let mut section_ids = std::collections::BTreeSet::new();

        for page in &self.pages {
            if !seen_pages.insert(page.index) {
                return Err(Error::validation(format!(
                    "duplicate page index {}",
                    page.index
                )));
            }
            let mut orders = std::collections::BTreeSet::new();
            for block in &page.blocks {
                if !orders.insert(block.reading_order) {
                    return Err(Error::validation(format!(
                        "duplicate reading order {} on page {}",
                        block.reading_order, page.index
                    )));
                }
                if !block_ids.insert(block.id.clone()) {
                    return Err(Error::validation(format!(
                        "duplicate block id {}",
                        block.id
                    )));
                }
                if let Some(geometry) = &block.geometry {
                    geometry.validate()?;
                }
                if let Some(table) = &block.table {
                    table.validate()?;
                }
            }
        }

        for section in &self.sections {
            if !section_ids.insert(section.id.clone()) {
                return Err(Error::validation(format!(
                    "duplicate section id {}",
                    section.id
                )));
            }
            for block in &section.blocks {
                if !block_ids.contains(block) {
                    return Err(Error::validation(format!(
                        "section {} references unknown block {}",
                        section.id, block
                    )));
                }
            }
            if let Some(parent) = &section.parent {
                if !section_ids.contains(parent) && !self.sections.iter().any(|s| &s.id == parent) {
                    return Err(Error::validation(format!(
                        "section {} references unknown parent {}",
                        section.id, parent
                    )));
                }
            }
        }

        let mut reference_ids = std::collections::BTreeSet::new();
        for reference in &self.references {
            if !reference_ids.insert(reference.id.clone()) {
                return Err(Error::validation(format!(
                    "duplicate reference id {}",
                    reference.id
                )));
            }
        }
        let mut citation_ids = std::collections::BTreeSet::new();
        for citation in &self.citations {
            if !citation_ids.insert(citation.id.clone()) {
                return Err(Error::validation(format!(
                    "duplicate citation id {}",
                    citation.id
                )));
            }
            if let Some(reference) = &citation.reference {
                if !reference_ids.contains(reference) {
                    return Err(Error::validation(format!(
                        "citation {} references unknown reference {}",
                        citation.id, reference
                    )));
                }
            }
        }
        Ok(())
    }
}

/// Bibliographic metadata.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct DocumentMetadata {
    /// Document title.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    /// Authors in document order.
    #[serde(default)]
    pub authors: Vec<Author>,
    /// Abstract, when the document contains one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub abstract_text: Option<String>,
    /// Language tag when known.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub language: Option<String>,
    /// Number of pages.
    #[serde(default)]
    pub page_count: u32,
    /// Original file name.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_file_name: Option<String>,
}

/// A document author.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Author {
    /// Display name.
    pub name: String,
    /// Affiliation, when present.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub affiliation: Option<String>,
}

/// One physical page.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Page {
    /// Zero-based page index.
    pub index: u32,
    /// Page width in `unit`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub width: Option<f64>,
    /// Page height in `unit`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub height: Option<f64>,
    /// Coordinate unit, e.g. `"pt"`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub unit: Option<String>,
    /// Blocks in physical reading order.
    #[serde(default)]
    pub blocks: Vec<Block>,
}

/// The kind of a content block.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum BlockKind {
    /// Document title.
    Title,
    /// Section heading with its nesting level.
    Heading {
        /// Heading level, starting at 1.
        level: u8,
    },
    /// Body paragraph.
    Paragraph,
    /// A list container.
    List,
    /// One list item.
    ListItem,
    /// Figure or image.
    Figure,
    /// Table.
    Table,
    /// Display or inline formula.
    Formula,
    /// Footnote.
    Footnote,
    /// Running page header.
    PageHeader,
    /// Running page footer.
    PageFooter,
    /// Bibliographic reference entry.
    Reference,
    /// Code or preformatted block.
    Code,
    /// A block type this normaliser does not classify yet.
    Unknown {
        /// Original MinerU label.
        label: String,
    },
}

impl BlockKind {
    /// Whether the block contributes to the continuous text.
    #[must_use]
    pub const fn is_textual(&self) -> bool {
        matches!(
            self,
            Self::Title | Self::Heading { .. } | Self::Paragraph | Self::ListItem | Self::Footnote
        )
    }

    /// Section level implied by this block, when it starts one.
    #[must_use]
    pub const fn heading_level(&self) -> Option<u8> {
        match self {
            Self::Title => Some(1),
            Self::Heading { level } => Some(*level),
            _ => None,
        }
    }
}

/// A content block.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Block {
    /// Stable IRI.
    pub id: String,
    /// What kind of content this is.
    pub kind: BlockKind,
    /// Normalised text.
    #[serde(default)]
    pub text: String,
    /// Bounding box on the page.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub geometry: Option<Geometry>,
    /// Position within the page.
    #[serde(default)]
    pub reading_order: u32,
    /// Lines, preserving span structure.
    #[serde(default)]
    pub lines: Vec<Line>,
    /// Logical section membership.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub section: Option<String>,
    /// Figure/table caption.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub caption: Option<Caption>,
    /// Image reference for a figure.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub asset: Option<AssetRef>,
    /// Table payload.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub table: Option<Table>,
    /// Formula payload.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub formula: Option<Formula>,
    /// List payload.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub list: Option<ListBlock>,
}

impl Block {
    /// Creates a block with the given kind and text.
    #[must_use]
    pub fn new(id: impl Into<String>, kind: BlockKind, text: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            kind,
            text: text.into(),
            geometry: None,
            reading_order: 0,
            lines: Vec::new(),
            section: None,
            caption: None,
            asset: None,
            table: None,
            formula: None,
            list: None,
        }
    }
}

/// A text line.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Line {
    /// Line text.
    #[serde(default)]
    pub text: String,
    /// Line bounding box.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub geometry: Option<Geometry>,
    /// Inline spans.
    #[serde(default)]
    pub spans: Vec<Span>,
}

/// An inline span.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Span {
    /// Span text.
    #[serde(default)]
    pub text: String,
    /// Span bounding box.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub geometry: Option<Geometry>,
    /// Style hint from the engine.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub style: Option<String>,
}

/// A rectangular region on a page.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Geometry {
    /// Left edge.
    pub x0: f64,
    /// Top edge.
    pub y0: f64,
    /// Right edge.
    pub x1: f64,
    /// Bottom edge.
    pub y1: f64,
    /// Coordinate unit.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub unit: Option<String>,
    /// Coordinate origin.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub origin: Option<String>,
}

impl Geometry {
    /// Checks that the rectangle is not inverted.
    ///
    /// # Errors
    ///
    /// Returns [`Error::Validation`] when an edge is inverted or non-finite.
    pub fn validate(&self) -> Result<()> {
        if ![self.x0, self.y0, self.x1, self.y1]
            .iter()
            .all(|value| value.is_finite())
        {
            return Err(Error::validation("geometry coordinates must be finite"));
        }
        if self.x0 > self.x1 || self.y0 > self.y1 {
            return Err(Error::validation(format!(
                "inverted geometry [{}, {}, {}, {}]",
                self.x0, self.y0, self.x1, self.y1
            )));
        }
        Ok(())
    }
}

/// A logical section.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Section {
    /// Stable IRI.
    pub id: String,
    /// Section heading.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    /// Nesting level, starting at 1.
    pub level: u8,
    /// Position among sibling sections.
    pub order: u32,
    /// Parent section IRI.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parent: Option<String>,
    /// Blocks belonging to this section.
    #[serde(default)]
    pub blocks: Vec<String>,
    /// First page the section appears on.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub page_start: Option<u32>,
}

/// A figure or table caption.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Caption {
    /// Caption text.
    pub text: String,
}

/// A table.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Table {
    /// HTML representation when the engine provides one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub html: Option<String>,
    /// Markdown representation when the engine provides one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub markdown: Option<String>,
    /// Reconstructed rows.
    #[serde(default)]
    pub rows: Vec<TableRow>,
}

impl Table {
    /// Checks that cell coordinates are unique within the table.
    ///
    /// # Errors
    ///
    /// Returns [`Error::Validation`] for a duplicate `(row, column)` cell.
    pub fn validate(&self) -> Result<()> {
        let mut cells = std::collections::BTreeSet::new();
        for row in &self.rows {
            for cell in &row.cells {
                if !cells.insert((cell.row, cell.column)) {
                    return Err(Error::validation(format!(
                        "duplicate table cell ({}, {})",
                        cell.row, cell.column
                    )));
                }
            }
        }
        Ok(())
    }
}

/// One table row.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct TableRow {
    /// Zero-based row index.
    pub index: u32,
    /// Cells in column order.
    #[serde(default)]
    pub cells: Vec<TableCell>,
}

/// One table cell.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TableCell {
    /// Zero-based row index.
    pub row: u32,
    /// Zero-based column index.
    pub column: u32,
    /// Row span.
    #[serde(default = "one")]
    pub row_span: u32,
    /// Column span.
    #[serde(default = "one")]
    pub column_span: u32,
    /// Whether this cell is a header.
    #[serde(default)]
    pub is_header: bool,
    /// Cell text.
    #[serde(default)]
    pub text: String,
}

const fn one() -> u32 {
    1
}

/// A formula.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Formula {
    /// LaTeX source.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub latex: Option<String>,
    /// MathML source.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mathml: Option<String>,
    /// Inline or display formula.
    #[serde(default)]
    pub formula_type: FormulaType,
    /// Rendered image fallback.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub asset: Option<AssetRef>,
}

/// Where a formula appears.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FormulaType {
    /// Display formula.
    #[default]
    Interline,
    /// Inline formula.
    Inline,
}

/// A list block.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ListBlock {
    /// Whether the list is ordered.
    #[serde(default)]
    pub ordered: bool,
    /// Item texts.
    #[serde(default)]
    pub items: Vec<String>,
}

/// A bibliographic reference.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Reference {
    /// Stable IRI.
    pub id: String,
    /// One-based reference index within the document.
    pub index: u32,
    /// Raw reference text, exactly as extracted.
    pub raw_text: String,
    /// Parsed publication details, when available.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub resolved: Option<ResolvedWork>,
}

/// Publication details parsed from a reference.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ResolvedWork {
    /// Title.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    /// Authors.
    #[serde(default)]
    pub authors: Vec<String>,
    /// Publication year.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub year: Option<u32>,
    /// DOI.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub doi: Option<String>,
    /// URL.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
}

/// An in-text citation occurrence.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Citation {
    /// Stable IRI.
    pub id: String,
    /// One-based occurrence index.
    pub index: u32,
    /// The citation marker as it appears, e.g. `"[12]"`.
    pub raw_text: String,
    /// Referenced entry IRI, when resolved.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reference: Option<String>,
    /// Block the citation appears in.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub block: Option<String>,
}

/// Engine identity used to match cached runs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CacheIdentity {
    /// Engine name, e.g. `"mineru"`.
    pub name: String,
    /// Engine version, when known without running an extraction.
    pub version: String,
}

/// A capability implemented by document extraction backends.
///
/// Implementations are synchronous and may be long-running; callers execute
/// them in a blocking worker (see `DocumentService`).
pub trait DocumentExtractor: Send + Sync {
    /// Runs one extraction for an already-resolved context.
    ///
    /// # Errors
    ///
    /// Returns [`Error::Extraction`] or [`Error::Python`] when extraction fails.
    fn extract(&self, context: &ExtractionContext) -> Result<ExtractionOutput>;

    /// Identifies the engine for cache matching.
    ///
    /// Returning `None` disables the version check, which is correct for
    /// adapters that only learn their version by running once.
    fn cache_identity(&self) -> Option<CacheIdentity> {
        None
    }
}
