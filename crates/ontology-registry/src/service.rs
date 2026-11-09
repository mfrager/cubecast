//! Registry service: ingestion, analysis, validation, publication, and search.

use std::collections::BTreeSet;
use std::sync::Arc;

use chrono::Utc;
use ontology_core::command::RegistryCommand;
use ontology_core::config::RegistryConfig;
use ontology_core::error::{Error, Result};
use ontology_core::format::RdfFormat;
use ontology_core::id::Id;
use ontology_core::model::{Quad, QuadPattern, Term};
use ontology_core::project::{AssetSummary, ReleaseSummary, SemanticAssetCatalog};
use ontology_core::rdf::RdfStore;
use ontology_core::registry::{
    Asset, Catalog, DiffReport, Distribution, ImportDependency, IngestRequest, Profile,
    RegistryResult, Release, ReleaseCandidate, ReleaseStatistics, SearchHit, SearchKind,
    SearchResult, TermSummary, ValidationReport,
};
use ontology_core::registry_graph::{release_graph, staging_graph};

use crate::analysis;
use crate::artifact::{ArtifactStore, FilesystemArtifactStore};
use crate::identity::{self, release_key, sha256_hex};
use crate::repository::{dcterms, RegistryRepository};
use crate::source;

/// Registry service over a shared RDF store and artifact store.
pub struct RegistryService {
    repository: RegistryRepository,
    artifacts: Arc<dyn ArtifactStore>,
    config: RegistryConfig,
}

impl RegistryService {
    /// Creates a service with a filesystem artifact store.
    ///
    /// # Errors
    ///
    /// Returns configuration or I/O errors from the artifact store.
    pub fn new(store: Arc<dyn RdfStore>, config: RegistryConfig) -> Result<Self> {
        let artifacts = Arc::new(FilesystemArtifactStore::new(&config.artifact_dir)?);
        Ok(Self {
            repository: RegistryRepository::new(store),
            artifacts,
            config,
        })
    }

    /// Creates a service with an injected artifact store.
    #[must_use]
    pub fn with_artifacts(
        store: Arc<dyn RdfStore>,
        artifacts: Arc<dyn ArtifactStore>,
        config: RegistryConfig,
    ) -> Self {
        Self {
            repository: RegistryRepository::new(store),
            artifacts,
            config,
        }
    }

    /// Returns the repository.
    #[must_use]
    pub fn repository(&self) -> &RegistryRepository {
        &self.repository
    }

    /// Returns the configuration.
    #[must_use]
    pub fn config(&self) -> &RegistryConfig {
        &self.config
    }

