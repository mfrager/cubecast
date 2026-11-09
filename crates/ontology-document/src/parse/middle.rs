//! Raw MinerU artefact extraction.
//!
//! The reader is intentionally tolerant: MinerU releases add fields, rename
//! containers, and classify blocks differently. Anything it does not recognise
//! is preserved as [`BlockKind::Unknown`] with its text, so information is
//! never dropped silently.

use std::collections::BTreeMap;

use serde_json::Value;

use ontology_core::document::{
    AssetRef, Block, BlockKind, Caption, Formula, FormulaType, Geometry, Line, ListBlock, Span,
    Table, TableCell, TableRow,
};
use ontology_core::error::{Error, Result};

use crate::parse::SchemaKind;

pub(crate) struct RawPage {
    pub index: u32,
    pub width: Option<f64>,
    pub height: Option<f64>,
    pub unit: Option<String>,
    pub blocks: Vec<RawBlock>,
    pub discarded: Vec<RawBlock>,
}

pub(crate) enum RawBlock {
    Block(BlockDraft),
}

impl RawBlock {
    pub fn into_block(self, base: &str, counter: u32, reading_order: u32) -> Block {
        let Self::Block(draft) = self;
        let mut block = Block::new(format!("{base}/block/{counter}"), draft.kind, draft.text);
        block.geometry = draft.geometry;
        block.lines = draft.lines;
        block.caption = draft.caption.map(|text| Caption { text });
        block.asset = draft.asset;
        block.table = draft.table;
        block.formula = draft.formula;
        block.list = draft.list;
        block.reading_order = reading_order;
        block
    }
}

pub(crate) struct BlockDraft {
    pub kind: BlockKind,
    pub text: String,
    pub geometry: Option<Geometry>,
    pub lines: Vec<Line>,
    pub caption: Option<String>,
    pub asset: Option<AssetRef>,
    pub table: Option<Table>,
    pub formula: Option<Formula>,
    pub list: Option<ListBlock>,
}

/// Extracts raw pages from any supported artefact family.
///
/// # Errors
///
/// Returns [`Error::Extraction`] when a page or block container has the wrong
/// JSON shape.
pub(crate) fn extract_pages(
    value: &Value,
    kind: SchemaKind,
    warnings: &mut Vec<ontology_core::document::ExtractionWarning>,
) -> Result<Vec<RawPage>> {
    match kind {
        SchemaKind::Middle | SchemaKind::LegacyMiddle => {
            let pages = value
                .get("pdf_info")
                .and_then(Value::as_array)
                .ok_or_else(|| Error::extraction("parse", "middle JSON has no pdf_info array"))?;
            pages
                .iter()
                .enumerate()
                .map(|(index, page)| page_from_middle(page, index, warnings))
                .collect()
        }
        SchemaKind::StructuredContent => {
            let pages = value
                .get("pages")
                .and_then(Value::as_array)
                .ok_or_else(|| {
                    Error::extraction("parse", "structured content has no pages array")
                })?;
            pages
                .iter()
                .enumerate()
                .map(|(index, page)| page_from_middle(page, index, warnings))
                .collect()
        }
        SchemaKind::ContentList => content_list_pages(value, warnings),
    }
}

fn page_from_middle(
    page: &Value,
    index: usize,
    warnings: &mut Vec<ontology_core::document::ExtractionWarning>,
) -> Result<RawPage> {
    let page_index = page
        .get("page_idx")
        .or_else(|| page.get("page_index"))
        .or_else(|| page.get("index"))
        .and_then(Value::as_u64)
        .map_or(index as u32, |value| value as u32);
    let (width, height) = page_size(page);
    let unit = page
        .get("unit")
        .and_then(Value::as_str)
        .map(str::to_owned)
        .or_else(|| Some("pt".to_owned()));

    let containers = ["para_blocks", "blocks", "preproc_blocks", "layout_blocks"];
    let mut blocks = Vec::new();
    for key in containers {
        if let Some(items) = page.get(key).and_then(Value::as_array) {
            for item in items {
                if let Some(draft) = block_draft(item, warnings, page_index, height) {
                    blocks.push(RawBlock::Block(draft));
                }
            }
            if !blocks.is_empty() {
                break;
            }
        }
    }

    let mut discarded = Vec::new();
    if let Some(items) = page.get("discarded_blocks").and_then(Value::as_array) {
        for item in items {
            if let Some(mut draft) = block_draft(item, warnings, page_index, height) {
                let text = draft.text.trim();
                if text.is_empty() {
                    continue;
                }
                let classified = if let Some(geometry) = draft.geometry.clone() {
                    if height.is_some_and(|height| geometry.y1 <= height * 0.12) {
                        BlockKind::PageHeader
                    } else {
                        BlockKind::PageFooter
                    }
                } else if matches!(draft.kind, BlockKind::PageHeader | BlockKind::PageFooter) {
                    draft.kind.clone()
                } else {
                    BlockKind::PageFooter
                };
                draft.kind = classified;
                discarded.push(RawBlock::Block(draft));
            }
        }
    }

    Ok(RawPage {
        index: page_index,
        width,
        height,
        unit,
        blocks,
        discarded,
    })
}

