//! Axum router for the v1 HTTP API.
//!
//! Handlers only translate transport input into [`Command`] values and return
//! the resulting [`CommandResult`] as JSON, so the API shares exactly the same
//! behaviour as the CLI.

use axum::body::Body;
use axum::extract::{Path, Query, Request, State};
use axum::http::header::{AUTHORIZATION, CONTENT_DISPOSITION, CONTENT_TYPE, RETRY_AFTER};
use axum::http::StatusCode;
use axum::middleware::{self, Next};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use ontology_core::command::{
    Command, CommandResult, DocumentCommand, ExternalCommand, JobCommand, OntologyCommand,
    ProjectCommand, RdfCommand, RegistryCommand, SemanticCommand, TaxonomyCommand,
};
use ontology_core::document::ExtractionRequest;
use ontology_core::error::Error;
use ontology_core::external::{ClassMapping, ImportRequest, MappingQuery, PropertyMapping};
use ontology_core::id::Id;
use ontology_core::job::JobRequest;
use ontology_core::model::{
    GraphName, InstancePatch, InstanceQuery, NewInstance, StatementId, StatementInput,
};
use ontology_core::owl::{
    ClassQuery, GraphScope, Ontology, OwlClass, OwlProperty, PropertyKind, PropertyQuery,
};
use ontology_core::project::{NewProject, TaskEvent, WorkKind};
use ontology_core::rdf::OntologyService;
use ontology_core::registry::IngestRequest;
use ontology_core::semantic::{ApplyPolicy, ClassificationOptions, InferenceOptions, VectorFilter};
use ontology_core::skos::{Concept, ConceptCollection, ConceptScheme, SkosRelation, TaxonomyQuery};
use serde::Deserialize;
use serde_json::{json, Value};

use crate::error::ApiError;
use crate::openapi;
use crate::state::AppState;

type ApiResult = std::result::Result<Json<CommandResult>, ApiError>;

/// Builds the complete API router.
#[must_use]
pub fn router(state: AppState) -> Router {
    let protected = Router::new()
        .route("/api/v1/system/status", get(status))
        .route("/api/v1/system/metrics", get(metrics))
        .route("/api/v1/config", get(config))
        .route("/api/v1/commands", post(command))
        .route("/api/v1/documents/extractions", post(document_extract))
        .route("/api/v1/documents/{id}", get(document_get))
        .route("/api/v1/rdf", post(rdf_command))
        .route("/api/v1/ontology", post(ontology_command))
        .route("/api/v1/taxonomy", post(taxonomy_command))
        .route("/api/v1/external", post(external_command))
        .route("/api/v1/semantic", post(semantic_command))
        .route("/api/v1/import", post(import_rdf))
        .route("/api/v1/export", post(export_rdf))
        .route("/api/v1/rdf/import", post(import_rdf))
        .route("/api/v1/rdf/export", post(export_rdf))
        .route("/api/v1/jobs", get(list_jobs).post(submit_job))
        .route("/api/v1/jobs/{id}", get(get_job).delete(cancel_job))
        .route("/api/v1/jobs/{id}/results", get(job_results))
        .route("/api/v1/jobs/{id}/cancel", post(cancel_job))
        .route("/api/v1/jobs/{id}/retry", post(retry_job))
        .route(
            "/api/v1/instances",
            get(list_instances).post(create_instance),
        )
        .route(
            "/api/v1/instances/{id}",
            get(get_instance)
                .put(update_instance)
                .delete(delete_instance),
        )
        .route(
            "/api/v1/instances/{id}/statements",
            get(list_statements)
                .post(add_statements)
                .delete(delete_statements),
        )
        .route("/api/v1/instances/{id}/relationships", get(relationships))
        .route("/api/v1/graphs", get(list_graphs))
        .route("/api/v1/sparql/query", post(sparql_query))
        .route("/api/v1/sparql/update", post(sparql_update))
        .route("/api/v1/classes", get(list_classes).post(create_class))
        .route(
            "/api/v1/classes/{id}",
            get(get_class).put(update_class).delete(delete_class),
        )
        .route("/api/v1/classes/{id}/hierarchy", get(class_hierarchy))
        .route(
            "/api/v1/applicable-properties/{class}",
            get(applicable_properties),
        )
        .route("/api/v1/applicable-classes", post(applicable_classes))
        .route("/api/v1/ontology/validate", post(validate_ontology))
        .route(
            "/api/v1/properties",
            get(list_properties).post(create_property),
        )
        .route(
            "/api/v1/properties/{id}",
            get(get_property)
                .put(update_property)
                .delete(delete_property),
        )
        .route(
            "/api/v1/ontologies",
            get(list_ontologies).post(create_ontology),
        )
        .route(
            "/api/v1/ontologies/{id}",
            get(get_ontology).delete(delete_ontology),
        )
        .route("/api/v1/ontologies/{id}/versions", get(ontology_versions))
        .route("/api/v1/schemes", get(list_schemes).post(create_scheme))
        .route(
            "/api/v1/schemes/{id}",
            get(get_scheme).put(update_scheme).delete(delete_scheme),
        )
        .route("/api/v1/schemes/{id}/versions", get(scheme_versions))
        .route("/api/v1/concepts", get(list_concepts).post(create_concept))
        .route(
            "/api/v1/concepts/{id}",
            get(get_concept).put(update_concept).delete(delete_concept),
        )
        .route("/api/v1/concepts/{id}/relations", get(list_relations))
        .route("/api/v1/concepts/{id}/descendants", get(descendants))
        .route("/api/v1/concepts/{id}/ancestors", get(ancestors))
        .route("/api/v1/concepts/{id}/move", post(move_concept))
        .route("/api/v1/concepts/search", get(search_concepts))
        .route("/api/v1/taxonomies/{scheme}/tree", get(concept_tree))
        .route("/api/v1/taxonomies/merge", post(merge_schemes))
        .route("/api/v1/taxonomies/validate", post(validate_taxonomy))
        .route(
            "/api/v1/relations",
            post(add_relation).delete(remove_relation),
        )
        .route(
            "/api/v1/collections",
            get(list_collections).post(create_collection),
        )
        .route(
            "/api/v1/collections/{id}",
            get(get_collection).delete(delete_collection),
        )
        .route("/api/v1/classifications/run", post(classify))
        .route("/api/v1/classifications/batch", post(classify_batch))
        .route(
            "/api/v1/classifications/{instance}",
            get(list_classifications),
        )
        .route(
            "/api/v1/classifications/override",
            post(override_classification),
        )
        .route(
            "/api/v1/classifications/review",
            post(review_classification),
        )
        .route("/api/v1/classifications/reclassify", post(reclassify))
        .route("/api/v1/embeddings/generate", post(generate_embeddings))
        .route(
            "/api/v1/embeddings/{instance}",
            get(list_embeddings).delete(delete_embeddings),
        )
        .route("/api/v1/similarity/search", post(search_similar))
        .route("/api/v1/inference/preview", post(preview_inference))
        .route("/api/v1/inference/apply", post(apply_inference))
        .route("/api/v1/inference/explain", post(explain_inference))
        .route(
            "/api/v1/external-ontologies",
            get(list_external).post(import_external),
        )
        .route(
            "/api/v1/external-ontologies/{id}",
            get(get_external).delete(delete_external),
        )
        .route("/api/v1/external-ontologies/{id}/sync", post(sync_external))
        .route(
            "/api/v1/mappings/classes",
            get(list_class_mappings).post(map_class),
        )
        .route(
            "/api/v1/mappings/properties",
            get(list_property_mappings).post(map_property),
        )
        .route("/api/v1/mappings/validate", post(validate_mappings))
        .route("/api/v1/registry", post(registry_command))
        .route(
            "/api/v1/registry/catalogs",
            get(list_catalogs).post(create_catalog),
        )
        .route("/api/v1/registry/catalogs/{id}", get(get_catalog))
        .route(
            "/api/v1/registry/catalogs/{id}/assets",
            get(list_assets).post(register_asset),
        )
        .route("/api/v1/registry/assets/{key}", get(get_asset))
        .route("/api/v1/registry/assets/{key}/releases", get(list_releases))
        .route(
            "/api/v1/registry/ingestions",
            post(ingest).get(list_candidates),
        )
        .route("/api/v1/registry/ingestions/{id}", get(get_candidate))
        .route(
            "/api/v1/registry/ingestions/{id}/analyze",
            post(analyze_candidate),
        )
        .route(
            "/api/v1/registry/ingestions/{id}/validate",
            post(validate_candidate),
        )
        .route(
            "/api/v1/registry/ingestions/{id}/release-candidate",
            post(release_candidate),
        )
        .route(
            "/api/v1/registry/candidates/{id}/publish",
            post(publish_candidate),
        )
        .route("/api/v1/registry/releases/{key}", get(get_release))
        .route(
            "/api/v1/registry/releases/{key}/archive",
            post(archive_release),
        )
        .route("/api/v1/registry/releases/{key}/terms", get(release_terms))
        .route(
            "/api/v1/registry/releases/{key}/terms/{term}",
            get(get_term),
        )
        .route(
            "/api/v1/registry/releases/{key}/imports",
            get(release_imports),
        )
        .route(
            "/api/v1/registry/releases/{key}/statistics",
            get(release_statistics),
        )
        .route(
            "/api/v1/registry/releases/{key}/diff/{other}",
            get(diff_releases),
        )
        .route("/api/v1/registry/releases/{key}/reports", get(list_reports))
        .route(
            "/api/v1/registry/download/{key}/{format}",
            get(download_release),
        )
        .route(
            "/api/v1/registry/profiles",
            get(list_profiles).post(create_profile),
        )
        .route("/api/v1/registry/search", get(search_registry))
        .route("/api/v1/projects", get(list_projects).post(create_project))
        .route("/api/v1/projects/{project}", get(get_project))
        .route(
            "/api/v1/projects/{project}/goals",
            get(list_goals).post(add_goal),
        )
        .route(
            "/api/v1/projects/{project}/requirements",
            get(list_requirements).post(add_requirement),
        )
        .route(
            "/api/v1/projects/{project}/work",
            get(list_work).post(add_work),
        )
        .route("/api/v1/work/{id}/transition", post(transition_work))
        .route("/api/v1/work/{id}/assignments", post(assign_work))
        .route(
            "/api/v1/projects/{project}/dependencies",
            get(project_dependencies),
        )
        .route(
            "/api/v1/projects/{project}/deliverables",
            get(list_deliverables).post(add_deliverable),
        )
        .route("/api/v1/deliverables/{id}/accept", post(accept_deliverable))
        .route(
            "/api/v1/projects/{project}/snapshots",
            post(create_snapshot),
        )
        .route("/api/v1/projects/{project}/overview", get(project_overview))
        .route("/api/v1/projects/{project}/timeline", get(project_timeline));

    let public = Router::new()
        .route("/api/v1/system/health", get(health))
        .route("/api/v1/openapi.json", get(openapi_document));

    public
        .merge(
            protected
                .layer(middleware::from_fn_with_state(
                    state.clone(),
                    enforce_rate_limit,
                ))
                .layer(middleware::from_fn_with_state(state.clone(), authenticate))
                .layer(middleware::from_fn_with_state(
                    state.clone(),
                    record_metrics,
                )),
        )
        .with_state(state)
}