    /// Executes one registry command.
    pub async fn execute(&self, command: &RegistryCommand) -> Result<RegistryResult> {
        match command {
            RegistryCommand::CreateCatalog { title, description } => self
                .create_catalog(title, description.as_deref())
                .map(RegistryResult::Catalog),
            RegistryCommand::GetCatalog { id } => self.get_catalog(id).map(RegistryResult::Catalog),
            RegistryCommand::ListCatalogs => {
                let items = self.list_catalogs()?;
                let count = items.len();
                Ok(RegistryResult::Catalogs { items, count })
            }
            RegistryCommand::RegisterAsset {
                catalog,
                asset_iri,
                title,
                description,
                namespace,
            } => self
                .register_asset(
                    catalog.as_ref(),
                    asset_iri,
                    title.as_deref(),
                    description.as_deref(),
                    namespace.as_deref(),
                )
                .map(RegistryResult::Asset),
            RegistryCommand::GetAsset { key } => self.get_asset(key).map(RegistryResult::Asset),
            RegistryCommand::ListAssets { catalog } => {
                let items = self.list_assets(catalog.as_ref())?;
                let count = items.len();
                Ok(RegistryResult::Assets { items, count })
            }
            RegistryCommand::Ingest(request) => {
                self.ingest(request).await.map(RegistryResult::Candidate)
            }
            RegistryCommand::GetCandidate { id } => {
                self.get_candidate(id).map(RegistryResult::Candidate)
            }
            RegistryCommand::ListCandidates { asset } => {
                let items = self.list_candidates(asset.as_ref())?;
                let count = items.len();
                Ok(RegistryResult::Candidates { items, count })
            }
            RegistryCommand::Analyze { candidate } => {
                self.analyze(candidate).map(RegistryResult::Candidate)
            }
            RegistryCommand::Validate { candidate, profile } => self
                .validate(candidate, profile.as_ref())
                .map(RegistryResult::Validation),
            RegistryCommand::Publish { candidate, channel } => self
                .publish(candidate, channel.as_deref())
                .map(RegistryResult::Release),
            RegistryCommand::ListReleases { asset } => {
                let items = self.list_releases_for(asset.as_deref())?;
                let count = items.len();
                Ok(RegistryResult::Releases { items, count })
            }
            RegistryCommand::GetRelease { key } => {
                self.get_release(key).map(RegistryResult::Release)
            }
            RegistryCommand::ReleaseTerms {
                release,
                query,
                limit,
                offset,
            } => {
                let (release, items, count) =
                    self.release_terms(release, query.as_deref(), *limit, *offset)?;
                Ok(RegistryResult::Terms {
                    release,
                    items,
                    count,
                })
            }
            RegistryCommand::GetTerm { release, term } => {
                self.get_term(release, term).map(RegistryResult::Term)
            }
            RegistryCommand::ReleaseImports { release } => {
                let (release, items) = self.imports(release)?;
                Ok(RegistryResult::Imports { release, items })
            }
            RegistryCommand::ReleaseStatistics { release } => {
                let (release, statistics) = self.statistics(release)?;
                Ok(RegistryResult::Statistics {
                    release,
                    statistics,
                })
            }
            RegistryCommand::Diff { left, right } => {
                self.diff(left, right).map(RegistryResult::Diff)
            }
            RegistryCommand::Search {
                query,
                kind,
                limit,
                offset,
            } => self
                .search(query, kind.as_deref(), *limit, *offset)
                .map(RegistryResult::Search),
            RegistryCommand::ArchiveRelease { key, successor } => {
                self.archive(key, successor.as_deref())
            }
            RegistryCommand::ListReports { release } => {
                let published = self.repository.get_release(release)?;
                let items = self.reports(release)?;
                Ok(RegistryResult::Reports {
                    release: published.key,
                    items,
                })
            }
            RegistryCommand::CreateProfile {
                id,
                title,
                description,
            } => self
                .create_profile(id.clone(), title, description.as_deref())
                .map(RegistryResult::Profile),
            RegistryCommand::ListProfiles => {
                let items = self.list_profiles()?;
                let count = items.len();
                Ok(RegistryResult::Profiles { items, count })
            }
        }
    }

    /// Reads one distribution artifact for download.
    pub fn download(&self, release: &str, format: &str) -> Result<(String, Vec<u8>)> {
        let release = self.repository.get_release(release)?;
        let distribution = release
            .distributions
            .iter()
            .find(|distribution| distribution.format.eq_ignore_ascii_case(format))
            .ok_or_else(|| Error::NotFound {
                kind: "distribution",
                id: format.to_owned(),
            })?;
        Ok((
            distribution.media_type.clone(),
            self.artifacts.read(&distribution.sha256)?,
        ))
    }

    /// Creates a catalog.
    pub fn create_catalog(&self, title: &str, description: Option<&str>) -> Result<Catalog> {
        self.repository.create_catalog(title, description)
    }

    /// Retrieves a catalog.
    pub fn get_catalog(&self, id: &Id) -> Result<Catalog> {
        self.repository.get_catalog(id)
    }

    /// Lists catalogs.
    pub fn list_catalogs(&self) -> Result<Vec<Catalog>> {
        self.repository.list_catalogs()
    }

