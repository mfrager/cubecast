//! DoCO/BIBO/Schema.org/PROV-O mapping for extracted documents.
//!
//! Documents are written into four named graphs so layout, structure,
//! bibliography, and provenance can be queried independently. Identifiers are
//! deterministic: the same model always produces the same quads.

use std::collections::BTreeSet;

use ontology_core::document::{
    Block, BlockKind, Citation, Formula, Geometry, Reference, Section, StructuredDocument,
};
use ontology_core::error::{Error, Result};
use ontology_core::id::Id;
use ontology_core::model::{GraphName, Literal, Quad, QuadPattern, Term};
use ontology_core::namespace::RDF_TYPE;
use ontology_core::rdf::RdfStore;
use serde::{Deserialize, Serialize};

const XSD_DOUBLE: &str = "http://www.w3.org/2001/XMLSchema#double";
const BIBO_ACADEMIC: &str = "http://purl.org/ontology/bibo/AcademicArticle";
const SCHEMA_SCHOLARLY: &str = "https://schema.org/ScholarlyArticle";
const SCHEMA_PERSON: &str = "https://schema.org/Person";
const SCHEMA_IMAGE: &str = "https://schema.org/ImageObject";
const SCHEMA_TABLE: &str = "https://schema.org/Table";
const DOCO_SECTION: &str = "http://purl.org/spar/doco/Section";
const DOCO_PARAGRAPH: &str = "http://purl.org/spar/doco/Paragraph";
const DOCO_FIGURE: &str = "http://purl.org/spar/doco/Figure";
const DOCO_TABLE: &str = "http://purl.org/spar/doco/Table";
const DOCO_FOOTNOTE: &str = "http://purl.org/spar/doco/Footnote";
const DOCO_LIST: &str = "http://purl.org/spar/doco/List";
const DOCO_LIST_ITEM: &str = "http://purl.org/spar/doco/ListItem";
const DOCO_CAPTION: &str = "http://purl.org/spar/doco/Caption";
const DOCO_REFERENCE: &str = "http://purl.org/spar/doco/BibliographicReference";
const PROV_ACTIVITY: &str = "http://www.w3.org/ns/prov#Activity";
const PROV_SOFTWARE_AGENT: &str = "http://www.w3.org/ns/prov#SoftwareAgent";

/// Summary of one RDF mapping.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DocumentMapping {
    /// Document IRI.
    pub document: String,
    /// Number of quads written.
    pub quads: usize,
    /// Named graphs written.
    pub graphs: Vec<String>,
}

struct Graphs {
    structure: Id,
    layout: Id,
    bibliography: Id,
    provenance: Id,
}

impl Graphs {
    fn for_document(hex: &str) -> Result<Self> {
        let graph = |facet: &str| Id::new(format!("urn:onto:data:document:{hex}:{facet}"));
        Ok(Self {
            structure: graph("structure")?,
            layout: graph("layout")?,
            bibliography: graph("bibliography")?,
            provenance: graph("provenance")?,
        })
    }

    fn all(&self) -> [&Id; 4] {
        [
            &self.structure,
            &self.layout,
            &self.bibliography,
            &self.provenance,
        ]
    }
}

/// Maps a document to quads without writing them.
///
/// # Errors
///
/// Returns [`Error::Extraction`] when the model is unusable, or an IRI error
/// for an invalid identifier.
pub fn map_document(document: &StructuredDocument) -> Result<Vec<Quad>> {
    let hex = crate::ids::hex_suffix(&document.document_id)?;
    let graphs = Graphs::for_document(hex)?;
    let doc = Id::new(format!("urn:onto:document:{hex}"))?;
    let mut mapper = Mapper {
        document,
        doc: doc.clone(),
        graphs,
        quads: Vec::new(),
    };
    mapper.map()?;
    let mut quads = mapper.quads;
    quads.sort();
    quads.dedup();
    validate(document, &quads)?;
    Ok(quads)
}