async fn record_metrics(State(state): State<AppState>, request: Request, next: Next) -> Response {
    let response = next.run(request).await;
    state.metrics.record(!response.status().is_success());
    response
}

async fn enforce_rate_limit(
    State(state): State<AppState>,
    request: Request,
    next: Next,
) -> Response {
    if let Some(limiter) = &state.rate_limit {
        if !limiter.allow() {
            let mut response = (
                StatusCode::TOO_MANY_REQUESTS,
                Json(json!({
                    "error": {
                        "kind": "rate_limited",
                        "message": "request limit exceeded; retry later"
                    }
                })),
            )
                .into_response();
            response
                .headers_mut()
                .insert(RETRY_AFTER, axum::http::HeaderValue::from_static("60"));
            return response;
        }
    }
    next.run(request).await
}

async fn authenticate(State(state): State<AppState>, request: Request, next: Next) -> Response {
    let Some(expected) = state.api_key.as_deref() else {
        return next.run(request).await;
    };
    let authorized = request
        .headers()
        .get(AUTHORIZATION)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.strip_prefix("Bearer "))
        .is_some_and(|token| token == expected);
    if authorized {
        next.run(request).await
    } else {
        (
            StatusCode::UNAUTHORIZED,
            Json(json!({
                "error": {
                    "kind": "unauthorized",
                    "message": "missing or invalid bearer token"
                }
            })),
        )
            .into_response()
    }
}

async fn execute(state: &AppState, command: Command) -> ApiResult {
    Ok(Json(state.service.execute(command).await?))
}

async fn health() -> Json<Value> {
    Json(json!({
        "status": "ok",
        "version": env!("CARGO_PKG_VERSION"),
    }))
}

async fn status(State(state): State<AppState>) -> std::result::Result<Json<Value>, ApiError> {
    let jobs = state
        .service
        .execute(Command::Job(JobCommand::List))
        .await?;
    let job_count = match jobs {
        CommandResult::Collection(collection) => collection.count,
        _ => 0,
    };
    let uptime = (chrono::Utc::now() - state.started_at).num_seconds();
    Ok(Json(json!({
        "status": "ok",
        "version": env!("CARGO_PKG_VERSION"),
        "startedAt": state.started_at,
        "uptimeSeconds": uptime,
        "authentication": state.api_key.is_some(),
        "jobs": job_count,
    })))
}

async fn metrics(State(state): State<AppState>) -> Json<Value> {
    let (requests, errors) = state.metrics.snapshot();
    Json(json!({
        "requests": requests,
        "errors": errors,
        "rateLimitPerMinute": state.rate_limit.as_ref().map(|limiter| limiter.limit()),
        "uptimeSeconds": (chrono::Utc::now() - state.started_at).num_seconds(),
    }))
}

async fn config(State(state): State<AppState>) -> Json<Value> {
    let mut prefixes: Vec<Value> = state
        .service
        .namespaces()
        .prefixes()
        .map(|(prefix, namespace)| json!({ "prefix": prefix, "namespace": namespace }))
        .collect();
    prefixes.sort_by(|left, right| left["prefix"].as_str().cmp(&right["prefix"].as_str()));
    Json(json!({
        "version": env!("CARGO_PKG_VERSION"),
        "authentication": state.api_key.is_some(),
        "namespaces": prefixes,
    }))
}

async fn openapi_document() -> Json<Value> {
    Json(openapi::document())
}

async fn command(State(state): State<AppState>, Json(command): Json<Command>) -> ApiResult {
    execute(&state, command).await
}

async fn rdf_command(State(state): State<AppState>, Json(command): Json<RdfCommand>) -> ApiResult {
    execute(&state, Command::Rdf(command)).await
}

async fn document_extract(
    State(state): State<AppState>,
    Json(request): Json<ExtractionRequest>,
) -> ApiResult {
    execute(&state, Command::Document(DocumentCommand::Extract(request))).await
}