    /// Registers an asset.
    pub fn register_asset(
        &self,
        catalog: Option<&Id>,
        asset_iri: &Id,
        title: Option<&str>,
        description: Option<&str>,
        namespace: Option<&str>,
    ) -> Result<Asset> {
        self.repository
            .register_asset(catalog, asset_iri, title, description, namespace)
    }

    /// Retrieves an asset by key or IRI.
    pub fn get_asset(&self, value: &str) -> Result<Asset> {
        self.repository.get_asset(value)
    }

    /// Lists assets.
    pub fn list_assets(&self, catalog: Option<&Id>) -> Result<Vec<Asset>> {
        self.repository.list_assets(catalog)
    }

    /// Ingests a source into a candidate.
    ///
    /// # Errors
    ///
    /// Propagates source, parse, identity, conflict, and storage failures.
    pub async fn ingest(&self, request: &IngestRequest) -> Result<ReleaseCandidate> {
        let document = source::resolve(request, &self.config).await?;
        let parsed =
            ontology_rdf::parse_quads(document.format, &document.bytes, document.base.as_deref())?;
        let family = identity::family(&parsed, request.asset_iri.as_ref())?;
        let version = identity::version(&parsed, &family, request.version.as_deref())?;
        let key = release_key(&family, &version);
        let version_iri = identity::version_iri(&parsed, &family, &key)?;
        let source_hash = sha256_hex(&document.bytes);
        let content_hash = identity::content_hash(&parsed);

        if let Some(existing) = self.repository.find_release(&family, &version)? {
            if existing.source_hash == source_hash {
                return Ok(ReleaseCandidate {
                    id: existing.id.clone(),
                    asset: existing.asset,
                    version: existing.version,
                    version_iri: existing.id,
                    release_key: existing.key,
                    format: existing.format,
                    source_hash: existing.source_hash,
                    content_hash: existing.content_hash,
                    status: "published".to_owned(),
                    title: existing.title,
                    description: existing.description,
                    source: request.source.clone(),
                    statistics: existing.statistics,
                    imports: existing.imports,
                    created_at: existing.published_at,
                });
            }
            return Err(Error::Conflict {
                kind: "release",
                id: format!("{} {}", family, version),
            });
        }
        if let Some(existing) = self
            .repository
            .list_candidates(Some(&family))?
            .into_iter()
            .find(|candidate| candidate.release_key == key)
        {
            if existing.source_hash == source_hash {
                return Ok(existing);
            }
            return Err(Error::Conflict {
                kind: "release candidate",
                id: format!("{} {}", family, version),
            });
        }

        let candidate_id = Id::new(format!("urn:onto:registry:candidate:{}", ulid::Ulid::new()))?;
        let staging = staging_graph(&suffix(&candidate_id))?;
        let staged: Vec<Quad> = parsed
            .iter()
            .cloned()
            .map(|quad| quad.with_graph(staging.clone()))
            .collect();
        self.repository.store().insert(&staged)?;
        let analysis = analysis::analyze(&parsed);
        let imports = self.resolve_imports(&analysis.imports, &analysis)?;
        let title = request
            .title
            .clone()
            .unwrap_or_else(|| identity::title(&parsed, &family));
        let description = parsed
            .iter()
            .filter(|quad| quad.subject.as_iri() == Some(&family))
            .find_map(|quad| match &quad.object {
                Term::Literal(value) if quad.predicate == dcterms("description") => {
                    Some(value.value.clone())
                }
                _ => None,
            });
        let asset = match self.repository.get_asset_by_iri(&family) {
            Ok(asset) => asset,
            Err(Error::NotFound { .. }) => self.repository.register_asset(
                request.catalog.as_ref(),
                &family,
                Some(&title),
                description.as_deref(),
                None,
            )?,
            Err(error) => return Err(error),
        };
        let candidate = ReleaseCandidate {
            id: candidate_id.clone(),
            asset: asset.id,
            version,
            version_iri,
            release_key: key.clone(),
            format: document.format.as_str().to_owned(),
            source_hash: source_hash.clone(),
            content_hash,
            status: "ready".to_owned(),
            title,
            description,
            source: document.source,
            statistics: analysis.statistics,
            imports,
            created_at: Utc::now(),
        };
        self.repository.insert_candidate(&candidate)?;
        self.repository
            .replace_terms(&suffix(&candidate_id), &analysis.terms)?;
        let artifact = self
            .artifacts
            .put(&document.bytes, document.filename.as_deref())?;
        self.repository
            .insert_artifact_ref(&candidate_id, &artifact)?;
        Ok(candidate)
    }

