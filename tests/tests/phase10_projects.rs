//! Phase 10: project work, deliverable acceptance, snapshots, and overview.

use std::sync::Arc;

use ontology_core::config::{ProjectConfig, RegistryConfig};
use ontology_core::error::Error;
use ontology_core::id::Id;
use ontology_core::project::{NewProject, SemanticAssetCatalog, TaskEvent, WorkKind};
use ontology_core::rdf::RdfStore;
use ontology_core::registry::IngestRequest;
use ontology_project::ProjectService;
use ontology_rdf::OxigraphStore;
use ontology_registry::RegistryService;

struct Fixture {
    _directory: tempfile::TempDir,
    projects: ProjectService,
    registry: Arc<RegistryService>,
}

fn fixture() -> Fixture {
    let directory = tempfile::tempdir().expect("temp dir");
    let store: Arc<dyn RdfStore> = Arc::new(OxigraphStore::in_memory().expect("store"));
    let registry = Arc::new(
        RegistryService::new(
            Arc::clone(&store),
            RegistryConfig {
                artifact_dir: directory.path().join("artifacts"),
                ..RegistryConfig::default()
            },
        )
        .expect("registry"),
    );
    let projects = ProjectService::new(
        store,
        ProjectConfig {
            base_iri: "urn:onto:test-project:".to_owned(),
        },
        registry.clone() as Arc<dyn SemanticAssetCatalog>,
    );
    Fixture {
        _directory: directory,
        projects,
        registry,
    }
}

async fn published_release(registry: &RegistryService) -> (Id, Id, String) {
    let data = include_str!("../fixtures/registry/planning-1.0.ttl");
    let candidate = registry
        .ingest(&IngestRequest {
            catalog: None,
            source: "planning.ttl".to_owned(),
            data: Some(data.to_owned()),
            asset_iri: Some(Id::new("https://example.org/planning").unwrap()),
            version: Some("1.0".to_owned()),
            format: Some("turtle".to_owned()),
            title: None,
        })
        .await
        .expect("ingest");
    registry.validate(&candidate.id, None).expect("validate");
    let release = registry.publish(&candidate.id, None).expect("publish");
    (candidate.asset, release.id, release.key)
}

#[test]
fn project_work_breakdown_and_transitions() {
    let fixture = fixture();
    let project = fixture
        .projects
        .create(&NewProject {
            title: "Planning project".to_owned(),
            description: Some("A project".to_owned()),
            owner: Some("team".to_owned()),
        })
        .expect("project");
    assert_eq!(
        fixture.projects.repository().list_projects().unwrap().len(),
        1
    );

    let goal = fixture
        .projects
        .add_goal(&project.id, "Ship planning", None)
        .unwrap();
    assert_eq!(goal.project, project.id);
    assert_eq!(
        fixture
            .projects
            .repository()
            .list_goals(&project.id)
            .unwrap()
            .len(),
        1
    );

    let requirement = fixture
        .projects
        .add_requirement(&project.id, "Support plans", None, Some("capability"))
        .unwrap();
    assert_eq!(requirement.kind, "capability");

    let work = fixture
        .projects
        .add_work(
            &project.id,
            None,
            WorkKind::Task,
            "Draft",
            None,
            Some("alice"),
            None,
        )
        .unwrap();
    assert_eq!(work.status, "planned");
    let work = fixture
        .projects
        .transition(&work.id, TaskEvent::Start)
        .unwrap();
    assert_eq!(work.status, "active");
    let work = fixture
        .projects
        .transition(&work.id, TaskEvent::Complete)
        .unwrap();
    assert_eq!(work.status, "done");
    assert_eq!(work.progress, 100);

    let other = fixture
        .projects
        .add_work(
            &project.id,
            None,
            WorkKind::Milestone,
            "Release",
            None,
            None,
            None,
        )
        .unwrap();
    let error = fixture
        .projects
        .transition(&other.id, TaskEvent::Complete)
        .expect_err("illegal transition");
    assert!(matches!(error, Error::Project { .. }));
}

#[tokio::test]
async fn deliverable_acceptance_requires_validated_release_evidence() {
    let fixture = fixture();
    let (asset, release, release_key) = published_release(&fixture.registry).await;
    let project = fixture
        .projects
        .create(&NewProject {
            title: "Acceptance project".to_owned(),
            description: None,
            owner: None,
        })
        .unwrap();

    let deliverable = fixture
        .projects
        .add_deliverable(
            &project.id,
            "Planning ontology",
            &["ontology".to_owned()],
            Some(&asset),
            Some(&release),
            &["validated".to_owned()],
        )
        .unwrap();

    let accepted = fixture
        .projects
        .accept(&deliverable.id, Some("publication"), Some("validated"))
        .await
        .expect("accept");
    assert_eq!(accepted.status, "accepted");
    assert!(accepted.accepted_at.is_some());
    assert!(accepted.evidence.iter().any(|value| value == "publication"));

    let snapshot = fixture.projects.snapshot(&project.id).expect("snapshot");
    assert_eq!(snapshot.releases, vec![release.clone()]);
    assert_eq!(snapshot.accepted_deliverables, vec![deliverable.id.clone()]);

    let overview = fixture.projects.overview(&project.id).expect("overview");
    assert_eq!(overview.accepted_deliverables, 1);
    assert_eq!(overview.deliverables, 1);
    assert_eq!(overview.releases, vec![release]);
    assert_eq!(
        fixture
            .projects
            .repository()
            .list_snapshots(&project.id)
            .unwrap()
            .len(),
        1
    );
    assert!(fixture
        .projects
        .link_deliverable(&deliverable.id, &release_key)
        .await
        .is_ok());
}

#[tokio::test]
async fn acceptance_rejects_unvalidated_releases() {
    let fixture = fixture();
    let data = include_str!("../fixtures/registry/planning-1.0.ttl");
    let candidate = fixture
        .registry
        .ingest(&IngestRequest {
            catalog: None,
            source: "planning.ttl".to_owned(),
            data: Some(data.to_owned()),
            asset_iri: Some(Id::new("https://example.org/planning").unwrap()),
            version: Some("1.0".to_owned()),
            format: Some("turtle".to_owned()),
            title: None,
        })
        .await
        .unwrap();
    let release = fixture.registry.publish(&candidate.id, None).unwrap();
    // Publishing runs validation; remove the linked evidence to model an
    // externally imported release without a report.
    let metadata = ontology_core::registry_graph::catalog_graph();
    let quads = fixture
        .registry
        .repository()
        .store()
        .get(
            ontology_core::model::QuadPattern::new()
                .with_graph(metadata.clone())
                .with_subject(ontology_core::model::Term::Iri(release.id.clone())),
        )
        .unwrap();
    let evidence: Vec<_> = quads
        .into_iter()
        .filter(|quad| quad.predicate.as_str() == "urn:onto:registry:hasValidationReport")
        .collect();
    assert!(
        !evidence.is_empty(),
        "the published release must link a report"
    );
    fixture
        .registry
        .repository()
        .store()
        .delete(&evidence)
        .unwrap();

    let project = fixture
        .projects
        .create(&NewProject {
            title: "No evidence".to_owned(),
            description: None,
            owner: None,
        })
        .unwrap();
    let deliverable = fixture
        .projects
        .add_deliverable(&project.id, "Unexpected", &[], None, Some(&release.id), &[])
        .unwrap();
    let error = fixture
        .projects
        .accept(&deliverable.id, None, None)
        .await
        .expect_err("unvalidated release");
    assert!(matches!(error, Error::Project { .. }));
}