async fn document_get(State(state): State<AppState>, Path(id): Path<String>) -> ApiResult {
    execute(&state, Command::Document(DocumentCommand::Get { id })).await
}

async fn ontology_command(
    State(state): State<AppState>,
    Json(command): Json<OntologyCommand>,
) -> ApiResult {
    execute(&state, Command::Ontology(command)).await
}

async fn taxonomy_command(
    State(state): State<AppState>,
    Json(command): Json<TaxonomyCommand>,
) -> ApiResult {
    execute(&state, Command::Taxonomy(command)).await
}

async fn external_command(
    State(state): State<AppState>,
    Json(command): Json<ExternalCommand>,
) -> ApiResult {
    execute(&state, Command::External(command)).await
}

async fn semantic_command(
    State(state): State<AppState>,
    Json(command): Json<SemanticCommand>,
) -> ApiResult {
    execute(&state, Command::Semantic(command)).await
}

async fn registry_command(
    State(state): State<AppState>,
    Json(command): Json<RegistryCommand>,
) -> ApiResult {
    execute(&state, Command::Registry(command)).await
}

#[derive(Debug, Deserialize)]
struct CreateCatalogBody {
    title: String,
    #[serde(default)]
    description: Option<String>,
}

async fn create_catalog(
    State(state): State<AppState>,
    Json(body): Json<CreateCatalogBody>,
) -> ApiResult {
    execute(
        &state,
        Command::Registry(RegistryCommand::CreateCatalog {
            title: body.title,
            description: body.description,
        }),
    )
    .await
}

async fn list_catalogs(State(state): State<AppState>) -> ApiResult {
    execute(&state, Command::Registry(RegistryCommand::ListCatalogs)).await
}

async fn get_catalog(State(state): State<AppState>, Path(id): Path<String>) -> ApiResult {
    execute(
        &state,
        Command::Registry(RegistryCommand::GetCatalog { id: parse_id(&id)? }),
    )
    .await
}

#[derive(Debug, Deserialize)]
struct RegisterAssetBody {
    #[serde(alias = "id", alias = "asset")]
    asset_iri: Id,
    #[serde(default)]
    title: Option<String>,
    #[serde(default)]
    description: Option<String>,
    #[serde(default)]
    namespace: Option<String>,
}

async fn register_asset(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(body): Json<RegisterAssetBody>,
) -> ApiResult {
    execute(
        &state,
        Command::Registry(RegistryCommand::RegisterAsset {
            catalog: Some(parse_id(&id)?),
            asset_iri: body.asset_iri,
            title: body.title,
            description: body.description,
            namespace: body.namespace,
        }),
    )
    .await
}

async fn list_assets(State(state): State<AppState>, Path(id): Path<String>) -> ApiResult {
    execute(
        &state,
        Command::Registry(RegistryCommand::ListAssets {
            catalog: Some(parse_id(&id)?),
        }),
    )
    .await
}

async fn get_asset(State(state): State<AppState>, Path(key): Path<String>) -> ApiResult {
    execute(&state, Command::Registry(RegistryCommand::GetAsset { key })).await
}

async fn list_releases(State(state): State<AppState>, Path(key): Path<String>) -> ApiResult {
    execute(
        &state,
        Command::Registry(RegistryCommand::ListReleases { asset: Some(key) }),
    )
    .await
}

async fn ingest(State(state): State<AppState>, Json(request): Json<IngestRequest>) -> ApiResult {
    execute(&state, Command::Registry(RegistryCommand::Ingest(request))).await
}

#[derive(Debug, Deserialize)]
struct CandidateParams {
    #[serde(default)]
    asset: Option<String>,
}

async fn list_candidates(
    State(state): State<AppState>,
    Query(params): Query<CandidateParams>,
) -> ApiResult {
    let asset = params.asset.as_deref().map(parse_id).transpose()?;
    execute(
        &state,
        Command::Registry(RegistryCommand::ListCandidates { asset }),
    )
    .await
}

async fn get_candidate(State(state): State<AppState>, Path(id): Path<String>) -> ApiResult {
    execute(
        &state,
        Command::Registry(RegistryCommand::GetCandidate { id: parse_id(&id)? }),
    )
    .await
}

async fn analyze_candidate(State(state): State<AppState>, Path(id): Path<String>) -> ApiResult {
    execute(
        &state,
        Command::Registry(RegistryCommand::Analyze {
            candidate: parse_id(&id)?,
        }),
    )
    .await
}

#[derive(Debug, Default, Deserialize)]
struct ValidationParams {
    #[serde(default)]
    profile: Option<String>,
}

async fn validate_candidate(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Query(params): Query<ValidationParams>,
) -> ApiResult {
    let profile = params.profile.as_deref().map(parse_id).transpose()?;
    execute(
        &state,
        Command::Registry(RegistryCommand::Validate {
            candidate: parse_id(&id)?,
            profile,
        }),
    )
    .await
}

async fn release_candidate(State(state): State<AppState>, Path(id): Path<String>) -> ApiResult {
    execute(
        &state,
        Command::Registry(RegistryCommand::GetCandidate { id: parse_id(&id)? }),
    )
    .await
}

#[derive(Debug, Default, Deserialize)]
struct PublishParams {
    #[serde(default)]
    channel: Option<String>,
}

async fn publish_candidate(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Query(params): Query<PublishParams>,
) -> ApiResult {
    execute(
        &state,
        Command::Registry(RegistryCommand::Publish {
            candidate: parse_id(&id)?,
            channel: params.channel,
        }),
    )
    .await
}

async fn get_release(State(state): State<AppState>, Path(key): Path<String>) -> ApiResult {
    execute(
        &state,
        Command::Registry(RegistryCommand::GetRelease { key }),
    )
    .await
}

#[derive(Debug, Default, Deserialize)]
struct SuccessorParams {
    #[serde(default)]
    successor: Option<String>,
}

async fn archive_release(
    State(state): State<AppState>,
    Path(key): Path<String>,
    Query(params): Query<SuccessorParams>,
) -> ApiResult {
    execute(
        &state,
        Command::Registry(RegistryCommand::ArchiveRelease {
            key,
            successor: params.successor,
        }),
    )
    .await
}

#[derive(Debug, Deserialize)]
struct TermParams {
    #[serde(default, alias = "q")]
    query: Option<String>,
    #[serde(default = "default_registry_page")]
    limit: usize,
    #[serde(default)]
    offset: usize,
}

fn default_registry_page() -> usize {
    ontology_core::registry::DEFAULT_PAGE_SIZE
}

async fn release_terms(
    State(state): State<AppState>,
    Path(key): Path<String>,
    Query(params): Query<TermParams>,
) -> ApiResult {
    execute(
        &state,
        Command::Registry(RegistryCommand::ReleaseTerms {
            release: key,
            query: params.query,
            limit: params.limit,
            offset: params.offset,
        }),
    )
    .await
}

async fn get_term(
    State(state): State<AppState>,
    Path((key, term)): Path<(String, String)>,
) -> ApiResult {
    execute(
        &state,
        Command::Registry(RegistryCommand::GetTerm { release: key, term }),
    )
    .await
}

async fn release_imports(State(state): State<AppState>, Path(key): Path<String>) -> ApiResult {
    execute(
        &state,
        Command::Registry(RegistryCommand::ReleaseImports { release: key }),
    )
    .await
}

async fn release_statistics(State(state): State<AppState>, Path(key): Path<String>) -> ApiResult {
    execute(
        &state,
        Command::Registry(RegistryCommand::ReleaseStatistics { release: key }),
    )
    .await
}

async fn diff_releases(
    State(state): State<AppState>,
    Path((key, other)): Path<(String, String)>,
) -> ApiResult {
    execute(
        &state,
        Command::Registry(RegistryCommand::Diff {
            left: key,
            right: other,
        }),
    )
    .await
}