    fn resolve_imports(
        &self,
        imports: &[Id],
        analysis: &analysis::Analysis,
    ) -> Result<Vec<ImportDependency>> {
        let mut values = Vec::new();
        for iri in imports {
            let resolved = self.repository.get_release(iri.as_str()).ok().or_else(|| {
                self.repository
                    .get_asset_by_iri(iri)
                    .ok()
                    .and_then(|asset| self.repository.current_release(&asset.id).ok().flatten())
                    .and_then(|id| self.repository.get_release_by_iri(&id).ok())
            });
            values.push(ImportDependency {
                iri: iri.clone(),
                version: resolved.as_ref().map(|release| release.version.clone()),
                resolved: resolved.is_some(),
                resolved_release: resolved.map(|release| release.id),
            });
        }
        let _ = analysis;
        Ok(values)
    }

    /// Retrieves a candidate.
    pub fn get_candidate(&self, id: &Id) -> Result<ReleaseCandidate> {
        self.repository.get_candidate(id)
    }

    /// Lists candidates.
    pub fn list_candidates(&self, asset: Option<&Id>) -> Result<Vec<ReleaseCandidate>> {
        self.repository.list_candidates(asset)
    }

    /// Recomputes candidate analysis.
    pub fn analyze(&self, id: &Id) -> Result<ReleaseCandidate> {
        let mut candidate = self.repository.get_candidate(id)?;
        let graph = self.repository.staging_graph(id)?;
        let quads = self
            .repository
            .store()
            .get(QuadPattern::new().with_graph(graph))?;
        let analysis = analysis::analyze(&quads);
        let imports = self.resolve_imports(&analysis.imports, &analysis)?;
        candidate.statistics = analysis.statistics;
        candidate.content_hash = analysis.content_hash.clone();
        candidate.imports = imports;
        candidate.status = "ready".to_owned();
        self.repository.insert_candidate(&candidate)?;
        self.repository
            .replace_terms(&suffix(id), &analysis.terms)?;
        Ok(candidate)
    }

    /// Runs structure validation and stores the report.
    pub fn validate(&self, id: &Id, profile: Option<&Id>) -> Result<ValidationReport> {
        let candidate = self.repository.get_candidate(id)?;
        let graph = self.repository.staging_graph(id)?;
        let quads = self
            .repository
            .store()
            .get(QuadPattern::new().with_graph(graph))?;
        let report = analysis::validate(
            &candidate,
            &quads,
            self.config.require_resolved_imports,
            profile.is_some(),
        );
        self.repository.insert_validation(&report, id)?;
        Ok(report)
    }

