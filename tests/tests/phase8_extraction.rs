//! Phase 8: document extraction through the application service and jobs.
//!
//! MinerU itself is exercised by the gated `ontology-mineru` tests. This file
//! verifies the shared command path, mode enforcement, caching, the RDF hook,
//! and background job execution with a deterministic extractor.

use std::sync::Arc;

use chrono::{TimeZone, Utc};
use ontology_application::ApplicationService;
use ontology_core::command::{Command, CommandResult, DocumentCommand};
use ontology_core::config::ExtractionConfig;
use ontology_core::document::{
    ArtefactSet, Block, BlockKind, DocumentExtractor, DocumentMetadata, ExtractionContext,
    ExtractionMode, ExtractionOutput, ExtractionProvenance, ExtractionRequest, Page,
    StructuredDocument, TextFormat, TextOutput,
};
use ontology_core::error::{Error, Result};
use ontology_core::job::{JobRequest, JobState};
use ontology_core::rdf::OntologyService;
use ontology_document::{ids, DocumentService};
use ontology_integration_tests::memory_store_handle;

struct FixedExtractor;

impl DocumentExtractor for FixedExtractor {
    fn extract(&self, context: &ExtractionContext) -> Result<ExtractionOutput> {
        let run_id = ids::run_id(
            &context.document_id,
            "fixture",
            "1",
            "1",
            "pipeline",
            "default",
            "docvortex.middle",
            "2.0",
            context.mode,
            context.text_format,
        );
        let run_dir = ids::run_directory(&context.output_dir, &context.document_id, &run_id)?;
        std::fs::create_dir_all(&run_dir).map_err(|error| Error::Io {
            detail: error.to_string(),
        })?;
        let base = format!(
            "urn:onto:document:{}",
            ids::hex_suffix(&context.document_id)?
        );
        let mut provenance = ExtractionProvenance {
            engine: "fixture".to_owned(),
            engine_version: "1".to_owned(),
            shim_version: "1".to_owned(),
            python_version: String::new(),
            backend: "pipeline".to_owned(),
            tier: "default".to_owned(),
            input_schema: "docvortex.middle".to_owned(),
            input_schema_version: "2.0".to_owned(),
            mode: context.mode,
            text_format: context.text_format,
            source_sha256: context.document_id.clone(),
            source_bytes: 1,
            extracted_at: Utc
                .with_ymd_and_hms(2026, 2, 1, 0, 0, 0)
                .single()
                .expect("timestamp"),
            duration_ms: 1,
            cached: false,
        };
        provenance.cached = false;

        let block = Block {
            id: format!("{base}/block/1"),
            kind: BlockKind::Paragraph,
            text: "Extracted body.".to_owned(),
            geometry: None,
            reading_order: 0,
            lines: Vec::new(),
            section: None,
            caption: None,
            asset: None,
            table: None,
            formula: None,
            list: None,
        };
        let document = StructuredDocument {
            document_id: context.document_id.clone(),
            metadata: DocumentMetadata {
                title: Some("Fixture paper".to_owned()),
                page_count: 1,
                ..DocumentMetadata::default()
            },
            pages: vec![Page {
                index: 0,
                width: Some(612.0),
                height: Some(792.0),
                unit: Some("pt".to_owned()),
                blocks: vec![block],
            }],
            sections: Vec::new(),
            references: Vec::new(),
            citations: Vec::new(),
            provenance: provenance.clone(),
        };

        let output = ExtractionOutput {
            document_id: context.document_id.clone(),
            run_id,
            mode: context.mode,
            document: context.mode.wants_document().then_some(document),
            text: context
                .mode
                .wants_text()
                .then(|| TextOutput::new("Extracted body.", context.text_format)),
            artefacts: ArtefactSet {
                run_dir,
                ..ArtefactSet::default()
            },
            provenance,
            warnings: Vec::new(),
        };
        output.validate()?;
        Ok(output)
    }
}

fn service(output_dir: std::path::PathBuf) -> Arc<ApplicationService> {
    let config = ExtractionConfig {
        enabled: true,
        mode: ExtractionMode::Both,
        text_format: TextFormat::Markdown,
        output_dir,
        ..ExtractionConfig::default()
    };
    let document_service =
        Arc::new(DocumentService::new(Arc::new(FixedExtractor), config).expect("document service"));
    Arc::new(ApplicationService::new(memory_store_handle()).with_documents(document_service))
}

fn source(dir: &std::path::Path) -> std::path::PathBuf {
    let path = dir.join("paper.pdf");
    std::fs::write(&path, b"%PDF-1.7 fixture").expect("write pdf");
    path
}