async fn list_reports(State(state): State<AppState>, Path(key): Path<String>) -> ApiResult {
    execute(
        &state,
        Command::Registry(RegistryCommand::ListReports { release: key }),
    )
    .await
}

async fn download_release(
    State(state): State<AppState>,
    Path((key, format)): Path<(String, String)>,
) -> std::result::Result<Response, ApiError> {
    let registry = state.service.registry().ok_or_else(|| {
        ApiError(Error::Unsupported {
            operation: "registry is not configured".to_owned(),
        })
    })?;
    let (media_type, bytes) = registry.download(&key, &format)?;
    Response::builder()
        .header(CONTENT_TYPE, media_type)
        .header(
            CONTENT_DISPOSITION,
            format!("attachment; filename=\"{key}.{format}\""),
        )
        .body(Body::from(bytes))
        .map_err(|error| {
            ApiError(Error::Io {
                detail: error.to_string(),
            })
        })
}

async fn create_profile(
    State(state): State<AppState>,
    Json(body): Json<CreateProfileBody>,
) -> ApiResult {
    execute(
        &state,
        Command::Registry(RegistryCommand::CreateProfile {
            id: body.id,
            title: body.title,
            description: body.description,
        }),
    )
    .await
}

#[derive(Debug, Deserialize)]
struct CreateProfileBody {
    id: Id,
    title: String,
    #[serde(default)]
    description: Option<String>,
}

async fn list_profiles(State(state): State<AppState>) -> ApiResult {
    execute(&state, Command::Registry(RegistryCommand::ListProfiles)).await
}

#[derive(Debug, Deserialize)]
struct RegistrySearchParams {
    #[serde(alias = "query")]
    q: String,
    #[serde(default)]
    kind: Option<String>,
    #[serde(default = "default_registry_page")]
    limit: usize,
    #[serde(default)]
    offset: usize,
}

async fn search_registry(
    State(state): State<AppState>,
    Query(params): Query<RegistrySearchParams>,
) -> ApiResult {
    execute(
        &state,
        Command::Registry(RegistryCommand::Search {
            query: params.q,
            kind: params.kind,
            limit: params.limit,
            offset: params.offset,
        }),
    )
    .await
}

async fn create_project(State(state): State<AppState>, Json(body): Json<NewProject>) -> ApiResult {
    execute(&state, Command::Project(ProjectCommand::Create(body))).await
}

async fn list_projects(State(state): State<AppState>) -> ApiResult {
    execute(&state, Command::Project(ProjectCommand::List)).await
}

async fn get_project(State(state): State<AppState>, Path(project): Path<String>) -> ApiResult {
    execute(
        &state,
        Command::Project(ProjectCommand::Get {
            project: parse_id(&project)?,
        }),
    )
    .await
}

#[derive(Debug, Deserialize)]
struct AddGoalBody {
    title: String,
    #[serde(default)]
    description: Option<String>,
}

async fn add_goal(
    State(state): State<AppState>,
    Path(project): Path<String>,
    Json(body): Json<AddGoalBody>,
) -> ApiResult {
    execute(
        &state,
        Command::Project(ProjectCommand::AddGoal {
            project: parse_id(&project)?,
            title: body.title,
            description: body.description,
        }),
    )
    .await
}

async fn list_goals(State(state): State<AppState>, Path(project): Path<String>) -> ApiResult {
    execute(
        &state,
        Command::Project(ProjectCommand::ListGoals {
            project: parse_id(&project)?,
        }),
    )
    .await
}

#[derive(Debug, Deserialize)]
struct AddRequirementBody {
    title: String,
    #[serde(default)]
    description: Option<String>,
    #[serde(default)]
    kind: Option<String>,
}

async fn add_requirement(
    State(state): State<AppState>,
    Path(project): Path<String>,
    Json(body): Json<AddRequirementBody>,
) -> ApiResult {
    execute(
        &state,
        Command::Project(ProjectCommand::AddRequirement {
            project: parse_id(&project)?,
            title: body.title,
            description: body.description,
            kind: body.kind,
        }),
    )
    .await
}

async fn list_requirements(
    State(state): State<AppState>,
    Path(project): Path<String>,
) -> ApiResult {
    execute(
        &state,
        Command::Project(ProjectCommand::ListRequirements {
            project: parse_id(&project)?,
        }),
    )
    .await
}

#[derive(Debug, Deserialize)]
struct AddWorkBody {
    kind: WorkKind,
    title: String,
    #[serde(default)]
    description: Option<String>,
    #[serde(default)]
    parent: Option<Id>,
    #[serde(default)]
    assignee: Option<String>,
    #[serde(default)]
    due: Option<chrono::DateTime<chrono::Utc>>,
}

async fn add_work(
    State(state): State<AppState>,
    Path(project): Path<String>,
    Json(body): Json<AddWorkBody>,
) -> ApiResult {
    execute(
        &state,
        Command::Project(ProjectCommand::AddWork {
            project: parse_id(&project)?,
            parent: body.parent,
            kind: body.kind,
            title: body.title,
            description: body.description,
            assignee: body.assignee,
            due: body.due,
        }),
    )
    .await
}

async fn list_work(State(state): State<AppState>, Path(project): Path<String>) -> ApiResult {
    execute(
        &state,
        Command::Project(ProjectCommand::ListWork {
            project: parse_id(&project)?,
        }),
    )
    .await
}

#[derive(Debug, Deserialize)]
struct TransitionBody {
    event: TaskEvent,
}

async fn transition_work(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(body): Json<TransitionBody>,
) -> ApiResult {
    execute(
        &state,
        Command::Project(ProjectCommand::Transition {
            work: parse_id(&id)?,
            event: body.event,
        }),
    )
    .await
}

#[derive(Debug, Deserialize)]
struct AssignBody {
    assignee: String,
}

async fn assign_work(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(body): Json<AssignBody>,
) -> ApiResult {
    execute(
        &state,
        Command::Project(ProjectCommand::Assign {
            work: parse_id(&id)?,
            assignee: body.assignee,
        }),
    )
    .await
}

async fn project_dependencies(
    State(state): State<AppState>,
    Path(project): Path<String>,
) -> ApiResult {
    execute(
        &state,
        Command::Project(ProjectCommand::Dependencies {
            project: parse_id(&project)?,
        }),
    )
    .await
}

#[derive(Debug, Deserialize)]
struct AddDeliverableBody {
    title: String,
    #[serde(default)]
    kinds: Vec<String>,
    #[serde(default)]
    asset: Option<Id>,
    #[serde(default)]
    release: Option<Id>,
    #[serde(default)]
    acceptance_criteria: Vec<String>,
}

async fn add_deliverable(
    State(state): State<AppState>,
    Path(project): Path<String>,
    Json(body): Json<AddDeliverableBody>,
) -> ApiResult {
    execute(
        &state,
        Command::Project(ProjectCommand::AddDeliverable {
            project: parse_id(&project)?,
            title: body.title,
            kinds: body.kinds,
            asset: body.asset,
            release: body.release,
            acceptance_criteria: body.acceptance_criteria,
        }),
    )
    .await
}

async fn list_deliverables(
    State(state): State<AppState>,
    Path(project): Path<String>,
) -> ApiResult {
    execute(
        &state,
        Command::Project(ProjectCommand::ListDeliverables {
            project: parse_id(&project)?,
        }),
    )
    .await
}

#[derive(Debug, Default, Deserialize)]
struct AcceptBody {
    #[serde(default)]
    evidence: Option<String>,
    #[serde(default)]
    criterion: Option<String>,
}