fn page_size(page: &Value) -> (Option<f64>, Option<f64>) {
    if let Some(size) = page.get("page_size").and_then(Value::as_array) {
        if size.len() >= 2 {
            return (number(&size[0]), number(&size[1]));
        }
    }
    (
        page.get("width").and_then(number),
        page.get("height").and_then(number),
    )
}

fn block_draft(
    value: &Value,
    warnings: &mut Vec<ontology_core::document::ExtractionWarning>,
    page: u32,
    page_height: Option<f64>,
) -> Option<BlockDraft> {
    let label = value
        .get("type")
        .or_else(|| value.get("block_type"))
        .or_else(|| value.get("label"))
        .and_then(Value::as_str)
        .unwrap_or("unknown")
        .to_ascii_lowercase();

    let _ = page_height;
    let (kind, formula_type) = kind_for(&label, value);

    let lines = value
        .get("lines")
        .and_then(Value::as_array)
        .map(|lines| lines.iter().map(line_from_value).collect::<Vec<_>>())
        .unwrap_or_default();
    let mut text = value
        .get("content")
        .or_else(|| value.get("text"))
        .and_then(Value::as_str)
        .map(str::to_owned)
        .unwrap_or_else(|| {
            lines
                .iter()
                .map(|line| line.text.as_str())
                .collect::<Vec<_>>()
                .join("\n")
        });
    text = text.trim().to_owned();

    let geometry = value
        .get("bbox")
        .or_else(|| value.get("geometry"))
        .and_then(parse_geometry)
        .or_else(|| lines.iter().find_map(|line| line.geometry.clone()));

    if geometry.is_none() && !matches!(kind, BlockKind::Unknown { .. }) {
        warnings.push(
            ontology_core::document::ExtractionWarning::new(
                "missing_geometry",
                "block has no bounding box",
            )
            .with_page(page),
        );
    }

    let caption = caption_text(value);
    let asset = image_path(value).map(asset_ref);

    let table = (kind == BlockKind::Table).then(|| table_from_value(value));
    let formula = (kind == BlockKind::Formula).then(|| Formula {
        latex: value
            .get("latex")
            .or_else(|| value.get("text"))
            .and_then(Value::as_str)
            .map(str::to_owned),
        mathml: value
            .get("mathml")
            .and_then(Value::as_str)
            .map(str::to_owned),
        formula_type,
        asset: asset.clone(),
    });
    let list = (kind == BlockKind::List).then(|| list_from_value(value));

    let is_formula = kind == BlockKind::Formula;
    let draft = BlockDraft {
        kind,
        text,
        geometry,
        lines,
        caption,
        asset: if is_formula { None } else { asset },
        table,
        formula,
        list,
    };
    if draft.text.is_empty()
        && draft.asset.is_none()
        && draft.table.is_none()
        && draft.formula.is_none()
        && draft.kind != BlockKind::Figure
        && draft.kind != BlockKind::Table
    {
        return None;
    }
    Some(draft)
}