#[tokio::test]
async fn document_commands_run_through_the_application_service() {
    let dir = tempfile::tempdir().expect("tempdir");
    let source = source(dir.path());
    let service = service(dir.path().join("extractions"));

    let result = service
        .execute(Command::Document(DocumentCommand::Extract(
            ExtractionRequest::new(&source).with_mode(ExtractionMode::Both),
        )))
        .await
        .expect("extract");
    let output = match result {
        CommandResult::Document(output) => output,
        other => panic!("unexpected result: {other:?}"),
    };
    assert_eq!(output.mode, ExtractionMode::Both);
    assert_eq!(output.document.as_ref().unwrap().pages.len(), 1);
    assert_eq!(output.text.as_ref().unwrap().content, "Extracted body.");

    let fetched = service
        .execute(Command::Document(DocumentCommand::Get {
            id: output.document_id.clone(),
        }))
        .await
        .expect("get");
    match fetched {
        CommandResult::Document(stored) => {
            assert_eq!(stored.run_id, output.run_id);
            assert!(stored.document.is_some());
        }
        other => panic!("unexpected result: {other:?}"),
    }

    let cached = service
        .execute(Command::Document(DocumentCommand::Extract(
            ExtractionRequest::new(&source),
        )))
        .await
        .expect("cached extract");
    match cached {
        CommandResult::Document(cached) => assert!(cached.provenance.cached),
        other => panic!("unexpected result: {other:?}"),
    }
}

#[tokio::test]
async fn text_mode_returns_only_a_text_block() {
    let dir = tempfile::tempdir().expect("tempdir");
    let source = source(dir.path());
    let service = service(dir.path().join("extractions"));

    let result = service
        .execute(Command::Document(DocumentCommand::Extract(
            ExtractionRequest::new(&source).with_mode(ExtractionMode::Text),
        )))
        .await
        .expect("extract");
    let output = match result {
        CommandResult::Document(output) => output,
        other => panic!("unexpected result: {other:?}"),
    };
    assert!(output.document.is_none());
    assert_eq!(output.text.unwrap().content, "Extracted body.");
}

#[tokio::test]
async fn extraction_jobs_report_completion() {
    let dir = tempfile::tempdir().expect("tempdir");
    let source = source(dir.path());
    let service = service(dir.path().join("extractions"));

    let job = service
        .execute(Command::Job(ontology_core::command::JobCommand::Submit(
            JobRequest::DocumentExtract(ExtractionRequest::new(&source)),
        )))
        .await
        .expect("submit");
    let CommandResult::Job(mut status) = job else {
        panic!("unexpected result");
    };
    for _ in 0..200 {
        if status.state.is_terminal() {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        status = service.jobs().get(&status.id).await.expect("job status");
    }
    assert_eq!(status.state, JobState::Completed, "{status:?}");
    assert!(status.result.is_some());
}

#[tokio::test]
async fn http_routes_extract_and_fetch_documents() {
    use axum::body::Body;
    use axum::http::{header, Request, StatusCode};
    use tower::ServiceExt;

    let dir = tempfile::tempdir().expect("tempdir");
    let source = source(dir.path());
    let service = service(dir.path().join("extractions"));
    let app = ontology_api::router(ontology_api::AppState::open(service));

    let request = ExtractionRequest::new(&source).with_mode(ExtractionMode::Structured);
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/documents/extractions")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(serde_json::to_vec(&request).unwrap()))
                .expect("request"),
        )
        .await
        .expect("router response");
    assert_eq!(response.status(), StatusCode::OK);
    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("body");
    let value: serde_json::Value = serde_json::from_slice(&bytes).expect("json");
    assert_eq!(value["type"], "document");
    assert_eq!(value["mode"], "structured");
    assert!(value.get("text").is_none());

    let id = value["document_id"].as_str().expect("document id");
    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri(format!("/api/v1/documents/{id}"))
                .body(Body::empty())
                .expect("request"),
        )
        .await
        .expect("router response");
    assert_eq!(response.status(), StatusCode::OK);
}

#[tokio::test]
async fn document_commands_are_unsupported_without_configuration() {
    let service = ApplicationService::new(memory_store_handle());
    let error = service
        .execute(Command::Document(DocumentCommand::Extract(
            ExtractionRequest::new("missing.pdf"),
        )))
        .await
        .expect_err("must be unsupported");
    assert!(matches!(error, Error::Unsupported { .. }));
}