async fn accept_deliverable(
    State(state): State<AppState>,
    Path(id): Path<String>,
    body: Option<Json<AcceptBody>>,
) -> ApiResult {
    let body = body.map(|Json(body)| body).unwrap_or_default();
    execute(
        &state,
        Command::Project(ProjectCommand::Accept {
            deliverable: parse_id(&id)?,
            evidence: body.evidence,
            criterion: body.criterion,
        }),
    )
    .await
}

async fn create_snapshot(State(state): State<AppState>, Path(project): Path<String>) -> ApiResult {
    execute(
        &state,
        Command::Project(ProjectCommand::Snapshot {
            project: parse_id(&project)?,
        }),
    )
    .await
}

async fn project_overview(State(state): State<AppState>, Path(project): Path<String>) -> ApiResult {
    execute(
        &state,
        Command::Project(ProjectCommand::Overview {
            project: parse_id(&project)?,
        }),
    )
    .await
}

async fn project_timeline(State(state): State<AppState>, Path(project): Path<String>) -> ApiResult {
    execute(
        &state,
        Command::Project(ProjectCommand::Timeline {
            project: parse_id(&project)?,
        }),
    )
    .await
}

fn parse_id(value: &str) -> std::result::Result<Id, ApiError> {
    Id::new(value).map_err(ApiError::from)
}

fn parse_graph(value: Option<&str>) -> std::result::Result<Option<GraphName>, ApiError> {
    match value {
        None | Some("") => Ok(None),
        Some("default") => Ok(Some(GraphName::Default)),
        Some(value) => Ok(Some(GraphName::Iri(parse_id(value)?))),
    }
}

fn parse_property_kind(value: &str) -> std::result::Result<PropertyKind, ApiError> {
    match value {
        "object" => Ok(PropertyKind::Object),
        "datatype" => Ok(PropertyKind::Datatype),
        "annotation" => Ok(PropertyKind::Annotation),
        other => Err(ApiError(Error::invalid(format!(
            "unknown property kind `{other}`"
        )))),
    }
}

#[derive(Debug, Deserialize)]
struct InstanceParams {
    #[serde(default)]
    graph: Option<String>,
    #[serde(default, rename = "type")]
    type_filter: Option<String>,
    #[serde(default)]
    limit: Option<usize>,
    #[serde(default)]
    offset: Option<usize>,
}

#[derive(Debug, Deserialize)]
struct ClassParams {
    #[serde(default)]
    parent: Option<String>,
    #[serde(default)]
    label: Option<String>,
    #[serde(default)]
    limit: Option<usize>,
    #[serde(default)]
    offset: Option<usize>,
}

#[derive(Debug, Deserialize)]
struct PropertyParams {
    #[serde(default)]
    kind: Option<String>,
    #[serde(default)]
    label: Option<String>,
    #[serde(default)]
    limit: Option<usize>,
    #[serde(default)]
    offset: Option<usize>,
}

#[derive(Debug, Deserialize)]
struct ConceptParams {
    #[serde(default)]
    scheme: Option<String>,
    #[serde(default)]
    label: Option<String>,
    #[serde(default)]
    limit: Option<usize>,
    #[serde(default)]
    offset: Option<usize>,
}

#[derive(Debug, Deserialize)]
struct GraphParams {
    #[serde(default)]
    graph: Option<String>,
}

#[derive(Debug, Deserialize)]
struct SearchParams {
    q: String,
    #[serde(default)]
    scheme: Option<String>,
}

#[derive(Debug, Deserialize)]
struct MappingParams {
    #[serde(default)]
    external: Option<String>,
    #[serde(default)]
    internal: Option<String>,
    #[serde(default)]
    limit: Option<usize>,
    #[serde(default)]
    offset: Option<usize>,
}

#[derive(Debug, Deserialize)]
struct SparqlBody {
    #[serde(alias = "sparql")]
    query: String,
}

#[derive(Debug, Deserialize)]
struct AddStatementsBody {
    statements: Vec<StatementInput>,
    #[serde(default)]
    source: Option<String>,
    #[serde(default)]
    graph: Option<String>,
}

#[derive(Debug, Deserialize)]
struct ImportBody {
    format: String,
    data: String,
    #[serde(default)]
    graph: Option<String>,
}

#[derive(Debug, Deserialize)]
struct ExportBody {
    format: String,
    #[serde(default)]
    graph: Option<String>,
}

#[derive(Debug, Deserialize)]
struct ClassifyBody {
    instance: Id,
    #[serde(default)]
    options: Option<ClassificationOptions>,
}

#[derive(Debug, Deserialize)]
struct ClassifyBatchBody {
    instances: Vec<Id>,
    #[serde(default)]
    options: Option<ClassificationOptions>,
}

#[derive(Debug, Deserialize)]
struct OverrideBody {
    instance: Id,
    concept: Id,
    confidence: f64,
}

#[derive(Debug, Deserialize)]
struct ReviewBody {
    classification: Id,
    approved: bool,
}

#[derive(Debug, Deserialize)]
struct EmbeddingBody {
    instance: Id,
    #[serde(default)]
    attributes: Vec<Id>,
}

#[derive(Debug, Deserialize)]
struct SimilarityBody {
    instance: Id,
    #[serde(default)]
    limit: Option<usize>,
    #[serde(default)]
    filter: Option<VectorFilter>,
}

#[derive(Debug, Deserialize)]
struct InferenceBody {
    instance: Id,
    #[serde(default)]
    options: Option<InferenceOptions>,
    #[serde(default)]
    policy: Option<ApplyPolicy>,
}

#[derive(Debug, Deserialize)]
struct RelationBody {
    source: Id,
    relation: SkosRelation,
    target: Id,
}

#[derive(Debug, Deserialize)]
struct MoveBody {
    #[serde(default)]
    broader: Option<Id>,
    #[serde(default)]
    scheme: Option<Id>,
}

#[derive(Debug, Deserialize)]
struct MergeBody {
    source: Id,
    target: Id,
}

#[derive(Debug, Deserialize)]
struct TaxonomyBody {
    #[serde(default)]
    scheme: Option<Id>,
}

#[derive(Debug, Deserialize)]
struct ApplicableClassesBody {
    properties: Vec<Id>,
}

async fn list_instances(
    State(state): State<AppState>,
    Query(params): Query<InstanceParams>,
) -> ApiResult {
    let mut query = InstanceQuery::default();
    query.graph = parse_graph(params.graph.as_deref())?;
    if let Some(value) = params.type_filter {
        query.type_filter = Some(parse_id(&value)?);
    }
    query.limit = params.limit.unwrap_or(query.limit);
    query.offset = params.offset.unwrap_or(query.offset);
    execute(&state, Command::Rdf(RdfCommand::ListInstances(query))).await
}

async fn create_instance(State(state): State<AppState>, Json(new): Json<NewInstance>) -> ApiResult {
    execute(&state, Command::Rdf(RdfCommand::CreateInstance(new))).await
}

async fn get_instance(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Query(params): Query<GraphParams>,
) -> ApiResult {
    execute(
        &state,
        Command::Rdf(RdfCommand::GetInstance {
            id: parse_id(&id)?,
            graph: parse_graph(params.graph.as_deref())?,
        }),
    )
    .await
}

async fn update_instance(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Query(params): Query<GraphParams>,
    Json(patch): Json<InstancePatch>,
) -> ApiResult {
    execute(
        &state,
        Command::Rdf(RdfCommand::UpdateInstance {
            id: parse_id(&id)?,
            patch,
            graph: parse_graph(params.graph.as_deref())?,
            source: None,
        }),
    )
    .await
}

