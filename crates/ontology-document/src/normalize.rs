//! Normalisation: sections, reference lists, citations, and metadata.

use std::collections::BTreeMap;

use serde_json::Value;

use ontology_core::document::{
    BlockKind, Citation, DocumentMetadata, Page, Reference, Section, StructuredDocument,
};
use ontology_core::error::{Error, Result};

/// Builds the logical section tree and assigns every block to a section.
///
/// A heading block opens a section at its own level; blocks before the first
/// heading stay unassigned rather than being forced into an invented section.
pub fn build_sections(pages: &mut [Page], base: &str) -> Vec<Section> {
    let mut sections: Vec<Section> = Vec::new();
    let mut stack: Vec<(u8, usize)> = Vec::new();
    let mut counter = 0_u32;

    for page in pages.iter_mut() {
        for block in &mut page.blocks {
            if let Some(level) = block.kind.heading_level() {
                if !block.text.trim().is_empty() {
                    counter += 1;
                    let id = format!("{base}/section/{counter}");
                    while stack
                        .last()
                        .is_some_and(|(stack_level, _)| *stack_level >= level)
                    {
                        stack.pop();
                    }
                    let parent = stack.last().map(|(_, index)| sections[*index].id.clone());
                    let order = sections.len() as u32;
                    sections.push(Section {
                        id: id.clone(),
                        title: Some(block.text.trim().to_owned()),
                        level,
                        order,
                        parent,
                        blocks: vec![block.id.clone()],
                        page_start: Some(page.index),
                    });
                    stack.push((level, sections.len() - 1));
                    block.section = Some(id);
                    continue;
                }
            }
            if let Some((_, index)) = stack.last() {
                block.section = Some(sections[*index].id.clone());
                sections[*index].blocks.push(block.id.clone());
            }
        }
    }
    sections
}

/// Converts reference-list blocks into [`Reference`] resources.
///
/// Blocks inside a section titled References/Bibliography are reclassified,
/// even when MinerU labelled them as plain paragraphs.
pub fn extract_references(pages: &mut [Page], sections: &[Section], base: &str) -> Vec<Reference> {
    let titles: BTreeMap<&str, &str> = sections
        .iter()
        .filter_map(|section| {
            section
                .title
                .as_deref()
                .map(|title| (section.id.as_str(), title))
        })
        .collect();

    let mut references = Vec::new();
    for page in pages.iter_mut() {
        for block in &mut page.blocks {
            let in_reference_section = block
                .section
                .as_deref()
                .and_then(|id| titles.get(id))
                .is_some_and(|title| is_references_title(title));
            if in_reference_section
                && matches!(
                    block.kind,
                    BlockKind::Paragraph | BlockKind::ListItem | BlockKind::Unknown { .. }
                )
            {
                block.kind = BlockKind::Reference;
            }
            if block.kind != BlockKind::Reference {
                continue;
            }
            let text = block.text.trim();
            if text.is_empty() {
                continue;
            }
            let index = references.len() as u32 + 1;
            references.push(Reference {
                id: format!("{base}/reference/{index}"),
                index,
                raw_text: text.to_owned(),
                resolved: None,
            });
        }
    }
    references
}

/// Finds in-text citation markers in textual blocks.
///
/// Markers such as `[12]`, `[3, 7]`, and `[4-6]` are recorded with the first
/// numeric entry resolved against the extracted reference list. Unresolved
/// markers are kept, not discarded.
pub fn extract_citations(pages: &[Page], references: &[Reference], base: &str) -> Vec<Citation> {
    let by_index: BTreeMap<u32, &Reference> = references
        .iter()
        .map(|reference| (reference.index, reference))
        .collect();
    let mut citations = Vec::new();
    for page in pages {
        for block in &page.blocks {
            if !block.kind.is_textual() {
                continue;
            }
            for marker in citation_markers(&block.text) {
                let index = citations.len() as u32 + 1;
                let first = first_number(&marker);
                citations.push(Citation {
                    id: format!("{base}/citation/{index}"),
                    index,
                    reference: first
                        .and_then(|number| by_index.get(&number))
                        .map(|reference| reference.id.clone()),
                    raw_text: marker,
                    block: Some(block.id.clone()),
                });
            }
        }
    }
    citations
}