fn kind_for(label: &str, value: &Value) -> (BlockKind, FormulaType) {
    let level = value
        .get("level")
        .or_else(|| value.get("text_level"))
        .and_then(Value::as_u64)
        .map(|level| level.clamp(1, 9) as u8);
    match label {
        "title" => (BlockKind::Title, FormulaType::Interline),
        "heading" | "section_header" | "section_title" => (
            BlockKind::Heading {
                level: level.unwrap_or(1),
            },
            FormulaType::Interline,
        ),
        "h1" | "h2" | "h3" | "h4" | "h5" | "h6" => (
            BlockKind::Heading {
                level: label[1..].parse::<u8>().unwrap_or(1),
            },
            FormulaType::Interline,
        ),
        "text" | "paragraph" | "body" => (BlockKind::Paragraph, FormulaType::Interline),
        "list" => (BlockKind::List, FormulaType::Interline),
        "list_item" | "listitem" | "item" => (BlockKind::ListItem, FormulaType::Interline),
        "image" | "figure" | "picture" | "figure_body" => {
            (BlockKind::Figure, FormulaType::Interline)
        }
        "table" | "table_body" => (BlockKind::Table, FormulaType::Interline),
        "interline_equation" | "equation" | "formula" | "display_formula" => {
            (BlockKind::Formula, FormulaType::Interline)
        }
        "inline_equation" | "inline_formula" => (BlockKind::Formula, FormulaType::Inline),
        "footnote" => (BlockKind::Footnote, FormulaType::Interline),
        "header" | "page_header" => (BlockKind::PageHeader, FormulaType::Interline),
        "footer" | "page_footer" => (BlockKind::PageFooter, FormulaType::Interline),
        "reference" | "ref" | "bibliography" => (BlockKind::Reference, FormulaType::Interline),
        "code" | "code_block" => (BlockKind::Code, FormulaType::Interline),
        other => (
            BlockKind::Unknown {
                label: other.to_owned(),
            },
            FormulaType::Interline,
        ),
    }
}

fn line_from_value(value: &Value) -> Line {
    let spans = value
        .get("spans")
        .and_then(Value::as_array)
        .map(|spans| spans.iter().map(span_from_value).collect::<Vec<_>>())
        .unwrap_or_default();
    let text = value
        .get("text")
        .or_else(|| value.get("content"))
        .and_then(Value::as_str)
        .map(str::to_owned)
        .unwrap_or_else(|| {
            spans
                .iter()
                .map(|span| span.text.as_str())
                .collect::<String>()
        });
    Line {
        text: text.trim().to_owned(),
        geometry: value
            .get("bbox")
            .or_else(|| value.get("geometry"))
            .and_then(parse_geometry),
        spans,
    }
}

fn span_from_value(value: &Value) -> Span {
    let text = value
        .get("content")
        .or_else(|| value.get("text"))
        .and_then(Value::as_str)
        .map(str::to_owned)
        .unwrap_or_default();
    Span {
        text,
        geometry: value
            .get("bbox")
            .or_else(|| value.get("geometry"))
            .and_then(parse_geometry),
        style: value
            .get("style")
            .or_else(|| value.get("font"))
            .and_then(Value::as_str)
            .map(str::to_owned),
    }
}

fn parse_geometry(value: &Value) -> Option<Geometry> {
    if let Some(array) = value.as_array() {
        if array.len() >= 4 {
            return Some(Geometry {
                x0: number(&array[0])?,
                y0: number(&array[1])?,
                x1: number(&array[2])?,
                y1: number(&array[3])?,
                unit: Some("pt".to_owned()),
                origin: Some("top-left".to_owned()),
            });
        }
        return None;
    }
    let object = value.as_object()?;
    let get = |key: &str| object.get(key).and_then(number);
    Some(Geometry {
        x0: get("x0")?,
        y0: get("y0")?,
        x1: get("x1")?,
        y1: get("y1")?,
        unit: object
            .get("unit")
            .and_then(Value::as_str)
            .map(str::to_owned),
        origin: object
            .get("origin")
            .and_then(Value::as_str)
            .map(str::to_owned),
    })
}

fn number(value: &Value) -> Option<f64> {
    value
        .as_f64()
        .or_else(|| value.as_str().and_then(|text| text.parse().ok()))
}