    /// Publishes a candidate.
    pub fn publish(&self, id: &Id, channel: Option<&str>) -> Result<Release> {
        let candidate = self.repository.get_candidate(id)?;
        if let Some(existing) = self
            .repository
            .find_release(&candidate.asset, &candidate.version)?
        {
            if existing.source_hash == candidate.source_hash {
                self.repository
                    .delete_graph(&self.repository.staging_graph(id)?)?;
                return Ok(existing);
            }
            return Err(Error::Conflict {
                kind: "release",
                id: format!("{} {}", candidate.asset, candidate.version),
            });
        }
        let report = self.validate(id, None)?;
        if !report.conforms {
            return Err(Error::Validation {
                detail: format!(
                    "candidate {id} has {} blocking validation issue(s)",
                    report
                        .issues
                        .iter()
                        .filter(|issue| issue.severity == "error")
                        .count()
                ),
            });
        }
        let graph = release_graph(&candidate.release_key)?;
        let staging = self.repository.staging_graph(id)?;
        let quads = self
            .repository
            .store()
            .get(QuadPattern::new().with_graph(staging.clone()))?;
        let release_quads: Vec<Quad> = quads
            .into_iter()
            .map(|quad| quad.with_graph(graph.clone()))
            .collect();
        self.repository.store().insert(&release_quads)?;

        let mut distributions = Vec::new();
        for format in [
            RdfFormat::Turtle,
            RdfFormat::NTriples,
            RdfFormat::NQuads,
            RdfFormat::RdfXml,
            RdfFormat::JsonLd,
        ] {
            let data = self.repository.store().serialize(format, Some(&graph))?;
            let artifact = self.artifacts.put(data.as_bytes(), None)?;
            let url = format!(
                "{}/api/v1/registry/download/{}/{}",
                self.config.public_base_url.trim_end_matches('/'),
                candidate.release_key,
                format.as_str()
            );
            distributions.push(Distribution {
                id: Id::new(format!(
                    "urn:onto:registry:distribution:{}:{}",
                    candidate.release_key,
                    format.as_str()
                ))?,
                format: format.as_str().to_owned(),
                media_type: format.media_type().to_owned(),
                sha256: artifact.sha256,
                byte_size: artifact.byte_size,
                url,
            });
        }
        let release = Release {
            id: candidate.version_iri.clone(),
            key: candidate.release_key.clone(),
            asset: candidate.asset.clone(),
            version: candidate.version.clone(),
            title: candidate.title.clone(),
            description: candidate.description.clone(),
            format: candidate.format.clone(),
            source_hash: candidate.source_hash.clone(),
            content_hash: candidate.content_hash.clone(),
            channel: channel.unwrap_or("stable").to_owned(),
            published_at: Utc::now(),
            graph: graph.to_string(),
            statistics: candidate.statistics.clone(),
            imports: candidate.imports.clone(),
            distributions,
        };
        self.repository.insert_release(&release)?;
        self.repository.link_validation(&release.id, &report.id)?;
        self.repository
            .set_current_release(&release.asset, Some(&release.id))?;
        let terms = self.repository.terms(&suffix(id))?;
        self.repository.replace_terms(&release.key, &terms)?;
        self.repository.delete_graph(&staging)?;
        Ok(release)
    }

    /// Retrieves a release by key or IRI.
    pub fn get_release(&self, value: &str) -> Result<Release> {
        self.repository.get_release(value)
    }

    /// Lists releases, accepting an asset key or family IRI as the filter.
    pub fn list_releases_for(&self, asset: Option<&str>) -> Result<Vec<Release>> {
        let asset = asset
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(|value| self.repository.get_asset(value).map(|asset| asset.id))
            .transpose()?;
        self.list_releases(asset.as_ref())
    }

    /// Lists releases for a family IRI.
    pub fn list_releases(&self, asset: Option<&Id>) -> Result<Vec<Release>> {
        self.repository.list_releases(asset)
    }

    /// Searches terms in a release.
    pub fn release_terms(
        &self,
        release: &str,
        query: Option<&str>,
        limit: usize,
        offset: usize,
    ) -> Result<(String, Vec<TermSummary>, usize)> {
        let release = self.repository.get_release(release)?;
        let all = self.repository.terms(&release.key)?;
        let filtered: Vec<TermSummary> =
            all.into_iter()
                .filter(|term| {
                    query.is_none_or(|query| {
                        let query = query.to_lowercase();
                        term.iri.as_str().to_lowercase().contains(&query)
                            || term
                                .label
                                .as_deref()
                                .is_some_and(|label| label.to_lowercase().contains(&query))
                            || term.definition.as_deref().is_some_and(|definition| {
                                definition.to_lowercase().contains(&query)
                            })
                    })
                })
                .collect();
        let count = filtered.len();
        let items = filtered.into_iter().skip(offset).take(limit).collect();
        Ok((release.key, items, count))
    }