/// Derives bibliographic metadata from the artefact and the first page.
#[must_use]
pub fn derive_metadata(
    pages: &[Page],
    source_name: Option<&str>,
    value: &Value,
    source_bytes: u64,
) -> DocumentMetadata {
    let _ = source_bytes;
    let metadata = value.get("metadata");
    let title = metadata
        .and_then(|metadata| metadata.get("title"))
        .and_then(Value::as_str)
        .map(str::to_owned)
        .or_else(|| {
            pages
                .iter()
                .flat_map(|page| &page.blocks)
                .find(|block| block.kind == BlockKind::Title && !block.text.trim().is_empty())
                .map(|block| block.text.trim().to_owned())
        });
    let authors = metadata
        .and_then(|metadata| metadata.get("authors"))
        .and_then(Value::as_array)
        .map(|authors| {
            authors
                .iter()
                .filter_map(|author| {
                    if let Some(name) = author.as_str() {
                        return Some(ontology_core::document::Author {
                            name: name.to_owned(),
                            affiliation: None,
                        });
                    }
                    author.get("name").and_then(Value::as_str).map(|name| {
                        ontology_core::document::Author {
                            name: name.to_owned(),
                            affiliation: author
                                .get("affiliation")
                                .and_then(Value::as_str)
                                .map(str::to_owned),
                        }
                    })
                })
                .collect()
        })
        .unwrap_or_default();
    let abstract_text = metadata
        .and_then(|metadata| metadata.get("abstract"))
        .and_then(Value::as_str)
        .map(str::to_owned)
        .or_else(|| abstract_from_blocks(pages));
    let language = metadata
        .and_then(|metadata| metadata.get("language"))
        .and_then(Value::as_str)
        .map(str::to_owned);
    DocumentMetadata {
        title,
        authors,
        abstract_text,
        language,
        page_count: pages.len() as u32,
        source_file_name: source_name.map(str::to_owned),
    }
}

fn abstract_from_blocks(pages: &[Page]) -> Option<String> {
    let mut in_abstract = false;
    for page in pages {
        for block in &page.blocks {
            if block.kind.heading_level().is_some()
                && block.text.trim().eq_ignore_ascii_case("abstract")
            {
                in_abstract = true;
                continue;
            }
            if in_abstract && block.kind == BlockKind::Paragraph {
                return Some(block.text.trim().to_owned());
            }
            if in_abstract && block.kind.heading_level().is_some() {
                return None;
            }
        }
    }
    None
}

/// Whether a heading names the reference list.
#[must_use]
pub fn is_references_title(title: &str) -> bool {
    let normalized = title.trim().trim_end_matches(':').to_ascii_lowercase();
    matches!(
        normalized.as_str(),
        "references" | "bibliography" | "works cited" | "literature cited"
    )
}

/// Extracts bracketed numeric citation markers, in order.
#[must_use]
pub fn citation_markers(text: &str) -> Vec<String> {
    let mut markers = Vec::new();
    let mut cursor = 0;
    while let Some(start) = text[cursor..].find('[') {
        let start = cursor + start;
        let Some(end) = text[start..].find(']') else {
            break;
        };
        let inner = &text[start + 1..start + end];
        let plausible = inner.chars().any(|c| c.is_ascii_digit())
            && inner.chars().all(|c| {
                c.is_ascii_digit() || matches!(c, ',' | '-' | ' ' | '\u{2013}' | '\u{2014}')
            });
        if plausible {
            markers.push(format!("[{}]", inner.trim()));
        }
        cursor = start + end + 1;
    }
    markers
}

fn first_number(marker: &str) -> Option<u32> {
    let digits: String = marker
        .chars()
        .skip(1)
        .take_while(|c| c.is_ascii_digit())
        .collect();
    digits.parse().ok()
}

/// Checks the invariants that normalisation is expected to establish.
///
/// # Errors
///
/// Propagates the first violated model invariant.
pub fn validate(document: &StructuredDocument) -> Result<()> {
    document.validate().map_err(|error| match error {
        Error::Validation { detail } => Error::Extraction {
            stage: "normalize",
            detail,
        },
        other => other,
    })
}
