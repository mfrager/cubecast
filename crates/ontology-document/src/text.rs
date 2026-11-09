//! The continuous-text projection.
//!
//! MinerU's NLP Markdown is authoritative when present (see
//! `design/document_extractor2.md`). [`render_markdown`] is the deterministic
//! fallback used with fixture artefacts and with engines that do not emit a
//! Markdown rendering.

use ontology_core::document::{BlockKind, StructuredDocument};

/// Renders one continuous Markdown block from the canonical model.
///
/// Headings and paragraphs are kept, figures/tables/formulas and running
/// headers/footers are skipped, and pages are concatenated in reading order.
#[must_use]
pub fn render_markdown(document: &StructuredDocument) -> String {
    let mut out = String::new();
    for page in &document.pages {
        for block in &page.blocks {
            if !block.kind.is_textual() {
                continue;
            }
            let text = block.text.trim();
            if text.is_empty() {
                continue;
            }
            match &block.kind {
                BlockKind::Title => {
                    push_block(&mut out, &format!("# {text}"));
                }
                BlockKind::Heading { level } => {
                    let hashes = "#".repeat(usize::from((*level).max(1)));
                    push_block(&mut out, &format!("{hashes} {text}"));
                }
                BlockKind::ListItem => {
                    push_block(&mut out, &format!("- {text}"));
                }
                _ => push_block(&mut out, text),
            }
        }
    }
    out
}

fn push_block(out: &mut String, block: &str) {
    if !out.is_empty() {
        out.push_str("\n\n");
    }
    out.push_str(block);
}

/// Converts Markdown into plain UTF-8 text.
///
/// The conversion is deliberately structural: headings, emphasis markers,
/// links, images, list markers, and horizontal rules are removed, while the
/// sentence text and paragraph boundaries are preserved. No re-wrapping is
/// performed, so character offsets stay meaningful.
#[must_use]
pub fn to_plain(markdown: &str) -> String {
    let mut lines = Vec::new();
    for line in markdown.lines() {
        let trimmed = line.trim_end();
        if is_rule(trimmed) {
            continue;
        }
        let without_heading = strip_heading(trimmed);
        let without_image = strip_images(without_heading);
        let without_links = strip_links(&without_image);
        let without_emphasis = strip_emphasis(&without_links);
        let without_marker = strip_list_marker(without_emphasis.trim());
        lines.push(without_marker.trim_end().to_owned());
    }
    let joined = lines.join("\n");
    let mut out = String::with_capacity(joined.len());
    let mut blank_run = 0_usize;
    for line in joined.lines() {
        if line.trim().is_empty() {
            blank_run += 1;
            if blank_run > 1 {
                continue;
            }
        } else {
            blank_run = 0;
        }
        out.push_str(line);
        out.push('\n');
    }
    out.trim_end().to_owned()
}

fn is_rule(line: &str) -> bool {
    let trimmed = line.trim();
    if trimmed.len() < 3 {
        return false;
    }
    let first = trimmed.chars().next().unwrap_or(' ');
    matches!(first, '=' | '-' | '*' | '_')
        && trimmed.chars().all(|c| c == first || c == ' ')
        && trimmed.chars().filter(|c| *c == first).count() >= 3
}

fn strip_heading(line: &str) -> &str {
    let trimmed = line.trim_start();
    let hashes = trimmed.chars().take_while(|c| *c == '#').count();
    if hashes > 0 && hashes <= 6 && trimmed[hashes..].starts_with(' ') {
        trimmed[hashes..].trim_start()
    } else {
        line
    }
}

fn strip_list_marker(line: &str) -> &str {
    for marker in ["- ", "* ", "+ "] {
        if let Some(rest) = line.strip_prefix(marker) {
            return rest;
        }
    }
    let digits = line.chars().take_while(char::is_ascii_digit).count();
    if digits > 0 {
        let rest = &line[digits..];
        if let Some(rest) = rest.strip_prefix(". ").or_else(|| rest.strip_prefix(") ")) {
            return rest;
        }
    }
    line
}

fn strip_emphasis(value: &str) -> String {
    value
        .replace("**", "")
        .replace("__", "")
        .replace("~~", "")
        .replace('`', "")
}

fn strip_images(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    let mut rest = value;
    while let Some(start) = rest.find("![") {
        out.push_str(&rest[..start]);
        if let Some(end) = rest[start..].find("](") {
            let after = &rest[start + end + 2..];
            if let Some(close) = after.find(')') {
                rest = &after[close + 1..];
                continue;
            }
        }
        out.push_str(&rest[start..]);
        return out;
    }
    out.push_str(rest);
    out
}

fn strip_links(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    let mut rest = value;
    while let Some(start) = rest.find('[') {
        out.push_str(&rest[..start]);
        if let Some(end) = rest[start..].find("](") {
            let label = &rest[start + 1..start + end];
            let after = &rest[start + end + 2..];
            if let Some(close) = after.find(')') {
                out.push_str(label);
                rest = &after[close + 1..];
                continue;
            }
        }
        out.push_str(&rest[start..]);
        return out;
    }
    out.push_str(rest);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plain_text_drops_markdown_syntax() {
        let markdown = "# Title\n\nSome **bold** and a [link](https://example.org).\n\n\
                        ![figure](images/f.png)\n\n- first\n- second\n\n---\n\n## Next\n\n1. one\n2. two\n";
        let plain = to_plain(markdown);
        assert_eq!(
            plain,
            "Title\n\nSome bold and a link.\n\nfirst\nsecond\n\nNext\n\none\ntwo"
        );
    }

    #[test]
    fn plain_text_keeps_sentence_characters() {
        let plain = to_plain("Use snake_case names and a * b arithmetic.");
        assert_eq!(plain, "Use snake_case names and a * b arithmetic.");
    }

    #[test]
    fn markdown_render_skips_visual_blocks() {
        use ontology_core::document::{
            Block, BlockKind, DocumentMetadata, Page, StructuredDocument,
        };

        let mut document = StructuredDocument {
            document_id: "urn:sha256:abc".to_owned(),
            metadata: DocumentMetadata::default(),
            pages: vec![Page {
                index: 0,
                width: None,
                height: None,
                unit: None,
                blocks: vec![
                    Block::new("urn:onto:document:abc/block/1", BlockKind::Title, "T"),
                    Block::new(
                        "urn:onto:document:abc/block/2",
                        BlockKind::Paragraph,
                        "Body.",
                    ),
                    Block::new("urn:onto:document:abc/block/3", BlockKind::Figure, ""),
                ],
            }],
            sections: Vec::new(),
            references: Vec::new(),
            citations: Vec::new(),
            provenance: crate::tests_support::provenance(),
        };
        document.pages[0].blocks[0].reading_order = 0;
        document.pages[0].blocks[1].reading_order = 1;
        document.pages[0].blocks[2].reading_order = 2;
        let markdown = render_markdown(&document);
        assert_eq!(markdown, "# T\n\nBody.");
    }
}
