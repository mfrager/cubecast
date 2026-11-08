//! Phase 6 end-to-end tests: complete feature paths through the shared
//! application boundary and the HTTP router.

use std::fs;
use std::sync::Arc;
use std::time::Duration;

use axum::body::Body;
use axum::http::{header, Request, StatusCode};
use axum::Router;
use ontology_api::{router, AppState};
use ontology_application::ApplicationService;
use ontology_core::command::{
    Command, CommandResult, ExternalCommand, JobCommand, OntologyCommand, RdfCommand,
    SemanticCommand, TaxonomyCommand,
};
use ontology_core::external::{ClassMapping, ImportRequest, MappingKind, PropertyMapping};
use ontology_core::id::Id;
use ontology_core::job::{JobRequest, JobState, JobStatus};
use ontology_core::model::{NewInstance, QueryResult, Term};
use ontology_core::owl::{OwlClass, OwlProperty, PropertyKind};
use ontology_core::rdf::OntologyService;
use ontology_core::semantic::{
    ClassificationOptions, InferenceMethod, InferenceOptions, ReviewStatus,
};
use ontology_core::skos::{Concept, ConceptScheme};
use ontology_integration_tests::{ex, memory_store_handle};
use serde_json::{json, Value};
use tower::ServiceExt;

const EXTERNAL_CATALOG: &str = r#"
@prefix ex: <https://external.example.org/> .
@prefix owl: <http://www.w3.org/2002/07/owl#> .
@prefix rdfs: <http://www.w3.org/2000/01/rdf-schema#> .

<https://external.example.org/onto> a owl:Ontology ;
    rdfs:label "External Catalog" ;
    owl:versionInfo "2.1" .

ex:Book a owl:Class .
ex:hasAuthor a owl:ObjectProperty ;
    rdfs:domain ex:Book .
"#;

fn external(local: &str) -> Id {
    Id::new(format!("https://external.example.org/{local}")).expect("external IRI")
}

async fn wait_for_job(service: &ApplicationService, id: &Id) -> JobStatus {
    for _ in 0..400 {
        let status = service.jobs().get(id).await.expect("job status");
        if status.is_finished() {
            return status;
        }
        tokio::time::sleep(Duration::from_millis(5)).await;
    }
    panic!("job did not finish");
}

