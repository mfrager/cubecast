//! End-to-end tests for parsing, mode projections, caching, and RDF mapping.

use std::sync::Arc;

use ontology_core::config::ExtractionConfig;
use ontology_core::document::{
    DocumentExtractor, ExtractionContext, ExtractionMode, ExtractionOutput, ExtractionProvenance,
    ExtractionRequest, ExtractionWarning, TextFormat, TextOutput,
};
use ontology_core::error::{Error, Result};
use ontology_core::model::{GraphName, Quad, QuadPattern, QueryResult};
use ontology_core::rdf::RdfStore;
use ontology_document::{
    detect_schema, ids, map_document, parse_document, write_document, DocumentService,
    ParsedDocument,
};

const MIDDLE: &str = include_str!("fixtures/mineru/middle.json");

fn parse_fixture(mode: ExtractionMode, format: TextFormat) -> ParsedDocument {
    let value: serde_json::Value = serde_json::from_str(MIDDLE).expect("fixture json");
    let detected = detect_schema(&value).expect("schema");
    assert_eq!(detected.kind, ontology_document::SchemaKind::Middle);
    assert_eq!(detected.version, "2.0");
    let document_id = format!("urn:sha256:{}", "a".repeat(64));
    parse_document(
        &value,
        &detected,
        &ontology_document::parse::ParseInput {
            document_id: &document_id,
            source_name: Some("fixture.pdf"),
            source_bytes: 1,
            provenance: provenance(mode, format),
        },
    )
    .expect("parse")
}

fn provenance(mode: ExtractionMode, format: TextFormat) -> ExtractionProvenance {
    ExtractionProvenance {
        engine: "mineru".to_owned(),
        engine_version: "2.1.0".to_owned(),
        shim_version: "1".to_owned(),
        python_version: "3.12".to_owned(),
        backend: "pipeline".to_owned(),
        tier: "default".to_owned(),
        input_schema: "docvortex.middle".to_owned(),
        input_schema_version: "2.0".to_owned(),
        mode,
        text_format: format,
        source_sha256: format!("urn:sha256:{}", "a".repeat(64)),
        source_bytes: 1,
        extracted_at: chrono::Utc::now(),
        duration_ms: 1,
        cached: false,
    }
}

#[test]
fn fixture_parses_into_pages_blocks_and_sections() {
    let parsed = parse_fixture(ExtractionMode::Both, TextFormat::Markdown);
    let document = &parsed.document;

    assert_eq!(document.pages.len(), 2);
    assert_eq!(document.metadata.page_count, 2);
    assert_eq!(
        document.metadata.title.as_deref(),
        Some("A Study of Things")
    );
    assert_eq!(
        document.metadata.abstract_text.as_deref(),
        Some("We study things and report what we find.\nThe results are reproducible.")
    );

    let blocks: Vec<_> = document
        .pages
        .iter()
        .flat_map(|page| &page.blocks)
        .collect();
    assert_eq!(
        blocks.len(),
        14,
        "13 content blocks plus the discarded header"
    );
    assert_eq!(blocks[0].reading_order, 0);
    assert!(blocks
        .iter()
        .any(|block| block.kind == ontology_core::document::BlockKind::Figure));
    assert!(blocks
        .iter()
        .any(|block| block.kind == ontology_core::document::BlockKind::PageHeader));

    let figure = blocks
        .iter()
        .find(|block| block.kind == ontology_core::document::BlockKind::Figure)
        .unwrap();
    assert_eq!(
        figure.caption.as_ref().unwrap().text,
        "Figure 1: Overview of the method."
    );
    assert_eq!(figure.asset.as_ref().unwrap().path, "images/overview.png");
    assert_eq!(figure.lines.len(), 0);

    let table = blocks
        .iter()
        .find(|block| block.kind == ontology_core::document::BlockKind::Table)
        .unwrap();
    let table = table.table.as_ref().unwrap();
    assert_eq!(table.rows.len(), 3);
    assert_eq!(table.rows[0].cells[0].text, "Name");
    assert!(table.rows[0].cells[0].is_header);
    assert_eq!(table.rows[2].cells[1].text, "2");

    let formula = blocks
        .iter()
        .find(|block| block.kind == ontology_core::document::BlockKind::Formula)
        .unwrap();
    assert_eq!(
        formula.formula.as_ref().unwrap().latex.as_deref(),
        Some("E = mc^2")
    );

    assert_eq!(document.sections.len(), 5);
    assert_eq!(
        document.sections[0].title.as_deref(),
        Some("A Study of Things")
    );
    assert_eq!(document.sections[1].title.as_deref(), Some("Abstract"));
    assert_eq!(document.references.len(), 2);
    assert_eq!(
        document.references[0].raw_text,
        "[1] A. Author. On things. Journal of Tests, 2020."
    );
    assert_eq!(document.citations.len(), 1);
    assert_eq!(document.citations[0].raw_text, "[1]");
    assert_eq!(
        document.citations[0].reference.as_ref().unwrap(),
        &document.references[0].id
    );
}