/// Writes a document atomically into its four graphs.
///
/// Existing quads in those graphs are replaced, so re-extraction is
/// idempotent instead of duplicating statements.
///
/// # Errors
///
/// Propagates mapping and storage failures.
pub fn write_document(
    store: &dyn RdfStore,
    document: &StructuredDocument,
) -> Result<DocumentMapping> {
    let quads = map_document(document)?;
    let hex = crate::ids::hex_suffix(&document.document_id)?;
    let graphs = Graphs::for_document(hex)?;

    match store.transaction() {
        Ok(mut transaction) => {
            for graph in graphs.all() {
                let existing =
                    store.get(QuadPattern::new().with_graph(GraphName::Iri(graph.clone())))?;
                if !existing.is_empty() {
                    transaction.delete(&existing)?;
                }
            }
            transaction.insert(&quads)?;
            transaction.commit()?;
        }
        Err(Error::Unsupported { .. }) => {
            for graph in graphs.all() {
                let existing =
                    store.get(QuadPattern::new().with_graph(GraphName::Iri(graph.clone())))?;
                if !existing.is_empty() {
                    store.delete(&existing)?;
                }
            }
            store.insert(&quads)?;
        }
        Err(error) => return Err(error),
    }

    Ok(DocumentMapping {
        document: format!("urn:onto:document:{hex}"),
        quads: quads.len(),
        graphs: graphs.all().iter().map(|graph| graph.to_string()).collect(),
    })
}

fn validate(document: &StructuredDocument, quads: &[Quad]) -> Result<()> {
    let subjects: BTreeSet<String> = quads
        .iter()
        .filter_map(|quad| quad.subject.as_iri().map(ToString::to_string))
        .collect();
    for page in &document.pages {
        for block in &page.blocks {
            if !subjects.contains(&block.id) {
                return Err(Error::extraction(
                    "rdf",
                    format!("block {} produced no RDF statements", block.id),
                ));
            }
        }
    }
    let unique: BTreeSet<&Quad> = quads.iter().collect();
    if unique.len() != quads.len() {
        return Err(Error::extraction(
            "rdf",
            "duplicate quads in document mapping",
        ));
    }
    Ok(())
}

struct Mapper<'a> {
    document: &'a StructuredDocument,
    doc: Id,
    graphs: Graphs,
    quads: Vec<Quad>,
}