    /// Retrieves one term.
    pub fn get_term(&self, release: &str, term: &str) -> Result<TermSummary> {
        let release = self.repository.get_release(release)?;
        self.repository.find_term(&release.key, term)
    }

    /// Returns imports for a release.
    pub fn imports(&self, release: &str) -> Result<(String, Vec<ImportDependency>)> {
        let release = self.repository.get_release(release)?;
        Ok((release.key.clone(), release.imports))
    }

    /// Returns statistics for a release.
    pub fn statistics(&self, release: &str) -> Result<(String, ReleaseStatistics)> {
        let release = self.repository.get_release(release)?;
        Ok((release.key.clone(), release.statistics))
    }

    /// Compares two releases by indexed classes and properties.
    pub fn diff(&self, left: &str, right: &str) -> Result<DiffReport> {
        let left = self.repository.get_release(left)?;
        let right = self.repository.get_release(right)?;
        let left_terms = self.repository.terms(&left.key)?;
        let right_terms = self.repository.terms(&right.key)?;
        let left_classes: BTreeSet<Id> = left_terms
            .iter()
            .filter(|term| term.kind == "class")
            .map(|term| term.iri.clone())
            .collect();
        let right_classes: BTreeSet<Id> = right_terms
            .iter()
            .filter(|term| term.kind == "class")
            .map(|term| term.iri.clone())
            .collect();
        let left_properties: BTreeSet<Id> = left_terms
            .iter()
            .filter(|term| term.kind.ends_with("property"))
            .map(|term| term.iri.clone())
            .collect();
        let right_properties: BTreeSet<Id> = right_terms
            .iter()
            .filter(|term| term.kind.ends_with("property"))
            .map(|term| term.iri.clone())
            .collect();
        let added_classes: Vec<Id> = right_classes.difference(&left_classes).cloned().collect();
        let removed_classes: Vec<Id> = left_classes.difference(&right_classes).cloned().collect();
        let added_properties: Vec<Id> = right_properties
            .difference(&left_properties)
            .cloned()
            .collect();
        let removed_properties: Vec<Id> = left_properties
            .difference(&right_properties)
            .cloned()
            .collect();
        let mut changed_terms = Vec::new();
        for left_term in &left_terms {
            if let Some(right_term) = right_terms.iter().find(|term| term.iri == left_term.iri) {
                if left_term.label != right_term.label
                    || left_term.definition != right_term.definition
                {
                    changed_terms.push(left_term.iri.clone());
                }
            }
        }
        let breaking = !removed_classes.is_empty() || !removed_properties.is_empty();
        let compatible = !breaking;
        let summary = if breaking {
            format!(
                "breaking: {} class(es) and {} property(ies) removed; {} changed",
                removed_classes.len(),
                removed_properties.len(),
                changed_terms.len()
            )
        } else {
            format!(
                "compatible: {} class(es) and {} property(ies) added",
                added_classes.len(),
                added_properties.len()
            )
        };
        Ok(DiffReport {
            left: left.key,
            right: right.key,
            added_classes,
            removed_classes,
            added_properties,
            removed_properties,
            changed_terms,
            breaking,
            compatible,
            summary,
        })
    }