#[test]
fn structured_and_text_projections_use_the_requested_mode() {
    let parsed = parse_fixture(ExtractionMode::Text, TextFormat::Markdown);
    let markdown = ontology_document::text::render_markdown(&parsed.document);
    assert!(markdown.contains("# A Study of Things"));
    assert!(markdown.contains("# 1 Introduction"));
    assert!(!markdown.contains("Figure 1"));
    assert!(
        !markdown.contains("Journal of Tests"),
        "running header is not text"
    );

    let plain = ontology_document::text::to_plain(&markdown);
    assert!(!plain.contains('#'));
    assert!(plain.contains("A Study of Things"));
}

#[test]
fn output_validation_enforces_the_mode() {
    let parsed = parse_fixture(ExtractionMode::Structured, TextFormat::Markdown);
    let base = ExtractionOutput {
        document_id: parsed.document.document_id.clone(),
        run_id: "urn:onto:extraction:test".to_owned(),
        mode: ExtractionMode::Structured,
        document: Some(parsed.document.clone()),
        text: None,
        artefacts: Default::default(),
        provenance: provenance(ExtractionMode::Structured, TextFormat::Markdown),
        warnings: Vec::new(),
    };
    assert!(base.validate().is_ok());

    let mut wrong = base.clone();
    wrong.text = Some(TextOutput::new("text", TextFormat::Markdown));
    assert!(matches!(wrong.validate(), Err(Error::Extraction { .. })));

    let mut missing = base;
    missing.document = None;
    assert!(missing.validate().is_err());
}

#[test]
fn rdf_mapping_is_deterministic_and_covers_every_block() {
    let parsed = parse_fixture(ExtractionMode::Both, TextFormat::Markdown);
    let first = map_document(&parsed.document).expect("map");
    let second = map_document(&parsed.document).expect("map");
    assert_eq!(first, second);
    assert!(first.len() > 100);

    let graphs: std::collections::BTreeSet<_> =
        first.iter().map(|quad| quad.graph.to_string()).collect();
    assert_eq!(graphs.len(), 4);

    let has_doc_type = first.iter().any(|quad| {
        quad.predicate.as_str() == "http://www.w3.org/1999/02/22-rdf-syntax-ns#type"
            && quad.object.to_string().contains("AcademicArticle")
    });
    assert!(has_doc_type);

    let has_table_cell = first.iter().any(|quad| {
        quad.predicate.as_str() == "urn:onto:document:vocab#cellText"
            && quad.object.to_string().contains("alpha")
    });
    assert!(has_table_cell);

    let store = TestStore::default();
    let report = write_document(&store, &parsed.document).expect("write");
    assert_eq!(report.quads, first.len());
    let after_first = store.len();
    write_document(&store, &parsed.document).expect("rewrite");
    assert_eq!(
        store.len(),
        after_first,
        "rewriting must not duplicate quads"
    );
}