async fn delete_instance(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Query(params): Query<GraphParams>,
) -> ApiResult {
    execute(
        &state,
        Command::Rdf(RdfCommand::DeleteInstance {
            id: parse_id(&id)?,
            graph: parse_graph(params.graph.as_deref())?,
        }),
    )
    .await
}

async fn list_statements(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Query(params): Query<GraphParams>,
) -> ApiResult {
    execute(
        &state,
        Command::Rdf(RdfCommand::ListStatements {
            id: parse_id(&id)?,
            graph: parse_graph(params.graph.as_deref())?,
        }),
    )
    .await
}

async fn add_statements(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(body): Json<AddStatementsBody>,
) -> ApiResult {
    execute(
        &state,
        Command::Rdf(RdfCommand::AddStatements {
            id: parse_id(&id)?,
            statements: body.statements,
            graph: parse_graph(body.graph.as_deref())?,
            source: body.source,
        }),
    )
    .await
}

async fn delete_statements(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Query(params): Query<GraphParams>,
    Json(statement_ids): Json<Vec<StatementId>>,
) -> ApiResult {
    execute(
        &state,
        Command::Rdf(RdfCommand::DeleteStatements {
            id: parse_id(&id)?,
            statement_ids,
            graph: parse_graph(params.graph.as_deref())?,
        }),
    )
    .await
}

async fn relationships(State(state): State<AppState>, Path(id): Path<String>) -> ApiResult {
    execute(
        &state,
        Command::Rdf(RdfCommand::GetRelationships { id: parse_id(&id)? }),
    )
    .await
}

async fn list_graphs(State(state): State<AppState>) -> ApiResult {
    execute(&state, Command::Rdf(RdfCommand::ListGraphs)).await
}

async fn sparql_query(State(state): State<AppState>, Json(body): Json<SparqlBody>) -> ApiResult {
    execute(
        &state,
        Command::Rdf(RdfCommand::Query { sparql: body.query }),
    )
    .await
}

async fn sparql_update(State(state): State<AppState>, Json(body): Json<SparqlBody>) -> ApiResult {
    execute(
        &state,
        Command::Rdf(RdfCommand::Update { sparql: body.query }),
    )
    .await
}

async fn import_rdf(State(state): State<AppState>, Json(body): Json<ImportBody>) -> ApiResult {
    execute(
        &state,
        Command::Rdf(RdfCommand::Import {
            format: body.format,
            data: body.data,
            graph: parse_graph(body.graph.as_deref())?,
        }),
    )
    .await
}

async fn export_rdf(State(state): State<AppState>, Json(body): Json<ExportBody>) -> ApiResult {
    execute(
        &state,
        Command::Rdf(RdfCommand::Export {
            format: body.format,
            graph: parse_graph(body.graph.as_deref())?,
        }),
    )
    .await
}

async fn list_classes(
    State(state): State<AppState>,
    Query(params): Query<ClassParams>,
) -> ApiResult {
    let mut query = ClassQuery::default();
    if let Some(value) = params.parent {
        query.parent = Some(parse_id(&value)?);
    }
    query.label_contains = params.label;
    query.limit = params.limit.unwrap_or(query.limit);
    query.offset = params.offset.unwrap_or(query.offset);
    execute(
        &state,
        Command::Ontology(OntologyCommand::ListClasses(query)),
    )
    .await
}

async fn create_class(State(state): State<AppState>, Json(class): Json<OwlClass>) -> ApiResult {
    execute(
        &state,
        Command::Ontology(OntologyCommand::CreateClass(class)),
    )
    .await
}

async fn get_class(State(state): State<AppState>, Path(id): Path<String>) -> ApiResult {
    execute(
        &state,
        Command::Ontology(OntologyCommand::GetClass { id: parse_id(&id)? }),
    )
    .await
}

async fn update_class(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(mut class): Json<OwlClass>,
) -> ApiResult {
    class.id = parse_id(&id)?;
    execute(
        &state,
        Command::Ontology(OntologyCommand::UpdateClass(class)),
    )
    .await
}

async fn delete_class(State(state): State<AppState>, Path(id): Path<String>) -> ApiResult {
    execute(
        &state,
        Command::Ontology(OntologyCommand::DeleteClass { id: parse_id(&id)? }),
    )
    .await
}

async fn class_hierarchy(State(state): State<AppState>, Path(id): Path<String>) -> ApiResult {
    execute(
        &state,
        Command::Ontology(OntologyCommand::ClassHierarchy { id: parse_id(&id)? }),
    )
    .await
}

async fn applicable_properties(
    State(state): State<AppState>,
    Path(class): Path<String>,
) -> ApiResult {
    execute(
        &state,
        Command::Ontology(OntologyCommand::ApplicableProperties {
            class: parse_id(&class)?,
        }),
    )
    .await
}

async fn applicable_classes(
    State(state): State<AppState>,
    Json(body): Json<ApplicableClassesBody>,
) -> ApiResult {
    execute(
        &state,
        Command::Ontology(OntologyCommand::ApplicableClasses {
            properties: body.properties,
        }),
    )
    .await
}

async fn validate_ontology(
    State(state): State<AppState>,
    Json(scope): Json<GraphScope>,
) -> ApiResult {
    execute(
        &state,
        Command::Ontology(OntologyCommand::Validate { scope }),
    )
    .await
}

async fn list_properties(
    State(state): State<AppState>,
    Query(params): Query<PropertyParams>,
) -> ApiResult {
    let mut query = PropertyQuery::new();
    if let Some(value) = params.kind {
        query.kind = Some(parse_property_kind(&value)?);
    }
    query.label_contains = params.label;
    query.limit = params.limit.unwrap_or(query.limit);
    query.offset = params.offset.unwrap_or(query.offset);
    execute(
        &state,
        Command::Ontology(OntologyCommand::ListProperties(query)),
    )
    .await
}

async fn create_property(
    State(state): State<AppState>,
    Json(property): Json<OwlProperty>,
) -> ApiResult {
    execute(
        &state,
        Command::Ontology(OntologyCommand::CreateProperty(property)),
    )
    .await
}

async fn get_property(State(state): State<AppState>, Path(id): Path<String>) -> ApiResult {
    execute(
        &state,
        Command::Ontology(OntologyCommand::GetProperty { id: parse_id(&id)? }),
    )
    .await
}

async fn update_property(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(mut property): Json<OwlProperty>,
) -> ApiResult {
    property.id = parse_id(&id)?;
    execute(
        &state,
        Command::Ontology(OntologyCommand::UpdateProperty(property)),
    )
    .await
}

async fn delete_property(State(state): State<AppState>, Path(id): Path<String>) -> ApiResult {
    execute(
        &state,
        Command::Ontology(OntologyCommand::DeleteProperty { id: parse_id(&id)? }),
    )
    .await
}

async fn list_ontologies(State(state): State<AppState>) -> ApiResult {
    execute(&state, Command::Ontology(OntologyCommand::ListOntologies)).await
}

async fn create_ontology(
    State(state): State<AppState>,
    Json(ontology): Json<Ontology>,
) -> ApiResult {
    execute(
        &state,
        Command::Ontology(OntologyCommand::CreateOntology(ontology)),
    )
    .await
}

async fn get_ontology(State(state): State<AppState>, Path(id): Path<String>) -> ApiResult {
    execute(
        &state,
        Command::Ontology(OntologyCommand::GetOntology { id: parse_id(&id)? }),
    )
    .await
}

async fn delete_ontology(State(state): State<AppState>, Path(id): Path<String>) -> ApiResult {
    execute(
        &state,
        Command::Ontology(OntologyCommand::DeleteOntology { id: parse_id(&id)? }),
    )
    .await
}

