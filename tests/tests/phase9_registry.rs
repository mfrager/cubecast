//! Phase 9: registry ingestion, analysis, validation, publication, and search.

use std::sync::Arc;

use ontology_core::config::RegistryConfig;
use ontology_core::error::Error;
use ontology_core::id::Id;
use ontology_core::model::QuadPattern;
use ontology_core::rdf::RdfStore;
use ontology_core::registry::{IngestRequest, SearchKind};
use ontology_core::registry_graph::release_graph;
use ontology_rdf::OxigraphStore;
use ontology_registry::{artifact::ArtifactStore, FilesystemArtifactStore, RegistryService};

struct Registry {
    _directory: tempfile::TempDir,
    store: Arc<dyn RdfStore>,
    service: RegistryService,
}

fn registry() -> Registry {
    let directory = tempfile::tempdir().expect("temp dir");
    let store: Arc<dyn RdfStore> = Arc::new(OxigraphStore::in_memory().expect("store"));
    let config = RegistryConfig {
        artifact_dir: directory.path().join("artifacts"),
        ..RegistryConfig::default()
    };
    let service = RegistryService::new(Arc::clone(&store), config).expect("registry service");
    Registry {
        _directory: directory,
        store,
        service,
    }
}

fn request(data: &str, version: &str) -> IngestRequest {
    IngestRequest {
        catalog: None,
        source: "https://example.org/planning.ttl".to_owned(),
        data: Some(data.to_owned()),
        asset_iri: None,
        version: Some(version.to_owned()),
        format: Some("turtle".to_owned()),
        title: None,
    }
}

fn planning_10() -> &'static str {
    include_str!("../fixtures/registry/planning-1.0.ttl")
}

fn planning_11() -> &'static str {
    include_str!("../fixtures/registry/planning-1.1.ttl")
}

#[test]
fn catalogs_and_assets_are_persisted_in_the_catalog_graph() {
    let registry = registry();
    let catalog = registry
        .service
        .create_catalog("Test catalog", Some("Catalog for tests"))
        .expect("catalog");
    assert_eq!(catalog.title, "Test catalog");
    assert_eq!(registry.service.list_catalogs().expect("catalogs").len(), 1);

    let asset = registry
        .service
        .register_asset(
            Some(&catalog.id),
            &Id::new("https://example.org/planning").unwrap(),
            Some("Planning"),
            Some("Planning family"),
            Some("https://example.org/"),
        )
        .expect("asset");
    assert_eq!(asset.catalog, catalog.id);
    assert_eq!(
        registry
            .service
            .list_assets(Some(&catalog.id))
            .unwrap()
            .len(),
        1
    );
    assert_eq!(
        registry.service.get_asset(&asset.key).expect("by key").id,
        asset.id
    );
}

#[tokio::test]
async fn ingest_analyze_validate_and_publish() {
    let registry = registry();
    let candidate = registry
        .service
        .ingest(&request(planning_10(), "1.0"))
        .await
        .expect("ingest");
    assert_eq!(candidate.version, "1.0");
    assert_eq!(candidate.statistics.classes, 1);
    assert_eq!(candidate.statistics.object_properties, 1);
    assert_eq!(candidate.statistics.datatype_properties, 1);
    assert_eq!(candidate.statistics.terms, 3);
    assert_eq!(candidate.imports.len(), 1);
    assert!(!candidate.imports[0].resolved);

    let candidate = registry.service.analyze(&candidate.id).expect("analyze");
    assert_eq!(candidate.statistics.terms, 3);

    let report = registry
        .service
        .validate(&candidate.id, None)
        .expect("validate");
    assert!(report.conforms);
    assert!(!report.external_shacl);
    assert_eq!(report.issues.len(), 1);
    assert_eq!(report.issues[0].severity, "warning");

    let release = registry
        .service
        .publish(&candidate.id, Some("stable"))
        .expect("publish");
    assert_eq!(release.version, "1.0");
    assert_eq!(release.channel, "stable");
    assert_eq!(release.distributions.len(), 5);
    assert!(!release.distributions[0].sha256.is_empty());

    let terms = registry
        .service
        .release_terms(&release.key, Some("Plan"), 100, 0)
        .expect("terms");
    assert_eq!(terms.2, 1);
    assert_eq!(terms.1[0].label.as_deref(), Some("Plan"));

    let statistics = registry.service.statistics(&release.key).unwrap();
    assert_eq!(statistics.1.quads, 13);

    let reports = registry.service.reports(&release.key).expect("reports");
    assert!(reports.iter().any(|report| report.conforms));

    let (_, bytes) = registry.service.download(&release.key, "turtle").unwrap();
    assert!(String::from_utf8(bytes).unwrap().contains("Planning"));

    let graph = release_graph(&release.key).unwrap();
    let release_quads = registry
        .store
        .get(QuadPattern::new().with_graph(graph))
        .unwrap();
    assert_eq!(release_quads.len(), 13);

    let search = registry
        .service
        .search("Milestone", Some("all"), 100, 0)
        .unwrap();
    assert!(search.items.is_empty());
    let search = registry
        .service
        .search("Planning", Some("all"), 100, 0)
        .unwrap();
    assert!(!search.items.is_empty());
    assert!(SearchKind::parse(Some("all")).is_some());
}