    /// Archives a release and selects a successor where supplied.
    pub fn archive(&self, value: &str, successor: Option<&str>) -> Result<RegistryResult> {
        let release = self.repository.get_release(value)?;
        self.repository.set_channel(&release.id, "archived")?;
        let successor = successor
            .map(|value| self.repository.get_release(value))
            .transpose()?;
        if let Some(successor) = successor {
            if successor.asset != release.asset {
                return Err(Error::validation(
                    "archive successor must belong to the same asset family",
                ));
            }
            self.repository
                .set_current_release(&release.asset, Some(&successor.id))?;
        } else if self.repository.current_release(&release.asset)?.as_ref() == Some(&release.id) {
            self.repository.set_current_release(&release.asset, None)?;
        }
        Ok(RegistryResult::Archived {
            release: release.key,
            channel: "archived".to_owned(),
        })
    }

    /// Creates a profile.
    pub fn create_profile(
        &self,
        id: Id,
        title: &str,
        description: Option<&str>,
    ) -> Result<Profile> {
        let profile = Profile {
            id,
            title: title.to_owned(),
            description: description.map(ToOwned::to_owned),
            shapes_hash: None,
            created_at: Utc::now(),
        };
        self.repository.create_profile(&profile)?;
        Ok(profile)
    }

    /// Lists profiles.
    pub fn list_profiles(&self) -> Result<Vec<Profile>> {
        self.repository.list_profiles()
    }

    /// Lists validation reports for a release or candidate.
    pub fn reports(&self, value: &str) -> Result<Vec<ValidationReport>> {
        let id = if value.starts_with("urn:") {
            Id::new(value)?
        } else {
            self.repository.get_release(value)?.id
        };
        self.repository.validation_reports(&id)
    }

    /// Searches catalogs, assets, releases, terms, namespaces, and imports.
    pub fn search(
        &self,
        query: &str,
        kind: Option<&str>,
        limit: usize,
        offset: usize,
    ) -> Result<SearchResult> {
        let kind = SearchKind::parse(kind).ok_or_else(|| {
            Error::invalid("search kind must be catalog, term, namespace, dependency, or all")
        })?;
        let needle = query.trim().to_lowercase();
        let mut hits = Vec::new();
        if matches!(kind, SearchKind::All | SearchKind::Catalog) {
            for catalog in self.repository.list_catalogs()? {
                let haystack = format!("{} {}", catalog.id, catalog.title).to_lowercase();
                if haystack.contains(&needle) {
                    hits.push(SearchHit {
                        kind: "catalog".to_owned(),
                        key: catalog.id.to_string(),
                        iri: catalog.id,
                        label: catalog.title,
                        description: catalog.description,
                        version: None,
                    });
                }
            }
            for asset in self.repository.list_assets(None)? {
                let haystack = format!(
                    "{} {} {}",
                    asset.id,
                    asset.title,
                    asset.namespace.clone().unwrap_or_default()
                )
                .to_lowercase();
                if haystack.contains(&needle) {
                    hits.push(SearchHit {
                        kind: "asset".to_owned(),
                        key: asset.key,
                        iri: asset.id,
                        label: asset.title,
                        description: asset.description,
                        version: None,
                    });
                }
            }
            for release in self.repository.list_releases(None)? {
                let haystack =
                    format!("{} {} {}", release.id, release.title, release.version).to_lowercase();
                if haystack.contains(&needle) {
                    hits.push(SearchHit {
                        kind: "release".to_owned(),
                        key: release.key,
                        iri: release.id,
                        label: release.title,
                        description: release.description,
                        version: Some(release.version),
                    });
                }
            }
        }
        if matches!(kind, SearchKind::All | SearchKind::Namespace) {
            for asset in self.repository.list_assets(None)? {
                if let Some(namespace) = asset.namespace {
                    if namespace.to_lowercase().contains(&needle) {
                        hits.push(SearchHit {
                            kind: "namespace".to_owned(),
                            key: asset.key,
                            iri: asset.id,
                            label: namespace,
                            description: Some(asset.title),
                            version: None,
                        });
                    }
                }
            }
        }
        if matches!(kind, SearchKind::All | SearchKind::Term) {
            for release in self.repository.list_releases(None)? {
                for term in self.repository.terms(&release.key)? {
                    let haystack = format!(
                        "{} {} {}",
                        term.iri,
                        term.label.clone().unwrap_or_default(),
                        term.definition.clone().unwrap_or_default()
                    )
                    .to_lowercase();
                    if haystack.contains(&needle) {
                        hits.push(SearchHit {
                            kind: "term".to_owned(),
                            key: format!("{}:{}", release.key, term.key),
                            iri: term.iri,
                            label: term.label.unwrap_or_else(|| term.key.clone()),
                            description: term.definition,
                            version: Some(release.version.clone()),
                        });
                    }
                }
            }
        }
        if matches!(kind, SearchKind::All | SearchKind::Dependency) {
            for release in self.repository.list_releases(None)? {
                for import in release.imports {
                    let haystack = format!(
                        "{} {}",
                        import.iri,
                        import.version.clone().unwrap_or_default()
                    )
                    .to_lowercase();
                    if haystack.contains(&needle) {
                        hits.push(SearchHit {
                            kind: "dependency".to_owned(),
                            key: release.key.clone(),
                            iri: import.iri,
                            label: import
                                .version
                                .unwrap_or_else(|| "unresolved import".to_owned()),
                            description: Some(format!("imported by {}", release.id)),
                            version: Some(release.version.clone()),
                        });
                    }
                }
            }
        }
        hits.sort_by(|left, right| {
            left.kind
                .cmp(&right.kind)
                .then(left.label.cmp(&right.label))
                .then(left.iri.cmp(&right.iri))
        });
        let count = hits.len();
        let items = hits.into_iter().skip(offset).take(limit).collect();
        Ok(SearchResult {
            items,
            count,
            limit,
            offset,
        })
    }