#[test]
fn service_persists_and_reuses_a_cached_run() {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("runtime");
    runtime.block_on(async {
        let dir = tempfile::tempdir().expect("tempdir");
        let source = dir.path().join("paper.pdf");
        std::fs::write(&source, b"%PDF-1.4 fixture").expect("write source");

        let extractor = Arc::new(FixtureExtractor {
            middle: MIDDLE.to_owned(),
            calls: std::sync::atomic::AtomicUsize::new(0),
        });
        let mut config = ExtractionConfig {
            enabled: true,
            mode: ExtractionMode::Both,
            text_format: TextFormat::Markdown,
            output_dir: dir.path().join("extractions"),
            ..ExtractionConfig::default()
        };
        config.max_input_bytes = 1024 * 1024;
        let service = DocumentService::new(extractor.clone(), config).expect("service");

        let first = service
            .extract(ExtractionRequest::new(&source))
            .await
            .expect("extract");
        assert!(!first.provenance.cached);
        assert!(first.document.is_some());
        assert!(first.text.is_some());
        assert!(first.artefacts.run_dir.exists());
        assert!(first.artefacts.structured_json.as_ref().unwrap().exists());

        let second = service
            .extract(ExtractionRequest::new(&source))
            .await
            .expect("cached extract");
        assert!(second.provenance.cached);
        assert_eq!(second.run_id, first.run_id);
        assert_eq!(
            extractor.calls.load(std::sync::atomic::Ordering::SeqCst),
            1,
            "cache hit must not invoke the extractor"
        );

        let stored = service
            .get(&first.document_id)
            .await
            .expect("load stored extraction");
        assert_eq!(stored.document.unwrap().pages.len(), 2);

        let forced = service
            .extract(ExtractionRequest::new(&source).force())
            .await
            .expect("forced extract");
        assert!(!forced.provenance.cached);
        assert_eq!(extractor.calls.load(std::sync::atomic::Ordering::SeqCst), 2);

        let text_only = service
            .extract(ExtractionRequest::new(&source).with_mode(ExtractionMode::Text))
            .await
            .expect("text extraction");
        assert!(text_only.document.is_none());
        let text = text_only.text.expect("text");
        assert!(text.content.contains("A Study of Things"));
        assert_eq!(
            text.content,
            first.text.as_ref().expect("both-mode text").content,
            "both mode must return the same text block as text mode"
        );
    });
}

#[test]
fn service_rejects_non_pdf_sources() {
    let dir = tempfile::tempdir().expect("tempdir");
    let source = dir.path().join("notes.txt");
    std::fs::write(&source, b"hello").expect("write");
    let service = DocumentService::new(
        Arc::new(FixtureExtractor {
            middle: MIDDLE.to_owned(),
            calls: std::sync::atomic::AtomicUsize::new(0),
        }),
        ExtractionConfig {
            enabled: true,
            output_dir: dir.path().join("extractions"),
            ..ExtractionConfig::default()
        },
    )
    .expect("service");
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("runtime");
    let error = runtime
        .block_on(service.extract(ExtractionRequest::new(&source)))
        .expect_err("unsupported extension");
    assert!(matches!(error, Error::Invalid { .. }));
}

struct FixtureExtractor {
    middle: String,
    calls: std::sync::atomic::AtomicUsize,
}