#[tokio::test]
async fn repeated_ingest_is_idempotent_and_mutated_versions_conflict() {
    let registry = registry();
    let first = registry
        .service
        .ingest(&request(planning_10(), "1.0"))
        .await
        .expect("first ingest");
    let second = registry
        .service
        .ingest(&request(planning_10(), "1.0"))
        .await
        .expect("second ingest");
    assert_eq!(first.id, second.id);
    assert_eq!(first.release_key, second.release_key);
    assert_eq!(second.status, "ready");
    assert_eq!(registry.service.list_candidates(None).unwrap().len(), 1);

    let error = registry
        .service
        .ingest(&request(planning_11(), "1.0"))
        .await
        .expect_err("different bytes for the same version");
    assert!(matches!(error, Error::Conflict { .. }));
}

#[tokio::test]
async fn publishing_the_same_candidate_twice_is_idempotent() {
    let registry = registry();
    let candidate = registry
        .service
        .ingest(&request(planning_10(), "1.0"))
        .await
        .unwrap();
    let first = registry.service.publish(&candidate.id, None).unwrap();
    let second = registry.service.publish(&candidate.id, None).unwrap();
    assert_eq!(first.id, second.id);
    assert_eq!(first.source_hash, second.source_hash);
    assert_eq!(registry.service.list_releases(None).unwrap().len(), 1);
    let graph = release_graph(&first.key).unwrap();
    assert_eq!(
        registry
            .store
            .get(QuadPattern::new().with_graph(graph))
            .unwrap()
            .len(),
        13
    );
}

#[tokio::test]
async fn diff_reports_breaking_removals_and_additions() {
    let registry = registry();
    let one = registry
        .service
        .ingest(&request(planning_10(), "1.0"))
        .await
        .unwrap();
    let release_one = registry.service.publish(&one.id, None).unwrap();
    let two = registry
        .service
        .ingest(&request(planning_11(), "1.1"))
        .await
        .unwrap();
    let release_two = registry.service.publish(&two.id, None).unwrap();
    let diff = registry
        .service
        .diff(&release_one.key, &release_two.key)
        .unwrap();
    assert!(diff.breaking);
    assert!(!diff.compatible);
    assert!(diff
        .added_classes
        .iter()
        .any(|iri| iri.as_str() == "https://example.org/Milestone"));
    assert!(diff
        .removed_properties
        .iter()
        .any(|iri| iri.as_str() == "https://example.org/stepCount"));
    assert!(!diff.changed_terms.is_empty());
}

#[tokio::test]
async fn archive_keeps_release_content_and_records_channel() {
    let registry = registry();
    let candidate = registry
        .service
        .ingest(&request(planning_10(), "1.0"))
        .await
        .unwrap();
    let release = registry.service.publish(&candidate.id, None).unwrap();
    registry.service.archive(&release.key, None).unwrap();
    let archived = registry.service.get_release(&release.key).unwrap();
    assert_eq!(archived.channel, "archived");
    let graph = release_graph(&release.key).unwrap();
    assert_eq!(
        registry
            .store
            .get(QuadPattern::new().with_graph(graph))
            .unwrap()
            .len(),
        13
    );
}

#[test]
fn artifact_store_is_content_addressed() {
    let directory = tempfile::tempdir().unwrap();
    let store = FilesystemArtifactStore::new(directory.path()).unwrap();
    let first = store.put(b"abc", Some("a.ttl")).unwrap();
    let second = store.put(b"abc", Some("b.ttl")).unwrap();
    assert_eq!(first.sha256, second.sha256);
    assert_eq!(store.read(&first.sha256).unwrap(), b"abc");
}