#[tokio::test]
async fn full_semantic_lifecycle_through_commands() {
    let service = ApplicationService::new(memory_store_handle());

    service
        .execute(Command::Ontology(OntologyCommand::CreateClass(
            OwlClass::new(ex("Equipment")).with_label("en", "Equipment"),
        )))
        .await
        .expect("equipment class");
    service
        .execute(Command::Ontology(OntologyCommand::CreateClass(
            OwlClass::new(ex("Hardware")).with_superclass(ex("Equipment")),
        )))
        .await
        .expect("hardware class");
    service
        .execute(Command::Ontology(OntologyCommand::CreateProperty(
            OwlProperty::new(ex("name"), PropertyKind::Datatype).with_domain(ex("Equipment")),
        )))
        .await
        .expect("name property");
    service
        .execute(Command::Taxonomy(TaxonomyCommand::CreateScheme(
            ConceptScheme::new(ex("categories"))
                .with_label("en", "Categories")
                .with_version("1.0"),
        )))
        .await
        .expect("scheme");
    service
        .execute(Command::Taxonomy(TaxonomyCommand::CreateConcept(
            Concept::new(ex("networking"))
                .with_scheme(ex("categories"))
                .with_pref_label("Networking Equipment", "en")
                .with_alt_label("network gear", "en")
                .with_alt_label("Network Router", "en"),
        )))
        .await
        .expect("concept");

    let router = NewInstance::new(ex("router"))
        .with_type(ex("Hardware"))
        .with_property(ex("name"), Term::string("Network Router"));
    let switch = NewInstance::new(ex("switch"))
        .with_type(ex("Hardware"))
        .with_property(ex("name"), Term::string("network gear"));
    let created = service
        .execute(Command::Rdf(RdfCommand::CreateInstance(router)))
        .await
        .expect("create router");
    assert!(matches!(created, CommandResult::Resource(ref value) if value.kind == "instance"));
    service
        .execute(Command::Rdf(RdfCommand::CreateInstance(switch)))
        .await
        .expect("create switch");

    let classified = service
        .execute(Command::Semantic(SemanticCommand::Classify {
            instance: ex("router"),
            options: ClassificationOptions::new()
                .with_scheme(ex("categories"))
                .with_threshold(0.5),
        }))
        .await
        .expect("classify");
    let classifications = match classified {
        CommandResult::Classifications(values) => values,
        other => panic!("unexpected classification result: {other:?}"),
    };
    let classification = classifications
        .iter()
        .find(|value| value.concept == ex("networking"))
        .expect("networking classification");
    assert_eq!(classification.status, ReviewStatus::Pending);

    let reviewed = service
        .execute(Command::Semantic(SemanticCommand::ReviewClassification {
            classification: classification.id.clone(),
            approved: true,
        }))
        .await
        .expect("approve classification");
    match reviewed {
        CommandResult::Classifications(values) => {
            assert_eq!(values[0].status, ReviewStatus::Approved);
        }
        other => panic!("unexpected review result: {other:?}"),
    }

    let preview = service
        .execute(Command::Semantic(SemanticCommand::PreviewInference {
            instance: ex("router"),
            options: InferenceOptions::new(),
        }))
        .await
        .expect("inference preview");
    match preview {
        CommandResult::Preview(preview) => {
            assert!(preview.classes.iter().any(|value| {
                value.class == ex("Equipment") && value.method == InferenceMethod::Subclass
            }));
            assert!(preview.classes.iter().any(|value| {
                value.class == ex("networking") && value.method == InferenceMethod::Classification
            }));
        }
        other => panic!("unexpected inference result: {other:?}"),
    }

    let applied = service
        .execute(Command::Semantic(SemanticCommand::ApplyInference {
            instance: ex("router"),
            options: InferenceOptions::new(),
            policy: ontology_core::semantic::ApplyPolicy::Automatic { threshold: 0.8 },
        }))
        .await
        .expect("apply inference");
    match applied {
        CommandResult::Inference(report) => assert!(report.applied),
        other => panic!("unexpected inference report: {other:?}"),
    }

    for instance in [ex("router"), ex("switch")] {
        service
            .execute(Command::Semantic(SemanticCommand::GenerateEmbeddings {
                instance,
                attributes: vec![ex("name")],
            }))
            .await
            .expect("generate embeddings");
    }
    let similar = service
        .execute(Command::Semantic(SemanticCommand::SearchSimilar {
            instance: ex("router"),
            limit: 10,
            filter: None,
        }))
        .await
        .expect("similarity");
    match similar {
        CommandResult::Similarity(hits) => {
            assert!(hits.iter().any(|hit| hit.instance == ex("switch")));
        }
        other => panic!("unexpected similarity result: {other:?}"),
    }

    let export = service
        .execute(Command::Job(JobCommand::Submit(JobRequest::RdfExport {
            format: "nquads".to_owned(),
            graph: None,
        })))
        .await
        .expect("submit export");
    let job = match export {
        CommandResult::Job(status) => status,
        other => panic!("unexpected job result: {other:?}"),
    };
    let finished = wait_for_job(&service, &job.id).await;
    assert_eq!(finished.state, JobState::Completed);
    assert!(finished
        .result
        .as_ref()
        .and_then(|value| value["data"].as_str())
        .is_some_and(|data| data.contains("router")));

    let query = service
        .execute(Command::Rdf(RdfCommand::Query {
            sparql: "ASK { GRAPH ?g { <https://example.org/router> <https://example.org/name> \"Network Router\" } }"
                .to_owned(),
        }))
        .await
        .expect("ask query");
    assert_eq!(query, CommandResult::Query(QueryResult::Boolean(true)));
}

#[tokio::test]
async fn imports_maps_and_infers_external_ontologies() {
    let directory = tempfile::tempdir().expect("temp dir");
    let path = directory.path().join("catalog.ttl");
    fs::write(&path, EXTERNAL_CATALOG).expect("write catalog");

    let service = ApplicationService::new(memory_store_handle());
    service
        .execute(Command::Ontology(OntologyCommand::CreateClass(
            OwlClass::new(ex("Book")),
        )))
        .await
        .expect("internal class");
    service
        .execute(Command::Ontology(OntologyCommand::CreateProperty(
            OwlProperty::new(ex("hasAuthor"), PropertyKind::Object),
        )))
        .await
        .expect("internal property");

    let imported = service
        .execute(Command::External(ExternalCommand::Import(
            ImportRequest::new(path.to_str().expect("path")).with_format("turtle"),
        )))
        .await
        .expect("import");
    match imported {
        CommandResult::Resource(resource) => {
            assert_eq!(resource.kind, "import_report");
            assert_eq!(resource.id, external("onto").as_str());
        }
        other => panic!("unexpected import result: {other:?}"),
    }

    service
        .execute(Command::External(ExternalCommand::MapClass(
            ClassMapping::new(external("Book"), ex("Book"), MappingKind::EquivalentClass)
                .expect("class mapping"),
        )))
        .await
        .expect("map class");
    service
        .execute(Command::External(ExternalCommand::MapProperty(
            PropertyMapping::new(
                external("hasAuthor"),
                ex("hasAuthor"),
                MappingKind::EquivalentProperty,
            )
            .expect("property mapping"),
        )))
        .await
        .expect("map property");

    let instance = NewInstance::new(ex("external-book"))
        .with_type(external("Book"))
        .with_property(external("hasAuthor"), Term::string("Ada"));
    service
        .execute(Command::Rdf(RdfCommand::CreateInstance(instance)))
        .await
        .expect("external instance");

    let preview = service
        .execute(Command::Semantic(SemanticCommand::PreviewInference {
            instance: ex("external-book"),
            options: InferenceOptions::new(),
        }))
        .await
        .expect("mapping inference");
    match preview {
        CommandResult::Preview(preview) => {
            assert!(preview.classes.iter().any(|value| {
                value.class == ex("Book") && value.method == InferenceMethod::Mapping
            }));
            assert!(preview.properties.iter().any(|value| {
                value.property == ex("hasAuthor")
                    && value.method == InferenceMethod::Mapping
                    && value.value == Term::string("Ada")
            }));
        }
        other => panic!("unexpected mapping preview: {other:?}"),
    }

    let validation = service
        .execute(Command::External(ExternalCommand::ValidateMappings))
        .await
        .expect("validate mappings");
    match validation {
        CommandResult::Mapping(report) => {
            assert_eq!(report.checked, 2);
            assert!(report.is_valid());
        }
        other => panic!("unexpected mapping report: {other:?}"),
    }
}