async fn ontology_versions(State(state): State<AppState>, Path(id): Path<String>) -> ApiResult {
    execute(
        &state,
        Command::Ontology(OntologyCommand::OntologyVersions { id: parse_id(&id)? }),
    )
    .await
}

async fn list_schemes(State(state): State<AppState>) -> ApiResult {
    execute(&state, Command::Taxonomy(TaxonomyCommand::ListSchemes)).await
}

async fn create_scheme(
    State(state): State<AppState>,
    Json(scheme): Json<ConceptScheme>,
) -> ApiResult {
    execute(
        &state,
        Command::Taxonomy(TaxonomyCommand::CreateScheme(scheme)),
    )
    .await
}

async fn get_scheme(State(state): State<AppState>, Path(id): Path<String>) -> ApiResult {
    execute(
        &state,
        Command::Taxonomy(TaxonomyCommand::GetScheme { id: parse_id(&id)? }),
    )
    .await
}

async fn update_scheme(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(mut scheme): Json<ConceptScheme>,
) -> ApiResult {
    scheme.id = parse_id(&id)?;
    execute(
        &state,
        Command::Taxonomy(TaxonomyCommand::UpdateScheme(scheme)),
    )
    .await
}

async fn delete_scheme(State(state): State<AppState>, Path(id): Path<String>) -> ApiResult {
    execute(
        &state,
        Command::Taxonomy(TaxonomyCommand::DeleteScheme { id: parse_id(&id)? }),
    )
    .await
}

async fn scheme_versions(State(state): State<AppState>, Path(id): Path<String>) -> ApiResult {
    execute(
        &state,
        Command::Taxonomy(TaxonomyCommand::SchemeVersions { id: parse_id(&id)? }),
    )
    .await
}

async fn list_concepts(
    State(state): State<AppState>,
    Query(params): Query<ConceptParams>,
) -> ApiResult {
    let mut query = TaxonomyQuery::new();
    if let Some(value) = params.scheme {
        query.scheme = Some(parse_id(&value)?);
    }
    query.label_contains = params.label;
    query.limit = params.limit.unwrap_or(query.limit);
    query.offset = params.offset.unwrap_or(query.offset);
    execute(
        &state,
        Command::Taxonomy(TaxonomyCommand::ListConcepts(query)),
    )
    .await
}

async fn create_concept(State(state): State<AppState>, Json(concept): Json<Concept>) -> ApiResult {
    execute(
        &state,
        Command::Taxonomy(TaxonomyCommand::CreateConcept(concept)),
    )
    .await
}

async fn get_concept(State(state): State<AppState>, Path(id): Path<String>) -> ApiResult {
    execute(
        &state,
        Command::Taxonomy(TaxonomyCommand::GetConcept { id: parse_id(&id)? }),
    )
    .await
}

async fn update_concept(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(mut concept): Json<Concept>,
) -> ApiResult {
    concept.id = parse_id(&id)?;
    execute(
        &state,
        Command::Taxonomy(TaxonomyCommand::UpdateConcept(concept)),
    )
    .await
}

async fn delete_concept(State(state): State<AppState>, Path(id): Path<String>) -> ApiResult {
    execute(
        &state,
        Command::Taxonomy(TaxonomyCommand::DeleteConcept { id: parse_id(&id)? }),
    )
    .await
}

async fn search_concepts(
    State(state): State<AppState>,
    Query(params): Query<SearchParams>,
) -> ApiResult {
    let scheme = params.scheme.map(|value| parse_id(&value)).transpose()?;
    execute(
        &state,
        Command::Taxonomy(TaxonomyCommand::SearchConcepts {
            query: params.q,
            scheme,
        }),
    )
    .await
}

async fn list_relations(State(state): State<AppState>, Path(id): Path<String>) -> ApiResult {
    execute(
        &state,
        Command::Taxonomy(TaxonomyCommand::ListRelations { id: parse_id(&id)? }),
    )
    .await
}

async fn add_relation(State(state): State<AppState>, Json(body): Json<RelationBody>) -> ApiResult {
    execute(
        &state,
        Command::Taxonomy(TaxonomyCommand::AddRelation {
            source: body.source,
            relation: body.relation,
            target: body.target,
        }),
    )
    .await
}

async fn remove_relation(
    State(state): State<AppState>,
    Json(body): Json<RelationBody>,
) -> ApiResult {
    execute(
        &state,
        Command::Taxonomy(TaxonomyCommand::RemoveRelation {
            source: body.source,
            relation: body.relation,
            target: body.target,
        }),
    )
    .await
}

async fn descendants(State(state): State<AppState>, Path(id): Path<String>) -> ApiResult {
    execute(
        &state,
        Command::Taxonomy(TaxonomyCommand::Descendants { id: parse_id(&id)? }),
    )
    .await
}

async fn ancestors(State(state): State<AppState>, Path(id): Path<String>) -> ApiResult {
    execute(
        &state,
        Command::Taxonomy(TaxonomyCommand::Ancestors { id: parse_id(&id)? }),
    )
    .await
}

async fn move_concept(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(body): Json<MoveBody>,
) -> ApiResult {
    execute(
        &state,
        Command::Taxonomy(TaxonomyCommand::MoveConcept {
            id: parse_id(&id)?,
            broader: body.broader,
            scheme: body.scheme,
        }),
    )
    .await
}

async fn concept_tree(State(state): State<AppState>, Path(scheme): Path<String>) -> ApiResult {
    execute(
        &state,
        Command::Taxonomy(TaxonomyCommand::ConceptTree {
            scheme: parse_id(&scheme)?,
        }),
    )
    .await
}

async fn merge_schemes(State(state): State<AppState>, Json(body): Json<MergeBody>) -> ApiResult {
    execute(
        &state,
        Command::Taxonomy(TaxonomyCommand::MergeSchemes {
            source: body.source,
            target: body.target,
        }),
    )
    .await
}

async fn validate_taxonomy(
    State(state): State<AppState>,
    Json(body): Json<TaxonomyBody>,
) -> ApiResult {
    execute(
        &state,
        Command::Taxonomy(TaxonomyCommand::ValidateTaxonomy {
            scheme: body.scheme,
        }),
    )
    .await
}

async fn list_collections(
    State(state): State<AppState>,
    Query(params): Query<GraphParams>,
) -> ApiResult {
    let scheme = params.graph.map(|value| parse_id(&value)).transpose()?;
    execute(
        &state,
        Command::Taxonomy(TaxonomyCommand::ListCollections { scheme }),
    )
    .await
}

async fn create_collection(
    State(state): State<AppState>,
    Json(collection): Json<ConceptCollection>,
) -> ApiResult {
    execute(
        &state,
        Command::Taxonomy(TaxonomyCommand::CreateCollection(collection)),
    )
    .await
}

async fn get_collection(State(state): State<AppState>, Path(id): Path<String>) -> ApiResult {
    execute(
        &state,
        Command::Taxonomy(TaxonomyCommand::GetCollection { id: parse_id(&id)? }),
    )
    .await
}

async fn delete_collection(State(state): State<AppState>, Path(id): Path<String>) -> ApiResult {
    execute(
        &state,
        Command::Taxonomy(TaxonomyCommand::DeleteCollection { id: parse_id(&id)? }),
    )
    .await
}

async fn classify(State(state): State<AppState>, Json(body): Json<ClassifyBody>) -> ApiResult {
    execute(
        &state,
        Command::Semantic(SemanticCommand::Classify {
            instance: body.instance,
            options: body.options.unwrap_or_default(),
        }),
    )
    .await
}