fn caption_text(value: &Value) -> Option<String> {
    for key in ["caption", "figure_caption", "table_caption"] {
        if let Some(caption) = value.get(key) {
            if let Some(text) = caption.as_str() {
                if !text.trim().is_empty() {
                    return Some(text.trim().to_owned());
                }
            }
            if let Some(items) = caption.as_array() {
                let joined = items
                    .iter()
                    .filter_map(|item| {
                        item.as_str().map(str::to_owned).or_else(|| {
                            item.get("content")
                                .or_else(|| item.get("text"))
                                .and_then(Value::as_str)
                                .map(str::to_owned)
                        })
                    })
                    .collect::<Vec<_>>()
                    .join(" ");
                if !joined.trim().is_empty() {
                    return Some(joined.trim().to_owned());
                }
            }
        }
    }
    None
}

fn image_path(value: &Value) -> Option<String> {
    for key in ["img_path", "image_path", "img", "path"] {
        if let Some(path) = value.get(key).and_then(Value::as_str) {
            if !path.trim().is_empty() {
                return Some(path.trim().to_owned());
            }
        }
    }
    None
}

fn asset_ref(path: String) -> AssetRef {
    let mime = mime_for(&path);
    let normalized = if path.starts_with("images/") {
        path
    } else if !path.contains('/') {
        format!("images/{path}")
    } else {
        let name = path.rsplit('/').next().unwrap_or(&path).to_owned();
        format!("images/{name}")
    };
    AssetRef {
        path: normalized,
        mime,
        sha256: String::new(),
    }
}

fn mime_for(path: &str) -> String {
    let lower = path.to_ascii_lowercase();
    if lower.ends_with(".png") {
        "image/png"
    } else if lower.ends_with(".jpg") || lower.ends_with(".jpeg") {
        "image/jpeg"
    } else if lower.ends_with(".webp") {
        "image/webp"
    } else if lower.ends_with(".svg") {
        "image/svg+xml"
    } else {
        "application/octet-stream"
    }
    .to_owned()
}

fn table_from_value(value: &Value) -> Table {
    let html = value
        .get("html")
        .or_else(|| value.get("table_html"))
        .and_then(Value::as_str)
        .map(str::to_owned)
        .or_else(|| {
            value
                .get("content")
                .and_then(Value::as_str)
                .filter(|content| content.trim_start().starts_with("<table"))
                .map(str::to_owned)
        });
    let markdown = value
        .get("markdown")
        .or_else(|| value.get("table_markdown"))
        .and_then(Value::as_str)
        .map(str::to_owned);
    let mut rows = Vec::new();
    if let Some(items) = value
        .get("rows")
        .or_else(|| value.get("table_rows"))
        .and_then(Value::as_array)
    {
        for (row_index, row) in items.iter().enumerate() {
            let index = row
                .get("index")
                .and_then(Value::as_u64)
                .map_or(row_index as u32, |value| value as u32);
            let cells = row
                .get("cells")
                .and_then(Value::as_array)
                .map(|cells| {
                    cells
                        .iter()
                        .enumerate()
                        .map(|(column, cell)| TableCell {
                            row: cell
                                .get("row")
                                .and_then(Value::as_u64)
                                .map_or(index, |value| value as u32),
                            column: cell
                                .get("column")
                                .or_else(|| cell.get("col"))
                                .and_then(Value::as_u64)
                                .map_or(column as u32, |value| value as u32),
                            row_span: cell
                                .get("row_span")
                                .and_then(Value::as_u64)
                                .map_or(1, |value| value.max(1) as u32),
                            column_span: cell
                                .get("column_span")
                                .or_else(|| cell.get("col_span"))
                                .and_then(Value::as_u64)
                                .map_or(1, |value| value.max(1) as u32),
                            is_header: cell
                                .get("is_header")
                                .or_else(|| cell.get("header"))
                                .and_then(Value::as_bool)
                                .unwrap_or(false),
                            text: cell
                                .get("text")
                                .or_else(|| cell.get("content"))
                                .and_then(Value::as_str)
                                .unwrap_or_default()
                                .trim()
                                .to_owned(),
                        })
                        .collect()
                })
                .unwrap_or_default();
            rows.push(TableRow { index, cells });
        }
    }
    if rows.is_empty() {
        if let Some(html) = &html {
            rows = rows_from_html(html);
        }
    }
    Table {
        html,
        markdown,
        rows,
    }
}