impl DocumentExtractor for FixtureExtractor {
    fn extract(&self, context: &ExtractionContext) -> Result<ExtractionOutput> {
        self.calls.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        let value: serde_json::Value =
            serde_json::from_str(&self.middle).map_err(Error::serialization)?;
        let detected = detect_schema(&value)?;
        let provenance = provenance(context.mode, context.text_format);
        let run_id = ids::run_id(
            &context.document_id,
            &provenance.engine,
            &provenance.engine_version,
            &provenance.shim_version,
            &provenance.backend,
            &provenance.tier,
            &provenance.input_schema,
            &provenance.input_schema_version,
            context.mode,
            context.text_format,
        );
        let run_dir = ids::run_directory(&context.output_dir, &context.document_id, &run_id)?;
        std::fs::create_dir_all(&run_dir).map_err(|error| Error::Io {
            detail: error.to_string(),
        })?;
        std::fs::write(run_dir.join("middle.json"), self.middle.as_bytes()).map_err(|error| {
            Error::Io {
                detail: error.to_string(),
            }
        })?;

        let parsed = parse_document(
            &value,
            &detected,
            &ontology_document::parse::ParseInput {
                document_id: &context.document_id,
                source_name: context.source.file_name().and_then(|name| name.to_str()),
                source_bytes: 1,
                provenance: provenance.clone(),
            },
        )?;

        let document = if context.mode.wants_document() {
            Some(parsed.document.clone())
        } else {
            None
        };
        let text = if context.mode.wants_text() {
            let markdown = ontology_document::text::render_markdown(&parsed.document);
            Some(TextOutput::new(
                match context.text_format {
                    TextFormat::Markdown => markdown,
                    TextFormat::Plain => ontology_document::text::to_plain(&markdown),
                },
                context.text_format,
            ))
        } else {
            None
        };

        Ok(ExtractionOutput {
            document_id: context.document_id.clone(),
            run_id,
            mode: context.mode,
            document,
            text,
            artefacts: ontology_core::document::ArtefactSet {
                run_dir,
                middle_json: None,
                markdown: None,
                structured_json: None,
                text: None,
                assets: Vec::new(),
            },
            provenance,
            warnings: Vec::<ExtractionWarning>::new(),
        })
    }
}

trait ForceExtraction {
    fn force(self) -> Self;
}

impl ForceExtraction for ExtractionRequest {
    fn force(mut self) -> Self {
        self.force = true;
        self
    }
}

#[derive(Default)]
struct TestStore {
    quads: std::sync::Mutex<Vec<Quad>>,
}

impl TestStore {
    fn len(&self) -> usize {
        self.quads.lock().expect("lock").len()
    }
}

impl RdfStore for TestStore {
    fn get(&self, pattern: QuadPattern) -> Result<Vec<Quad>> {
        let quads = self.quads.lock().map_err(|_| Error::storage("poisoned"))?;
        Ok(quads
            .iter()
            .filter(|quad| {
                pattern
                    .subject
                    .as_ref()
                    .is_none_or(|subject| subject == &quad.subject)
                    && pattern
                        .predicate
                        .as_ref()
                        .is_none_or(|predicate| predicate == &quad.predicate)
                    && pattern
                        .object
                        .as_ref()
                        .is_none_or(|object| object == &quad.object)
                    && pattern
                        .graph
                        .as_ref()
                        .is_none_or(|graph| graph == &quad.graph)
            })
            .cloned()
            .collect())
    }

    fn insert(&self, quads: &[Quad]) -> Result<()> {
        let mut existing = self.quads.lock().map_err(|_| Error::storage("poisoned"))?;
        for quad in quads {
            if !existing.contains(quad) {
                existing.push(quad.clone());
            }
        }
        Ok(())
    }

    fn delete(&self, quads: &[Quad]) -> Result<()> {
        let mut existing = self.quads.lock().map_err(|_| Error::storage("poisoned"))?;
        existing.retain(|quad| !quads.contains(quad));
        Ok(())
    }

    fn query(&self, _sparql: &str) -> Result<QueryResult> {
        Err(Error::Unsupported {
            operation: "query".to_owned(),
        })
    }

    fn update(&self, _sparql: &str) -> Result<()> {
        Err(Error::Unsupported {
            operation: "update".to_owned(),
        })
    }

    fn graphs(&self) -> Result<Vec<GraphName>> {
        let quads = self.quads.lock().map_err(|_| Error::storage("poisoned"))?;
        let mut graphs: Vec<GraphName> = quads.iter().map(|quad| quad.graph.clone()).collect();
        graphs.sort();
        graphs.dedup();
        Ok(graphs)
    }
}