    /// Returns a compact asset summary for the project layer.
    pub fn asset_summary(&self, id: &Id) -> Result<AssetSummary> {
        let asset = self.repository.get_asset_by_iri(id)?;
        Ok(AssetSummary {
            id: asset.id,
            key: asset.key,
            title: asset.title,
        })
    }

    /// Returns a compact release summary for the project layer.
    pub fn release_summary(&self, id: &Id) -> Result<ReleaseSummary> {
        let release = self
            .repository
            .get_release_by_iri(id)
            .or_else(|_| self.repository.get_release(id.as_str()))?;
        let validated = self
            .repository
            .validation_reports(&release.id)?
            .iter()
            .any(|report| report.conforms);
        Ok(ReleaseSummary {
            id: release.id,
            key: release.key,
            asset: release.asset,
            version: release.version,
            channel: release.channel,
            validated,
        })
    }
}

#[async_trait::async_trait]
impl SemanticAssetCatalog for RegistryService {
    async fn asset(&self, id: &Id) -> Result<AssetSummary> {
        self.asset_summary(id)
    }

    async fn release(&self, id: &Id) -> Result<ReleaseSummary> {
        self.release_summary(id)
    }

    async fn release_by_reference(&self, value: &str) -> Result<ReleaseSummary> {
        let release = self.repository.get_release(value)?;
        self.release_summary(&release.id)
    }

    async fn releases_for_asset(&self, id: &Id) -> Result<Vec<ReleaseSummary>> {
        let releases = self.repository.list_releases(Some(id))?;
        Ok(releases
            .into_iter()
            .map(|release| ReleaseSummary {
                id: release.id,
                key: release.key,
                asset: release.asset,
                version: release.version,
                channel: release.channel,
                validated: false,
            })
            .collect())
    }

    async fn validation_reports(&self, release: &Id) -> Result<Vec<ValidationReport>> {
        self.repository.validation_reports(release)
    }
}

fn suffix(id: &Id) -> String {
    id.as_str()
        .rsplit(':')
        .next()
        .unwrap_or(id.as_str())
        .to_owned()
}

/// Converts a registry result into the shared result family.
#[must_use]
pub fn result(value: RegistryResult) -> ontology_core::command::CommandResult {
    ontology_core::command::CommandResult::Registry(value)
}