fn rows_from_html(html: &str) -> Vec<TableRow> {
    let lower = html.to_ascii_lowercase();
    let mut rows = Vec::new();
    let mut cursor = 0;
    while let Some(start) = lower[cursor..].find("<tr") {
        let start = cursor + start;
        let end = lower[start..]
            .find("</tr>")
            .map(|offset| start + offset)
            .unwrap_or(html.len());
        let row_html = &html[start..end];
        let row_index = rows.len() as u32;
        let mut cells = Vec::new();
        let mut cell_cursor = 0;
        let row_lower = row_html.to_ascii_lowercase();
        loop {
            let td = row_lower[cell_cursor..]
                .find("<td")
                .map(|offset| (cell_cursor + offset, false));
            let th = row_lower[cell_cursor..]
                .find("<th")
                .map(|offset| (cell_cursor + offset, true));
            let (cell_start, header) = match (td, th) {
                (Some(td), Some(th)) => {
                    if td.0 <= th.0 {
                        td
                    } else {
                        th
                    }
                }
                (Some(td), None) => td,
                (None, Some(th)) => th,
                (None, None) => break,
            };
            let Some(open_end) = row_html[cell_start..].find('>') else {
                break;
            };
            let content_start = cell_start + open_end + 1;
            let close = row_lower[content_start..]
                .find("</td>")
                .or_else(|| row_lower[content_start..].find("</th>"));
            let Some(close) = close else {
                break;
            };
            let content_end = content_start + close;
            let text = strip_tags(&row_html[content_start..content_end]);
            cells.push(TableCell {
                row: row_index,
                column: cells.len() as u32,
                row_span: 1,
                column_span: 1,
                is_header: header,
                text: decode_entities(text.trim()),
            });
            cell_cursor = content_end + 5;
        }
        if !cells.is_empty() {
            rows.push(TableRow {
                index: row_index,
                cells,
            });
        }
        cursor = end;
    }
    rows
}

fn strip_tags(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    let mut in_tag = false;
    for character in value.chars() {
        match character {
            '<' => in_tag = true,
            '>' => in_tag = false,
            other if !in_tag => out.push(other),
            _ => {}
        }
    }
    out
}

fn decode_entities(value: &str) -> String {
    value
        .replace("&amp;", "&")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&#39;", "'")
        .replace("&nbsp;", " ")
}

fn list_from_value(value: &Value) -> ListBlock {
    let ordered = value
        .get("ordered")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let items = value
        .get("items")
        .or_else(|| value.get("list_items"))
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .filter_map(|item| {
                    item.as_str().map(str::to_owned).or_else(|| {
                        item.get("content")
                            .or_else(|| item.get("text"))
                            .and_then(Value::as_str)
                            .map(str::to_owned)
                    })
                })
                .collect()
        })
        .unwrap_or_default();
    ListBlock { ordered, items }
}

fn content_list_pages(
    value: &Value,
    warnings: &mut Vec<ontology_core::document::ExtractionWarning>,
) -> Result<Vec<RawPage>> {
    let items = value
        .get("content_list")
        .and_then(Value::as_array)
        .or_else(|| value.as_array())
        .ok_or_else(|| Error::extraction("parse", "content list has no items array"))?;
    let mut pages: BTreeMap<u32, RawPage> = BTreeMap::new();
    for item in items {
        let page_index = item
            .get("page_idx")
            .or_else(|| item.get("page_index"))
            .and_then(Value::as_u64)
            .unwrap_or(0) as u32;
        let page = pages.entry(page_index).or_insert_with(|| RawPage {
            index: page_index,
            width: None,
            height: None,
            unit: Some("pt".to_owned()),
            blocks: Vec::new(),
            discarded: Vec::new(),
        });
        let mut value = item.clone();
        if value.get("type").and_then(Value::as_str) == Some("text") {
            if let Some(level) = value.get("text_level").and_then(Value::as_u64) {
                let level = level.clamp(1, 9) as u8;
                value["type"] = Value::String(if level == 1 {
                    "title".to_owned()
                } else {
                    "heading".to_owned()
                });
                value["level"] = Value::from(level);
            }
        }
        if let Some(draft) = block_draft(&value, warnings, page_index, None) {
            page.blocks.push(RawBlock::Block(draft));
        }
    }
    Ok(pages.into_values().collect())
}