async fn call(app: &Router, request: Request<Body>) -> (StatusCode, Value) {
    let response = app.clone().oneshot(request).await.expect("router response");
    let status = response.status();
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("response body");
    (status, serde_json::from_slice(&body).expect("JSON body"))
}

fn json_request(method: &str, uri: &str, body: &Value) -> Request<Body> {
    Request::builder()
        .method(method)
        .uri(uri)
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(body.to_string()))
        .expect("request")
}

fn get(uri: &str) -> Request<Body> {
    Request::builder()
        .uri(uri)
        .body(Body::empty())
        .expect("request")
}

#[tokio::test]
async fn http_router_covers_schema_taxonomy_semantics_and_jobs() {
    let app = router(AppState::open(Arc::new(ApplicationService::new(
        memory_store_handle(),
    ))));

    let (status, value) = call(&app, get("/api/v1/system/health")).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(value["status"], "ok");

    let class = OwlClass::new(ex("Equipment")).with_label("en", "Equipment");
    let (status, value) = call(
        &app,
        json_request(
            "POST",
            "/api/v1/classes",
            &serde_json::to_value(class).unwrap(),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{value}");
    assert_eq!(value["kind"], "owl_class");

    let scheme = ConceptScheme::new(ex("categories")).with_label("en", "Categories");
    let (status, value) = call(
        &app,
        json_request(
            "POST",
            "/api/v1/schemes",
            &serde_json::to_value(scheme).unwrap(),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{value}");
    assert_eq!(value["kind"], "concept_scheme");

    let concept = Concept::new(ex("networking"))
        .with_scheme(ex("categories"))
        .with_pref_label("Networking Equipment", "en")
        .with_alt_label("network gear", "en");
    let (status, value) = call(
        &app,
        json_request(
            "POST",
            "/api/v1/concepts",
            &serde_json::to_value(concept).unwrap(),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{value}");
    assert_eq!(value["kind"], "concept");

    let instance = NewInstance::new(ex("http-router"))
        .with_type(ex("Equipment"))
        .with_property(ex("name"), Term::string("network gear"));
    let (status, value) = call(
        &app,
        json_request(
            "POST",
            "/api/v1/instances",
            &serde_json::to_value(instance).unwrap(),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{value}");
    assert_eq!(value["type"], "resource");

    let (status, value) = call(
        &app,
        json_request(
            "POST",
            "/api/v1/classifications/run",
            &json!({ "instance": ex("http-router") }),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{value}");
    assert_eq!(value["type"], "classifications");
    assert!(!value["items"]
        .as_array()
        .expect("classification items")
        .is_empty());

    let (status, value) = call(
        &app,
        json_request(
            "POST",
            "/api/v1/embeddings/generate",
            &json!({ "instance": ex("http-router"), "attributes": [ex("name")] }),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{value}");
    assert_eq!(value["type"], "embeddings");
    assert_eq!(value["items"].as_array().expect("embeddings").len(), 2);

    let (status, value) = call(
        &app,
        json_request(
            "POST",
            "/api/v1/inference/preview",
            &json!({ "instance": ex("http-router") }),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{value}");
    assert_eq!(value["type"], "preview");

    let (status, value) = call(
        &app,
        json_request("POST", "/api/v1/export", &json!({ "format": "nquads" })),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{value}");
    assert_eq!(value["type"], "export");
    assert!(value["data"]
        .as_str()
        .is_some_and(|data| data.contains("http-router")));

    let (status, value) = call(
        &app,
        json_request(
            "POST",
            "/api/v1/sparql/query",
            &json!({ "query": "ASK { GRAPH ?g { <https://example.org/http-router> ?p ?o } }" }),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{value}");
    assert_eq!(value["type"], "query");
    assert_eq!(value["resultType"], "boolean");
    assert_eq!(value["value"], true);
}