impl Mapper<'_> {
    fn map(&mut self) -> Result<()> {
        self.map_document()?;
        self.map_authors()?;
        self.map_pages()?;
        self.map_sections()?;
        self.map_references()?;
        self.map_citations()?;
        self.map_provenance()?;
        Ok(())
    }

    fn map_document(&mut self) -> Result<()> {
        let metadata = &self.document.metadata;
        self.add_type(
            &self.doc.clone(),
            BIBO_ACADEMIC,
            &self.graphs.structure.clone(),
        )?;
        self.add_type(
            &self.doc.clone(),
            SCHEMA_SCHOLARLY,
            &self.graphs.structure.clone(),
        )?;
        if let Some(title) = &metadata.title {
            self.literal(
                &self.doc.clone(),
                "title",
                title,
                &self.graphs.structure.clone(),
            )?;
            self.literal(
                &self.doc.clone(),
                "headline",
                title,
                &self.graphs.structure.clone(),
            )?;
        }
        if let Some(abstract_text) = &metadata.abstract_text {
            self.literal(
                &self.doc.clone(),
                "abstract",
                abstract_text,
                &self.graphs.structure.clone(),
            )?;
        }
        if let Some(language) = &metadata.language {
            self.literal(
                &self.doc.clone(),
                "language",
                language,
                &self.graphs.structure.clone(),
            )?;
        }
        self.literal(
            &self.doc.clone(),
            "identifier",
            &self.document.document_id,
            &self.graphs.bibliography.clone(),
        )?;
        self.integer(
            &self.doc.clone(),
            "pageCount",
            metadata.page_count as i64,
            &self.graphs.structure.clone(),
        )?;
        if let Some(source) = &metadata.source_file_name {
            self.literal(
                &self.doc.clone(),
                "source",
                source,
                &self.graphs.bibliography.clone(),
            )?;
        }
        Ok(())
    }

    fn map_authors(&mut self) -> Result<()> {
        for (index, author) in self.document.metadata.authors.iter().enumerate() {
            let iri = Id::new(format!("{}/author/{}", self.doc, index + 1))?;
            self.add_type(&iri, SCHEMA_PERSON, &self.graphs.bibliography.clone())?;
            self.literal(
                &iri,
                "name",
                &author.name,
                &self.graphs.bibliography.clone(),
            )?;
            if let Some(affiliation) = &author.affiliation {
                self.literal(
                    &iri,
                    "affiliation",
                    affiliation,
                    &self.graphs.bibliography.clone(),
                )?;
            }
            self.integer(
                &iri,
                "position",
                index as i64 + 1,
                &self.graphs.bibliography.clone(),
            )?;
            self.iri(
                &self.doc.clone(),
                "author",
                &iri,
                &self.graphs.bibliography.clone(),
            )?;
        }
        Ok(())
    }

    fn map_pages(&mut self) -> Result<()> {
        for page in &self.document.pages {
            let iri = Id::new(format!("{}/page/{}", self.doc, page.index + 1))?;
            self.add_type(&iri, "Page", &self.graphs.layout.clone())?;
            self.integer(
                &iri,
                "pageIndex",
                i64::from(page.index),
                &self.graphs.layout.clone(),
            )?;
            if let Some(width) = page.width {
                self.float(&iri, "pageWidth", width, &self.graphs.layout.clone())?;
            }
            if let Some(height) = page.height {
                self.float(&iri, "pageHeight", height, &self.graphs.layout.clone())?;
            }
            if let Some(unit) = &page.unit {
                self.literal(&iri, "coordinateUnit", unit, &self.graphs.layout.clone())?;
            }
            self.iri(
                &iri,
                "partOfDocument",
                &self.doc.clone(),
                &self.graphs.layout.clone(),
            )?;

            for block in &page.blocks {
                let block_iri = Id::new(block.id.clone())?;
                self.map_block(block, &block_iri, &iri, page.index)?;
            }
        }
        Ok(())
    }

    fn map_block(&mut self, block: &Block, iri: &Id, page: &Id, page_index: u32) -> Result<()> {
        let structure = self.graphs.structure.clone();
        let layout = self.graphs.layout.clone();
        self.add_type(iri, block_class(&block.kind), &structure)?;
        if !block.text.is_empty() {
            self.literal(iri, "text", &block.text, &structure)?;
        }
        self.iri(iri, "appearsOnPage", page, &structure)?;
        self.integer(
            iri,
            "readingOrderIndex",
            i64::from(block.reading_order),
            &structure,
        )?;
        if let Some(section) = &block.section {
            let section = Id::new(section.clone())?;
            self.iri(iri, "belongsToSection", &section, &structure)?;
        }
        if let Some(geometry) = &block.geometry {
            self.map_geometry(iri, geometry, page_index, 1)?;
        }
        for (index, line) in block.lines.iter().enumerate() {
            let line_iri = Id::new(format!("{iri}/line/{}", index + 1))?;
            self.add_type(&line_iri, "Line", &layout)?;
            self.literal(&line_iri, "text", &line.text, &layout)?;
            self.integer(&line_iri, "lineIndex", index as i64, &layout)?;
            self.iri(iri, "hasLine", &line_iri, &layout)?;
            if let Some(geometry) = &line.geometry {
                self.map_geometry(&line_iri, geometry, page_index, index as u32 + 1)?;
            }
            for (span_index, span) in line.spans.iter().enumerate() {
                let span_iri = Id::new(format!("{line_iri}/span/{}", span_index + 1))?;
                self.add_type(&span_iri, "Span", &layout)?;
                self.literal(&span_iri, "text", &span.text, &layout)?;
                self.iri(&line_iri, "hasSpan", &span_iri, &layout)?;
                if let Some(style) = &span.style {
                    self.literal(&span_iri, "style", style, &layout)?;
                }
            }
        }
        if let Some(caption) = &block.caption {
            let caption_iri = Id::new(format!("{iri}/caption"))?;
            self.add_type(&caption_iri, DOCO_CAPTION, &structure)?;
            self.literal(&caption_iri, "text", &caption.text, &structure)?;
            self.iri(iri, "hasCaption", &caption_iri, &structure)?;
        }
        if let Some(asset) = &block.asset {
            let asset_iri = Id::new(format!("{iri}/asset"))?;
            self.add_type(&asset_iri, SCHEMA_IMAGE, &structure)?;
            self.literal(&asset_iri, "path", &asset.path, &structure)?;
            self.literal(&asset_iri, "mimeType", &asset.mime, &structure)?;
            if !asset.sha256.is_empty() {
                self.literal(&asset_iri, "sha256", &asset.sha256, &structure)?;
            }
            self.iri(iri, "hasAsset", &asset_iri, &structure)?;
        }
        if let Some(table) = &block.table {
            let table_iri = Id::new(format!("{iri}/table"))?;
            self.add_type(&table_iri, SCHEMA_TABLE, &structure)?;
            if let Some(html) = &table.html {
                self.literal(&table_iri, "tableHtml", html, &structure)?;
            }
            if let Some(markdown) = &table.markdown {
                self.literal(&table_iri, "tableMarkdown", markdown, &structure)?;
            }
            self.iri(iri, "hasTable", &table_iri, &structure)?;
            for row in &table.rows {
                let row_iri = Id::new(format!("{table_iri}/row/{}", row.index))?;
                self.add_type(&row_iri, "TableRow", &structure)?;
                self.integer(&row_iri, "rowIndex", i64::from(row.index), &structure)?;
                self.iri(&table_iri, "hasRow", &row_iri, &structure)?;
                for cell in &row.cells {
                    let cell_iri = Id::new(format!("{row_iri}/cell/{}", cell.column))?;
                    self.add_type(&cell_iri, "TableCell", &structure)?;
                    self.integer(&cell_iri, "rowIndex", i64::from(cell.row), &structure)?;
                    self.integer(&cell_iri, "columnIndex", i64::from(cell.column), &structure)?;
                    self.integer(&cell_iri, "rowSpan", i64::from(cell.row_span), &structure)?;
                    self.integer(
                        &cell_iri,
                        "columnSpan",
                        i64::from(cell.column_span),
                        &structure,
                    )?;
                    self.literal(&cell_iri, "cellText", &cell.text, &structure)?;
                    self.boolean(&cell_iri, "headerCell", cell.is_header, &structure)?;
                    self.iri(&row_iri, "hasCell", &cell_iri, &structure)?;
                }
            }
        }
        if let Some(formula) = &block.formula {
            self.map_formula(iri, formula)?;
        }
        if let Some(list) = &block.list {
            let list_iri = Id::new(format!("{iri}/list"))?;
            self.add_type(&list_iri, DOCO_LIST, &structure)?;
            self.boolean(&list_iri, "orderedList", list.ordered, &structure)?;
            self.iri(iri, "hasList", &list_iri, &structure)?;
            for (index, item) in list.items.iter().enumerate() {
                let item_iri = Id::new(format!("{list_iri}/item/{}", index + 1))?;
                self.add_type(&item_iri, DOCO_LIST_ITEM, &structure)?;
                self.literal(&item_iri, "text", item, &structure)?;
                self.integer(&item_iri, "listIndex", index as i64 + 1, &structure)?;
                self.iri(&list_iri, "hasListItem", &item_iri, &structure)?;
            }
        }
        Ok(())
    }

    fn map_formula(&mut self, block: &Id, formula: &Formula) -> Result<()> {
        let iri = Id::new(format!("{block}/formula"))?;
        let structure = self.graphs.structure.clone();
        self.add_type(&iri, "Formula", &structure)?;
        if let Some(latex) = &formula.latex {
            self.literal(&iri, "latex", latex, &structure)?;
        }
        if let Some(mathml) = &formula.mathml {
            self.literal(&iri, "mathml", mathml, &structure)?;
        }
        self.literal(
            &iri,
            "formulaType",
            match formula.formula_type {
                ontology_core::document::FormulaType::Inline => "inline",
                ontology_core::document::FormulaType::Interline => "interline",
            },
            &structure,
        )?;
        self.iri(block, "hasFormula", &iri, &structure)?;
        Ok(())
    }

    fn map_geometry(
        &mut self,
        subject: &Id,
        geometry: &Geometry,
        page: u32,
        index: u32,
    ) -> Result<()> {
        let layout = self.graphs.layout.clone();
        let iri = Id::new(format!("{subject}/geometry/{index}"))?;
        self.add_type(&iri, "BoundingBox", &layout)?;
        self.float(&iri, "x0", geometry.x0, &layout)?;
        self.float(&iri, "y0", geometry.y0, &layout)?;
        self.float(&iri, "x1", geometry.x1, &layout)?;
        self.float(&iri, "y1", geometry.y1, &layout)?;
        self.integer(&iri, "pageIndex", i64::from(page), &layout)?;
        if let Some(unit) = &geometry.unit {
            self.literal(&iri, "coordinateUnit", unit, &layout)?;
        }
        if let Some(origin) = &geometry.origin {
            self.literal(&iri, "coordinateOrigin", origin, &layout)?;
        }
        self.iri(subject, "bbox", &iri, &layout)?;
        Ok(())
    }

    fn map_sections(&mut self) -> Result<()> {
        for section in &self.document.sections {
            let iri = Id::new(section.id.clone())?;
            self.map_section(section, &iri)?;
        }
        Ok(())
    }

    fn map_section(&mut self, section: &Section, iri: &Id) -> Result<()> {
        let structure = self.graphs.structure.clone();
        self.add_type(iri, DOCO_SECTION, &structure)?;
        if let Some(title) = &section.title {
            self.literal(iri, "title", title, &structure)?;
        }
        self.integer(iri, "sectionLevel", i64::from(section.level), &structure)?;
        self.integer(iri, "sectionIndex", i64::from(section.order), &structure)?;
        if let Some(page) = section.page_start {
            self.integer(iri, "startsOnPageIndex", i64::from(page), &structure)?;
        }
        if let Some(parent) = &section.parent {
            let parent = Id::new(parent.clone())?;
            self.iri(iri, "parentSection", &parent, &structure)?;
        }
        for block in &section.blocks {
            let block = Id::new(block.clone())?;
            self.iri(iri, "hasBlock", &block, &structure)?;
        }
        self.iri(&self.doc.clone(), "hasSection", iri, &structure)?;
        Ok(())
    }

    fn map_references(&mut self) -> Result<()> {
        for reference in &self.document.references {
            let iri = Id::new(reference.id.clone())?;
            self.map_reference(reference, &iri)?;
        }
        Ok(())
    }

    fn map_reference(&mut self, reference: &Reference, iri: &Id) -> Result<()> {
        let bibliography = self.graphs.bibliography.clone();
        self.add_type(iri, DOCO_REFERENCE, &bibliography)?;
        self.literal(iri, "rawReferenceText", &reference.raw_text, &bibliography)?;
        self.integer(
            iri,
            "referenceIndex",
            i64::from(reference.index),
            &bibliography,
        )?;
        if let Some(resolved) = &reference.resolved {
            if let Some(title) = &resolved.title {
                self.literal(iri, "referenceTitle", title, &bibliography)?;
            }
            if let Some(year) = resolved.year {
                self.integer(iri, "publicationYear", i64::from(year), &bibliography)?;
            }
            if let Some(doi) = &resolved.doi {
                self.literal(iri, "doi", doi, &bibliography)?;
            }
            if let Some(url) = &resolved.url {
                self.literal(iri, "url", url, &bibliography)?;
            }
            for author in &resolved.authors {
                self.literal(iri, "referenceAuthor", author, &bibliography)?;
            }
        }
        self.iri(&self.doc.clone(), "hasReference", iri, &bibliography)?;
        Ok(())
    }

    fn map_citations(&mut self) -> Result<()> {
        for citation in &self.document.citations {
            let iri = Id::new(citation.id.clone())?;
            self.map_citation(citation, &iri)?;
        }
        Ok(())
    }

    fn map_citation(&mut self, citation: &Citation, iri: &Id) -> Result<()> {
        let bibliography = self.graphs.bibliography.clone();
        self.add_type(iri, "CitationOccurrence", &bibliography)?;
        self.literal(iri, "citationText", &citation.raw_text, &bibliography)?;
        self.integer(
            iri,
            "citationIndex",
            i64::from(citation.index),
            &bibliography,
        )?;
        if let Some(reference) = &citation.reference {
            let reference = Id::new(reference.clone())?;
            self.iri(iri, "citesReference", &reference, &bibliography)?;
        }
        if let Some(block) = &citation.block {
            let block = Id::new(block.clone())?;
            self.iri(iri, "appearsInParagraph", &block, &bibliography)?;
        }
        self.iri(&self.doc.clone(), "hasCitation", iri, &bibliography)?;
        Ok(())
    }

    fn map_provenance(&mut self) -> Result<()> {
        let provenance = self.document.provenance.clone();
        let activity = Id::new(format!(
            "urn:onto:extraction:{}",
            crate::ids::sha256_hex(
                format!(
                    "{}\u{1f}{}\u{1f}{}\u{1f}{}",
                    self.doc,
                    provenance.extracted_at.to_rfc3339(),
                    provenance.engine,
                    provenance.engine_version
                )
                .as_bytes()
            )
        ))?;
        let graph = self.graphs.provenance.clone();
        self.add_type(&activity, PROV_ACTIVITY, &graph)?;
        self.iri(&activity, "used", &self.doc.clone(), &graph)?;
        self.iri(&activity, "generated", &self.doc.clone(), &graph)?;
        self.predicated_literal(
            &activity,
            "http://www.w3.org/ns/prov#endedAtTime",
            Term::Literal(Literal::datetime(&provenance.extracted_at)),
            &graph,
        )?;

        let agent = Id::new(format!("{activity}/agent"))?;
        self.add_type(&agent, PROV_SOFTWARE_AGENT, &graph)?;
        self.predicated_literal(
            &agent,
            "https://schema.org/name",
            Term::string(provenance.engine.clone()),
            &graph,
        )?;
        self.iri(&activity, "wasAssociatedWith", &agent, &graph)?;
        self.literal(&agent, "version", &provenance.engine_version, &graph)?;
        self.literal(&agent, "shimVersion", &provenance.shim_version, &graph)?;
        self.literal(&agent, "backend", &provenance.backend, &graph)?;
        self.literal(&agent, "tier", &provenance.tier, &graph)?;
        self.literal(&agent, "inputSchema", &provenance.input_schema, &graph)?;
        self.literal(
            &agent,
            "inputSchemaVersion",
            &provenance.input_schema_version,
            &graph,
        )?;
        self.literal(&agent, "extractionMode", provenance.mode.as_str(), &graph)?;
        self.literal(
            &agent,
            "textFormat",
            provenance.text_format.as_str(),
            &graph,
        )?;
        self.integer(
            &agent,
            "durationMs",
            provenance.duration_ms.min(i64::MAX as u64) as i64,
            &graph,
        )?;
        self.iri(&self.doc.clone(), "wasGeneratedBy", &activity, &graph)?;
        Ok(())
    }

    fn add_type(&mut self, subject: &Id, class: &str, graph: &Id) -> Result<()> {
        let class = if class.starts_with("http://") || class.starts_with("https://") {
            Id::new(class)?
        } else {
            pred(class)?
        };
        self.push(subject, &Id::new(RDF_TYPE)?, Term::Iri(class), graph)
    }

    fn iri(&mut self, subject: &Id, name: &str, object: &Id, graph: &Id) -> Result<()> {
        self.push(subject, &pred(name)?, Term::Iri(object.clone()), graph)
    }

    fn literal(&mut self, subject: &Id, name: &str, value: &str, graph: &Id) -> Result<()> {
        self.push(subject, &pred(name)?, Term::string(value), graph)
    }

    fn predicated_literal(
        &mut self,
        subject: &Id,
        predicate: &str,
        object: Term,
        graph: &Id,
    ) -> Result<()> {
        self.push(subject, &Id::new(predicate)?, object, graph)
    }

    fn integer(&mut self, subject: &Id, name: &str, value: i64, graph: &Id) -> Result<()> {
        self.push(
            subject,
            &pred(name)?,
            Term::Literal(Literal::integer(value)),
            graph,
        )
    }

    fn boolean(&mut self, subject: &Id, name: &str, value: bool, graph: &Id) -> Result<()> {
        self.push(
            subject,
            &pred(name)?,
            Term::Literal(Literal::boolean(value)),
            graph,
        )
    }

    fn float(&mut self, subject: &Id, name: &str, value: f64, graph: &Id) -> Result<()> {
        let datatype = Id::new(XSD_DOUBLE)?;
        self.push(
            subject,
            &pred(name)?,
            Term::Literal(Literal::typed(value.to_string(), datatype)),
            graph,
        )
    }

    fn push(&mut self, subject: &Id, predicate: &Id, object: Term, graph: &Id) -> Result<()> {
        let quad = Quad::in_graph(
            Term::Iri(subject.clone()),
            predicate.clone(),
            object,
            GraphName::Iri(graph.clone()),
        );
        quad.validate()?;
        self.quads.push(quad);
        Ok(())
    }
}

fn block_class(kind: &BlockKind) -> &str {
    match kind {
        BlockKind::Title => "Title",
        BlockKind::Heading { .. } => DOCO_SECTION,
        BlockKind::Paragraph => DOCO_PARAGRAPH,
        BlockKind::List => DOCO_LIST,
        BlockKind::ListItem => DOCO_LIST_ITEM,
        BlockKind::Figure => DOCO_FIGURE,
        BlockKind::Table => DOCO_TABLE,
        BlockKind::Formula => "Formula",
        BlockKind::Footnote => DOCO_FOOTNOTE,
        BlockKind::PageHeader => "PageHeader",
        BlockKind::PageFooter => "PageFooter",
        BlockKind::Reference => DOCO_REFERENCE,
        BlockKind::Code => "Code",
        BlockKind::Unknown { .. } => "Block",
    }
}

fn pred(name: &str) -> Result<Id> {
    Id::new(format!("urn:onto:document:vocab#{name}"))
}