async fn classify_batch(
    State(state): State<AppState>,
    Json(body): Json<ClassifyBatchBody>,
) -> ApiResult {
    execute(
        &state,
        Command::Semantic(SemanticCommand::ClassifyBatch {
            instances: body.instances,
            options: body.options.unwrap_or_default(),
        }),
    )
    .await
}

async fn list_classifications(
    State(state): State<AppState>,
    Path(instance): Path<String>,
) -> ApiResult {
    execute(
        &state,
        Command::Semantic(SemanticCommand::ListClassifications {
            instance: parse_id(&instance)?,
        }),
    )
    .await
}

async fn override_classification(
    State(state): State<AppState>,
    Json(body): Json<OverrideBody>,
) -> ApiResult {
    execute(
        &state,
        Command::Semantic(SemanticCommand::OverrideClassification {
            instance: body.instance,
            concept: body.concept,
            confidence: body.confidence,
        }),
    )
    .await
}

async fn review_classification(
    State(state): State<AppState>,
    Json(body): Json<ReviewBody>,
) -> ApiResult {
    execute(
        &state,
        Command::Semantic(SemanticCommand::ReviewClassification {
            classification: body.classification,
            approved: body.approved,
        }),
    )
    .await
}

async fn reclassify(State(state): State<AppState>, Json(body): Json<ClassifyBody>) -> ApiResult {
    execute(
        &state,
        Command::Semantic(SemanticCommand::Reclassify {
            instance: body.instance,
            options: body.options.unwrap_or_default(),
        }),
    )
    .await
}

async fn generate_embeddings(
    State(state): State<AppState>,
    Json(body): Json<EmbeddingBody>,
) -> ApiResult {
    execute(
        &state,
        Command::Semantic(SemanticCommand::GenerateEmbeddings {
            instance: body.instance,
            attributes: body.attributes,
        }),
    )
    .await
}

async fn list_embeddings(State(state): State<AppState>, Path(instance): Path<String>) -> ApiResult {
    execute(
        &state,
        Command::Semantic(SemanticCommand::ListEmbeddings {
            instance: parse_id(&instance)?,
        }),
    )
    .await
}

async fn delete_embeddings(
    State(state): State<AppState>,
    Path(instance): Path<String>,
) -> ApiResult {
    execute(
        &state,
        Command::Semantic(SemanticCommand::DeleteEmbeddings {
            instance: parse_id(&instance)?,
        }),
    )
    .await
}

async fn search_similar(
    State(state): State<AppState>,
    Json(body): Json<SimilarityBody>,
) -> ApiResult {
    execute(
        &state,
        Command::Semantic(SemanticCommand::SearchSimilar {
            instance: body.instance,
            limit: body.limit.unwrap_or(10),
            filter: body.filter,
        }),
    )
    .await
}

async fn preview_inference(
    State(state): State<AppState>,
    Json(body): Json<InferenceBody>,
) -> ApiResult {
    execute(
        &state,
        Command::Semantic(SemanticCommand::PreviewInference {
            instance: body.instance,
            options: body.options.unwrap_or_default(),
        }),
    )
    .await
}

async fn apply_inference(
    State(state): State<AppState>,
    Json(body): Json<InferenceBody>,
) -> ApiResult {
    execute(
        &state,
        Command::Semantic(SemanticCommand::ApplyInference {
            instance: body.instance,
            options: body.options.unwrap_or_default(),
            policy: body.policy.unwrap_or(ApplyPolicy::Suggest),
        }),
    )
    .await
}

async fn explain_inference(
    State(state): State<AppState>,
    Json(body): Json<InferenceBody>,
) -> ApiResult {
    execute(
        &state,
        Command::Semantic(SemanticCommand::ExplainInference {
            instance: body.instance,
            options: body.options.unwrap_or_default(),
        }),
    )
    .await
}

async fn list_external(State(state): State<AppState>) -> ApiResult {
    execute(&state, Command::External(ExternalCommand::List)).await
}

async fn import_external(
    State(state): State<AppState>,
    Json(request): Json<ImportRequest>,
) -> ApiResult {
    execute(&state, Command::External(ExternalCommand::Import(request))).await
}

async fn get_external(State(state): State<AppState>, Path(id): Path<String>) -> ApiResult {
    execute(
        &state,
        Command::External(ExternalCommand::Get { id: parse_id(&id)? }),
    )
    .await
}

async fn delete_external(State(state): State<AppState>, Path(id): Path<String>) -> ApiResult {
    execute(
        &state,
        Command::External(ExternalCommand::Delete { id: parse_id(&id)? }),
    )
    .await
}

async fn sync_external(State(state): State<AppState>, Path(id): Path<String>) -> ApiResult {
    execute(
        &state,
        Command::External(ExternalCommand::Sync { id: parse_id(&id)? }),
    )
    .await
}

async fn list_class_mappings(
    State(state): State<AppState>,
    Query(params): Query<MappingParams>,
) -> ApiResult {
    let mut query = MappingQuery::default();
    if let Some(value) = params.external {
        query.external = Some(parse_id(&value)?);
    }
    if let Some(value) = params.internal {
        query.internal = Some(parse_id(&value)?);
    }
    query.limit = params.limit.unwrap_or(query.limit);
    query.offset = params.offset.unwrap_or(query.offset);
    execute(
        &state,
        Command::External(ExternalCommand::ListClassMappings(query)),
    )
    .await
}

async fn list_property_mappings(
    State(state): State<AppState>,
    Query(params): Query<MappingParams>,
) -> ApiResult {
    let mut query = MappingQuery::default();
    if let Some(value) = params.external {
        query.external = Some(parse_id(&value)?);
    }
    if let Some(value) = params.internal {
        query.internal = Some(parse_id(&value)?);
    }
    query.limit = params.limit.unwrap_or(query.limit);
    query.offset = params.offset.unwrap_or(query.offset);
    execute(
        &state,
        Command::External(ExternalCommand::ListPropertyMappings(query)),
    )
    .await
}

async fn map_class(State(state): State<AppState>, Json(mapping): Json<ClassMapping>) -> ApiResult {
    execute(
        &state,
        Command::External(ExternalCommand::MapClass(mapping)),
    )
    .await
}

async fn map_property(
    State(state): State<AppState>,
    Json(mapping): Json<PropertyMapping>,
) -> ApiResult {
    execute(
        &state,
        Command::External(ExternalCommand::MapProperty(mapping)),
    )
    .await
}

async fn validate_mappings(State(state): State<AppState>) -> ApiResult {
    execute(&state, Command::External(ExternalCommand::ValidateMappings)).await
}

async fn submit_job(State(state): State<AppState>, Json(request): Json<JobRequest>) -> ApiResult {
    execute(&state, Command::Job(JobCommand::Submit(request))).await
}

async fn list_jobs(State(state): State<AppState>) -> ApiResult {
    execute(&state, Command::Job(JobCommand::List)).await
}

async fn get_job(State(state): State<AppState>, Path(id): Path<String>) -> ApiResult {
    execute(&state, Command::Job(JobCommand::Get { id: parse_id(&id)? })).await
}

async fn job_results(State(state): State<AppState>, Path(id): Path<String>) -> ApiResult {
    execute(
        &state,
        Command::Job(JobCommand::Results { id: parse_id(&id)? }),
    )
    .await
}

async fn cancel_job(State(state): State<AppState>, Path(id): Path<String>) -> ApiResult {
    execute(
        &state,
        Command::Job(JobCommand::Cancel { id: parse_id(&id)? }),
    )
    .await
}

async fn retry_job(State(state): State<AppState>, Path(id): Path<String>) -> ApiResult {
    execute(
        &state,
        Command::Job(JobCommand::Retry { id: parse_id(&id)? }),
    )
    .await
}
