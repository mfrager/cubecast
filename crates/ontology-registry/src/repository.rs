//! RDF repository for catalog metadata, releases, terms, and reports.

use std::sync::Arc;

use crate::identity::{asset_key, hash_key, release_key, term_key};
use chrono::{DateTime, Utc};
use ontology_core::error::{Error, Result};
use ontology_core::id::Id;
use ontology_core::model::{GraphName, Literal, Quad, QuadPattern, Term};
use ontology_core::rdf::RdfStore;
use ontology_core::registry::{
    ArtifactRef, Asset, Catalog, Distribution, ImportDependency, Profile, Release,
    ReleaseCandidate, ReleaseStatistics, TermSummary, ValidationIssue, ValidationReport,
};
use ontology_core::registry_graph::{
    analysis_graph, catalog_graph, staging_graph, validation_graph,
};

pub(crate) const RDF: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#";
pub(crate) const SKOS: &str = "http://www.w3.org/2004/02/skos/core#";
pub(crate) const DCAT: &str = "http://www.w3.org/ns/dcat#";
pub(crate) const ADMS: &str = "http://www.w3.org/ns/adms#";
pub(crate) const DCTERMS: &str = "http://purl.org/dc/terms/";
pub(crate) const PROV: &str = "http://www.w3.org/ns/prov#";
pub(crate) const VOID: &str = "http://rdfs.org/ns/void#";
pub(crate) const PROF: &str = "http://www.w3.org/ns/prof#";
pub(crate) const SH: &str = "http://www.w3.org/ns/shacl#";
pub(crate) const OREG: &str = "urn:onto:registry:";
pub(crate) const DEFAULT_CATALOG: &str = "urn:onto:registry:catalog";

pub(crate) fn id(namespace: &str, local: &str) -> Id {
    Id::new(format!("{namespace}{local}")).expect("well-known vocabulary IRIs are valid")
}

pub(crate) fn rdf_type() -> Id {
    id(RDF, "type")
}

pub(crate) fn dcat(local: &str) -> Id {
    id(DCAT, local)
}

pub(crate) fn adms(local: &str) -> Id {
    id(ADMS, local)
}

pub(crate) fn dcterms(local: &str) -> Id {
    id(DCTERMS, local)
}

pub(crate) fn prov(local: &str) -> Id {
    id(PROV, local)
}

pub(crate) fn void(local: &str) -> Id {
    id(VOID, local)
}

pub(crate) fn prof(local: &str) -> Id {
    id(PROF, local)
}

pub(crate) fn sh(local: &str) -> Id {
    id(SH, local)
}

pub(crate) fn oreg(local: &str) -> Id {
    id(OREG, local)
}

pub(crate) fn skos(local: &str) -> Id {
    id(SKOS, local)
}

fn literal(quads: &[Quad], predicate: &Id) -> Option<String> {
    quads
        .iter()
        .find(|quad| &quad.predicate == predicate)
        .and_then(|quad| match &quad.object {
            Term::Literal(value) => Some(value.value.clone()),
            _ => None,
        })
}

fn iri(quads: &[Quad], predicate: &Id) -> Option<Id> {
    quads
        .iter()
        .find(|quad| &quad.predicate == predicate)
        .and_then(|quad| quad.object.as_iri().cloned())
}

fn iris(quads: &[Quad], predicate: &Id) -> Vec<Id> {
    let mut values: Vec<Id> = quads
        .iter()
        .filter(|quad| &quad.predicate == predicate)
        .filter_map(|quad| quad.object.as_iri().cloned())
        .collect();
    values.sort();
    values.dedup();
    values
}

fn integer(quads: &[Quad], predicate: &Id) -> usize {
    literal(quads, predicate)
        .and_then(|value| value.parse().ok())
        .unwrap_or_default()
}

fn boolean(quads: &[Quad], predicate: &Id, default: bool) -> bool {
    match literal(quads, predicate) {
        Some(value) => matches!(value.trim().to_ascii_lowercase().as_str(), "true" | "1"),
        None => default,
    }
}

fn datetime(quads: &[Quad], predicate: &Id) -> DateTime<Utc> {
    quads
        .iter()
        .find(|quad| &quad.predicate == predicate)
        .and_then(|quad| match &quad.object {
            Term::Literal(value) => DateTime::parse_from_rfc3339(&value.value).ok(),
            _ => None,
        })
        .map_or_else(Utc::now, |value| value.with_timezone(&Utc))
}

fn typed(subject: &Id, class: &Id, graph: &GraphName) -> Quad {
    Quad::in_graph(
        Term::Iri(subject.clone()),
        rdf_type(),
        Term::Iri(class.clone()),
        graph.clone(),
    )
}

fn string(subject: &Id, predicate: &Id, value: &str, graph: &GraphName) -> Quad {
    Quad::in_graph(
        Term::Iri(subject.clone()),
        predicate.clone(),
        Term::string(value),
        graph.clone(),
    )
}

fn iri_quad(subject: &Id, predicate: &Id, object: &Id, graph: &GraphName) -> Quad {
    Quad::in_graph(
        Term::Iri(subject.clone()),
        predicate.clone(),
        Term::Iri(object.clone()),
        graph.clone(),
    )
}

fn int_quad(subject: &Id, predicate: &Id, value: usize, graph: &GraphName) -> Quad {
    Quad::in_graph(
        Term::Iri(subject.clone()),
        predicate.clone(),
        Term::Literal(Literal::integer(value as i64)),
        graph.clone(),
    )
}

fn bool_quad(subject: &Id, predicate: &Id, value: bool, graph: &GraphName) -> Quad {
    Quad::in_graph(
        Term::Iri(subject.clone()),
        predicate.clone(),
        Term::Literal(Literal::boolean(value)),
        graph.clone(),
    )
}

fn time_quad(subject: &Id, predicate: &Id, value: &DateTime<Utc>, graph: &GraphName) -> Quad {
    Quad::in_graph(
        Term::Iri(subject.clone()),
        predicate.clone(),
        Term::Literal(Literal::datetime(value)),
        graph.clone(),
    )
}

fn suffix(id: &Id) -> String {
    id.as_str()
        .rsplit(':')
        .next()
        .unwrap_or(id.as_str())
        .to_owned()
}

fn typed_quads(quads: &[Quad], class: &Id) -> bool {
    quads
        .iter()
        .any(|quad| quad.predicate == rdf_type() && quad.object == Term::Iri(class.clone()))
}

/// RDF repository for the registry.
pub struct RegistryRepository {
    store: Arc<dyn RdfStore>,
    metadata_graph: GraphName,
    profile_graph: GraphName,
}

impl RegistryRepository {
    /// Creates a repository on the default catalog graph.
    #[must_use]
    pub fn new(store: Arc<dyn RdfStore>) -> Self {
        Self {
            store,
            metadata_graph: catalog_graph(),
            profile_graph: GraphName::Iri(id(OREG, "profiles")),
        }
    }

    /// Returns the underlying store.
    #[must_use]
    pub fn store(&self) -> &Arc<dyn RdfStore> {
        &self.store
    }

    /// Returns the metadata graph.
    #[must_use]
    pub fn metadata_graph(&self) -> &GraphName {
        &self.metadata_graph
    }

    fn subject(&self, subject: &Id, graph: &GraphName) -> Result<Vec<Quad>> {
        self.store.get(
            QuadPattern::new()
                .with_subject(Term::Iri(subject.clone()))
                .with_graph(graph.clone()),
        )
    }

    fn replace(&self, removals: Vec<Quad>, additions: Vec<Quad>) -> Result<()> {
        if let Ok(mut transaction) = self.store.transaction() {
            if !removals.is_empty() {
                transaction.delete(&removals)?;
            }
            if !additions.is_empty() {
                transaction.insert(&additions)?;
            }
            return transaction.commit();
        }
        if !removals.is_empty() {
            self.store.delete(&removals)?;
        }
        if !additions.is_empty() {
            self.store.insert(&additions)?;
        }
        Ok(())
    }

    fn replace_subject(&self, subject: &Id, graph: &GraphName, additions: Vec<Quad>) -> Result<()> {
        let removals = self.subject(subject, graph)?;
        self.replace(removals, additions)
    }

    fn replace_predicate(
        &self,
        subject: &Id,
        predicate: &Id,
        graph: &GraphName,
        additions: Vec<Quad>,
    ) -> Result<()> {
        let removals = self.store.get(
            QuadPattern::new()
                .with_subject(Term::Iri(subject.clone()))
                .with_predicate(predicate.clone())
                .with_graph(graph.clone()),
        )?;
        self.replace(removals, additions)
    }

    /// Finds a subject by a literal property.
    pub fn find_by_literal(
        &self,
        class: &Id,
        predicate: &Id,
        value: &str,
        graph: &GraphName,
    ) -> Result<Option<Id>> {
        let subjects = self.store.get(
            QuadPattern::new()
                .with_predicate(predicate.clone())
                .with_object(Term::string(value))
                .with_graph(graph.clone()),
        )?;
        for quad in subjects {
            let Some(subject) = quad.subject.as_iri() else {
                continue;
            };
            let quads = self.subject(subject, graph)?;
            if typed_quads(&quads, class) {
                return Ok(Some(subject.clone()));
            }
        }
        Ok(None)
    }

    /// Ensures the default catalog exists and returns it.
    pub fn ensure_default_catalog(&self) -> Result<Catalog> {
        let id = Id::new(DEFAULT_CATALOG)?;
        match self.get_catalog(&id) {
            Ok(catalog) => Ok(catalog),
            Err(Error::NotFound { .. }) => {
                let now = Utc::now();
                let additions = vec![
                    typed(&id, &dcat("Catalog"), &self.metadata_graph),
                    string(
                        &id,
                        &dcterms("title"),
                        "Registry catalog",
                        &self.metadata_graph,
                    ),
                    time_quad(&id, &dcterms("created"), &now, &self.metadata_graph),
                ];
                self.replace_subject(&id, &self.metadata_graph, additions)?;
                Ok(Catalog {
                    id,
                    title: "Registry catalog".to_owned(),
                    description: None,
                    created_at: now,
                })
            }
            Err(error) => Err(error),
        }
    }

    /// Creates a catalog.
    pub fn create_catalog(&self, title: &str, description: Option<&str>) -> Result<Catalog> {
        if title.trim().is_empty() {
            return Err(Error::validation("catalog title must not be empty"));
        }
        let id = Id::new(format!("urn:onto:registry:catalog:{}", ulid::Ulid::new()))?;
        let now = Utc::now();
        let mut additions = vec![
            typed(&id, &dcat("Catalog"), &self.metadata_graph),
            string(&id, &dcterms("title"), title.trim(), &self.metadata_graph),
            time_quad(&id, &dcterms("created"), &now, &self.metadata_graph),
        ];
        if let Some(description) = description.map(str::trim).filter(|value| !value.is_empty()) {
            additions.push(string(
                &id,
                &dcterms("description"),
                description,
                &self.metadata_graph,
            ));
        }
        self.replace_subject(&id, &self.metadata_graph, additions)?;
        Ok(Catalog {
            id,
            title: title.trim().to_owned(),
            description: description.map(ToOwned::to_owned),
            created_at: now,
        })
    }

    /// Retrieves a catalog.
    pub fn get_catalog(&self, id: &Id) -> Result<Catalog> {
        let quads = self.subject(id, &self.metadata_graph)?;
        if !typed_quads(&quads, &dcat("Catalog")) {
            return Err(Error::NotFound {
                kind: "catalog",
                id: id.to_string(),
            });
        }
        Ok(Catalog {
            id: id.clone(),
            title: literal(&quads, &dcterms("title")).unwrap_or_else(|| "Catalog".to_owned()),
            description: literal(&quads, &dcterms("description")),
            created_at: datetime(&quads, &dcterms("created")),
        })
    }

    /// Lists catalogs.
    pub fn list_catalogs(&self) -> Result<Vec<Catalog>> {
        let mut values = Vec::new();
        for id in self.ids_of_type(&dcat("Catalog"), &self.metadata_graph)? {
            if let Ok(value) = self.get_catalog(&id) {
                values.push(value);
            }
        }
        Ok(values)
    }

    fn ids_of_type(&self, class: &Id, graph: &GraphName) -> Result<Vec<Id>> {
        let quads = self.store.get(
            QuadPattern::new()
                .with_predicate(rdf_type())
                .with_object(Term::Iri(class.clone()))
                .with_graph(graph.clone()),
        )?;
        let mut ids: Vec<Id> = quads
            .into_iter()
            .filter_map(|quad| quad.subject.as_iri().cloned())
            .collect();
        ids.sort();
        ids.dedup();
        Ok(ids)
    }

    /// Registers or updates an asset.
    pub fn register_asset(
        &self,
        catalog: Option<&Id>,
        asset_iri: &Id,
        title: Option<&str>,
        description: Option<&str>,
        namespace: Option<&str>,
    ) -> Result<Asset> {
        let catalog = match catalog {
            Some(id) => self.get_catalog(id)?,
            None => self.ensure_default_catalog()?,
        };
        let key = asset_key(asset_iri);
        let now = Utc::now();
        let existing = self.get_asset_by_iri(asset_iri).ok();
        let created = existing.as_ref().map_or(now, |asset| asset.created_at);
        let title = title
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(ToOwned::to_owned)
            .or_else(|| existing.as_ref().map(|asset| asset.title.clone()))
            .unwrap_or_else(|| asset_iri.local_name().to_owned());
        let description = description.map(ToOwned::to_owned).or_else(|| {
            existing
                .as_ref()
                .and_then(|asset| asset.description.clone())
        });
        let namespace = namespace
            .map(ToOwned::to_owned)
            .or_else(|| existing.as_ref().and_then(|asset| asset.namespace.clone()))
            .or_else(|| {
                let value = asset_iri.namespace();
                (!value.is_empty()).then(|| value.to_owned())
            });
        let mut additions = vec![
            typed(asset_iri, &oreg("Ontology"), &self.metadata_graph),
            typed(asset_iri, &dcat("DatasetSeries"), &self.metadata_graph),
            iri_quad(
                asset_iri,
                &oreg("catalog"),
                &catalog.id,
                &self.metadata_graph,
            ),
            string(asset_iri, &oreg("assetKey"), &key, &self.metadata_graph),
            string(asset_iri, &dcterms("title"), &title, &self.metadata_graph),
            time_quad(
                asset_iri,
                &dcterms("created"),
                &created,
                &self.metadata_graph,
            ),
            time_quad(asset_iri, &dcterms("modified"), &now, &self.metadata_graph),
        ];
        if let Some(description) = &description {
            additions.push(string(
                asset_iri,
                &dcterms("description"),
                description,
                &self.metadata_graph,
            ));
        }
        if let Some(namespace) = &namespace {
            additions.push(string(
                asset_iri,
                &oreg("namespaceIRI"),
                namespace,
                &self.metadata_graph,
            ));
        }
        self.replace_subject(asset_iri, &self.metadata_graph, additions)?;
        Ok(Asset {
            id: asset_iri.clone(),
            key,
            catalog: catalog.id,
            title,
            description,
            namespace,
            created_at: created,
            updated_at: now,
        })
    }

    /// Retrieves an asset by key or IRI.
    pub fn get_asset(&self, value: &str) -> Result<Asset> {
        let id = if let Ok(id) = Id::new(value) {
            if self
                .subject(&id, &self.metadata_graph)?
                .iter()
                .any(|quad| typed_quads(std::slice::from_ref(quad), &oreg("Ontology")))
            {
                id
            } else {
                self.find_by_literal(
                    &oreg("Ontology"),
                    &oreg("assetKey"),
                    value,
                    &self.metadata_graph,
                )?
                .ok_or_else(|| Error::NotFound {
                    kind: "asset",
                    id: value.to_owned(),
                })?
            }
        } else {
            self.find_by_literal(
                &oreg("Ontology"),
                &oreg("assetKey"),
                value,
                &self.metadata_graph,
            )?
            .ok_or_else(|| Error::NotFound {
                kind: "asset",
                id: value.to_owned(),
            })?
        };
        self.get_asset_by_iri(&id)
    }

    /// Retrieves an asset by IRI.
    pub fn get_asset_by_iri(&self, id: &Id) -> Result<Asset> {
        let quads = self.subject(id, &self.metadata_graph)?;
        if !typed_quads(&quads, &oreg("Ontology")) {
            return Err(Error::NotFound {
                kind: "asset",
                id: id.to_string(),
            });
        }
        Ok(Asset {
            id: id.clone(),
            key: literal(&quads, &oreg("assetKey")).unwrap_or_else(|| asset_key(id)),
            catalog: iri(&quads, &oreg("catalog"))
                .unwrap_or_else(|| Id::new(DEFAULT_CATALOG).expect("default catalog")),
            title: literal(&quads, &dcterms("title")).unwrap_or_else(|| id.local_name().to_owned()),
            description: literal(&quads, &dcterms("description")),
            namespace: literal(&quads, &oreg("namespaceIRI")),
            created_at: datetime(&quads, &dcterms("created")),
            updated_at: datetime(&quads, &dcterms("modified")),
        })
    }

    /// Lists assets, optionally restricted to a catalog.
    pub fn list_assets(&self, catalog: Option<&Id>) -> Result<Vec<Asset>> {
        let mut values = Vec::new();
        for id in self.ids_of_type(&oreg("Ontology"), &self.metadata_graph)? {
            if let Ok(asset) = self.get_asset_by_iri(&id) {
                if catalog.is_none_or(|catalog| &asset.catalog == catalog) {
                    values.push(asset);
                }
            }
        }
        values.sort_by(|left, right| left.title.cmp(&right.title).then(left.id.cmp(&right.id)));
        Ok(values)
    }

    /// Stores a candidate, its statistics, and dependency records.
    pub fn insert_candidate(&self, candidate: &ReleaseCandidate) -> Result<()> {
        let mut additions = vec![
            typed(
                &candidate.id,
                &oreg("ReleaseCandidate"),
                &self.metadata_graph,
            ),
            typed(&candidate.id, &prov("Entity"), &self.metadata_graph),
            iri_quad(
                &candidate.id,
                &oreg("asset"),
                &candidate.asset,
                &self.metadata_graph,
            ),
            string(
                &candidate.id,
                &dcat("version"),
                &candidate.version,
                &self.metadata_graph,
            ),
            iri_quad(
                &candidate.id,
                &oreg("versionIRI"),
                &candidate.version_iri,
                &self.metadata_graph,
            ),
            string(
                &candidate.id,
                &oreg("releaseKey"),
                &candidate.release_key,
                &self.metadata_graph,
            ),
            string(
                &candidate.id,
                &dcterms("format"),
                &candidate.format,
                &self.metadata_graph,
            ),
            string(
                &candidate.id,
                &oreg("sourceHash"),
                &candidate.source_hash,
                &self.metadata_graph,
            ),
            string(
                &candidate.id,
                &oreg("contentHash"),
                &candidate.content_hash,
                &self.metadata_graph,
            ),
            string(
                &candidate.id,
                &oreg("status"),
                &candidate.status,
                &self.metadata_graph,
            ),
            string(
                &candidate.id,
                &dcterms("title"),
                &candidate.title,
                &self.metadata_graph,
            ),
            string(
                &candidate.id,
                &oreg("source"),
                &candidate.source,
                &self.metadata_graph,
            ),
            time_quad(
                &candidate.id,
                &prov("generatedAtTime"),
                &candidate.created_at,
                &self.metadata_graph,
            ),
        ];
        if let Some(description) = &candidate.description {
            additions.push(string(
                &candidate.id,
                &dcterms("description"),
                description,
                &self.metadata_graph,
            ));
        }
        additions.extend(stats_quads(
            &candidate.id,
            &candidate.statistics,
            &self.metadata_graph,
        ));
        additions.extend(import_quads(
            &candidate.id,
            &candidate.imports,
            &self.metadata_graph,
        ));
        self.replace_subject(&candidate.id, &self.metadata_graph, additions)
    }

    /// Retrieves a candidate.
    pub fn get_candidate(&self, id: &Id) -> Result<ReleaseCandidate> {
        let quads = self.subject(id, &self.metadata_graph)?;
        if !typed_quads(&quads, &oreg("ReleaseCandidate")) {
            return Err(Error::NotFound {
                kind: "release candidate",
                id: id.to_string(),
            });
        }
        Ok(ReleaseCandidate {
            id: id.clone(),
            asset: iri(&quads, &oreg("asset"))
                .ok_or_else(|| malformed("candidate has no asset"))?,
            version: literal(&quads, &dcat("version")).unwrap_or_default(),
            version_iri: iri(&quads, &oreg("versionIRI")).unwrap_or_else(|| id.clone()),
            release_key: literal(&quads, &oreg("releaseKey"))
                .unwrap_or_else(|| release_key(id, "")),
            format: literal(&quads, &dcterms("format")).unwrap_or_else(|| "turtle".to_owned()),
            source_hash: literal(&quads, &oreg("sourceHash")).unwrap_or_default(),
            content_hash: literal(&quads, &oreg("contentHash")).unwrap_or_default(),
            status: literal(&quads, &oreg("status")).unwrap_or_else(|| "ready".to_owned()),
            title: literal(&quads, &dcterms("title")).unwrap_or_else(|| id.local_name().to_owned()),
            description: literal(&quads, &dcterms("description")),
            source: literal(&quads, &oreg("source")).unwrap_or_default(),
            statistics: read_stats(&quads),
            imports: self.read_imports(id, &self.metadata_graph)?,
            created_at: datetime(&quads, &prov("generatedAtTime")),
        })
    }

    /// Lists candidates, optionally restricted to an asset.
    pub fn list_candidates(&self, asset: Option<&Id>) -> Result<Vec<ReleaseCandidate>> {
        let mut values = Vec::new();
        for id in self.ids_of_type(&oreg("ReleaseCandidate"), &self.metadata_graph)? {
            if let Ok(candidate) = self.get_candidate(&id) {
                if asset.is_none_or(|asset| &candidate.asset == asset) {
                    values.push(candidate);
                }
            }
        }
        values.sort_by_key(|right| std::cmp::Reverse(right.created_at));
        Ok(values)
    }

    /// Returns the staging graph for a candidate.
    pub fn staging_graph(&self, candidate: &Id) -> Result<GraphName> {
        staging_graph(&suffix(candidate))
    }

    /// Returns the analysis graph used by a candidate or release key.
    pub fn analysis_graph(&self, key: &str) -> Result<GraphName> {
        analysis_graph(key)
    }

    /// Stores terms in an analysis graph, replacing its previous records.
    pub fn replace_terms(&self, key: &str, terms: &[TermSummary]) -> Result<()> {
        let graph = analysis_graph(key)?;
        let existing = self
            .store
            .get(QuadPattern::new().with_graph(graph.clone()))?;
        let mut additions = Vec::with_capacity(terms.len());
        for term in terms {
            let subject = Id::new(format!("urn:onto:registry:term:{key}:{}", term.key))?;
            additions.push(typed(&subject, &oreg("OntologyTerm"), &graph));
            additions.push(iri_quad(&subject, &oreg("termIri"), &term.iri, &graph));
            additions.push(string(&subject, &oreg("termKind"), &term.kind, &graph));
            additions.push(string(&subject, &oreg("termKey"), &term.key, &graph));
            additions.push(bool_quad(
                &subject,
                &oreg("deprecated"),
                term.deprecated,
                &graph,
            ));
            if let Some(label) = &term.label {
                additions.push(string(&subject, &skos("prefLabel"), label, &graph));
            }
            if let Some(definition) = &term.definition {
                additions.push(string(&subject, &skos("definition"), definition, &graph));
            }
            if let Some(namespace) = &term.namespace {
                additions.push(string(&subject, &oreg("namespaceIRI"), namespace, &graph));
            }
        }
        self.replace(existing, additions)
    }

    /// Reads all terms for an analysis key.
    pub fn terms(&self, key: &str) -> Result<Vec<TermSummary>> {
        let graph = analysis_graph(key)?;
        let mut values = Vec::new();
        for id in self.ids_of_type(&oreg("OntologyTerm"), &graph)? {
            let quads = self.subject(&id, &graph)?;
            let Some(iri) = iri(&quads, &oreg("termIri")) else {
                continue;
            };
            values.push(TermSummary {
                key: literal(&quads, &oreg("termKey")).unwrap_or_else(|| term_key(&iri)),
                iri,
                kind: literal(&quads, &oreg("termKind")).unwrap_or_else(|| "class".to_owned()),
                label: literal(&quads, &skos("prefLabel")),
                definition: literal(&quads, &skos("definition")),
                namespace: literal(&quads, &oreg("namespaceIRI")),
                deprecated: boolean(&quads, &oreg("deprecated"), false),
            });
        }
        values.sort_by(|left, right| left.iri.cmp(&right.iri));
        Ok(values)
    }

    /// Stores a published release.
    pub fn insert_release(&self, release: &Release) -> Result<()> {
        let mut additions = vec![
            typed(&release.id, &oreg("OntologyRelease"), &self.metadata_graph),
            typed(&release.id, &dcat("Dataset"), &self.metadata_graph),
            typed(&release.id, &adms("Asset"), &self.metadata_graph),
            typed(&release.id, &prov("Entity"), &self.metadata_graph),
            string(
                &release.id,
                &oreg("releaseKey"),
                &release.key,
                &self.metadata_graph,
            ),
            iri_quad(
                &release.id,
                &oreg("asset"),
                &release.asset,
                &self.metadata_graph,
            ),
            iri_quad(
                &release.id,
                &dcat("isVersionOf"),
                &release.asset,
                &self.metadata_graph,
            ),
            string(
                &release.id,
                &dcat("version"),
                &release.version,
                &self.metadata_graph,
            ),
            string(
                &release.id,
                &dcterms("title"),
                &release.title,
                &self.metadata_graph,
            ),
            string(
                &release.id,
                &dcterms("format"),
                &release.format,
                &self.metadata_graph,
            ),
            string(
                &release.id,
                &oreg("sourceHash"),
                &release.source_hash,
                &self.metadata_graph,
            ),
            string(
                &release.id,
                &oreg("contentHash"),
                &release.content_hash,
                &self.metadata_graph,
            ),
            string(
                &release.id,
                &oreg("releaseChannel"),
                &release.channel,
                &self.metadata_graph,
            ),
            string(
                &release.id,
                &oreg("graph"),
                &release.graph,
                &self.metadata_graph,
            ),
            time_quad(
                &release.id,
                &dcterms("issued"),
                &release.published_at,
                &self.metadata_graph,
            ),
        ];
        if let Some(description) = &release.description {
            additions.push(string(
                &release.id,
                &dcterms("description"),
                description,
                &self.metadata_graph,
            ));
        }
        additions.extend(stats_quads(
            &release.id,
            &release.statistics,
            &self.metadata_graph,
        ));
        additions.extend(import_quads(
            &release.id,
            &release.imports,
            &self.metadata_graph,
        ));
        for distribution in &release.distributions {
            additions.push(iri_quad(
                &release.id,
                &dcat("distribution"),
                &distribution.id,
                &self.metadata_graph,
            ));
            additions.extend(Self::distribution_quads(distribution, &self.metadata_graph));
        }
        self.store.insert(&additions)
    }

    /// Retrieves a release by key or IRI.
    pub fn get_release(&self, value: &str) -> Result<Release> {
        let id = if let Ok(id) = Id::new(value) {
            if typed_quads(
                &self.subject(&id, &self.metadata_graph)?,
                &oreg("OntologyRelease"),
            ) {
                id
            } else {
                self.find_by_literal(
                    &oreg("OntologyRelease"),
                    &oreg("releaseKey"),
                    value,
                    &self.metadata_graph,
                )?
                .ok_or_else(|| Error::NotFound {
                    kind: "release",
                    id: value.to_owned(),
                })?
            }
        } else {
            self.find_by_literal(
                &oreg("OntologyRelease"),
                &oreg("releaseKey"),
                value,
                &self.metadata_graph,
            )?
            .ok_or_else(|| Error::NotFound {
                kind: "release",
                id: value.to_owned(),
            })?
        };
        self.get_release_by_iri(&id)
    }

    /// Retrieves a release by version IRI.
    pub fn get_release_by_iri(&self, id: &Id) -> Result<Release> {
        let quads = self.subject(id, &self.metadata_graph)?;
        if !typed_quads(&quads, &oreg("OntologyRelease")) {
            return Err(Error::NotFound {
                kind: "release",
                id: id.to_string(),
            });
        }
        Ok(Release {
            id: id.clone(),
            key: literal(&quads, &oreg("releaseKey")).unwrap_or_else(|| release_key(id, "")),
            asset: iri(&quads, &oreg("asset")).ok_or_else(|| malformed("release has no asset"))?,
            version: literal(&quads, &dcat("version")).unwrap_or_default(),
            title: literal(&quads, &dcterms("title")).unwrap_or_else(|| id.local_name().to_owned()),
            description: literal(&quads, &dcterms("description")),
            format: literal(&quads, &dcterms("format")).unwrap_or_else(|| "turtle".to_owned()),
            source_hash: literal(&quads, &oreg("sourceHash")).unwrap_or_default(),
            content_hash: literal(&quads, &oreg("contentHash")).unwrap_or_default(),
            channel: literal(&quads, &oreg("releaseChannel"))
                .unwrap_or_else(|| "stable".to_owned()),
            published_at: datetime(&quads, &dcterms("issued")),
            graph: literal(&quads, &oreg("graph")).unwrap_or_else(|| {
                format!(
                    "urn:onto:registry:release:{}",
                    literal(&quads, &oreg("releaseKey")).unwrap_or_default()
                )
            }),
            statistics: read_stats(&quads),
            imports: self.read_imports(id, &self.metadata_graph)?,
            distributions: self.read_distributions(&quads)?,
        })
    }

    fn distribution_quads(distribution: &Distribution, graph: &GraphName) -> Vec<Quad> {
        vec![
            typed(&distribution.id, &dcat("Distribution"), graph),
            string(
                &distribution.id,
                &oreg("distributionFormat"),
                &distribution.format,
                graph,
            ),
            string(
                &distribution.id,
                &dcat("mediaType"),
                &distribution.media_type,
                graph,
            ),
            string(
                &distribution.id,
                &oreg("contentHash"),
                &distribution.sha256,
                graph,
            ),
            Quad::in_graph(
                Term::Iri(distribution.id.clone()),
                dcat("byteSize"),
                Term::Literal(Literal::integer(distribution.byte_size as i64)),
                graph.clone(),
            ),
            string(
                &distribution.id,
                &dcat("downloadURL"),
                &distribution.url,
                graph,
            ),
        ]
    }

    fn read_distributions(&self, quads: &[Quad]) -> Result<Vec<Distribution>> {
        let mut values = Vec::new();
        for id in iris(quads, &dcat("distribution")) {
            let quads = self.subject(&id, &self.metadata_graph)?;
            values.push(Distribution {
                id: id.clone(),
                format: literal(&quads, &oreg("distributionFormat")).unwrap_or_default(),
                media_type: literal(&quads, &dcat("mediaType")).unwrap_or_default(),
                sha256: literal(&quads, &oreg("contentHash")).unwrap_or_default(),
                byte_size: integer(&quads, &dcat("byteSize")) as u64,
                url: literal(&quads, &dcat("downloadURL")).unwrap_or_default(),
            });
        }
        values.sort_by(|left, right| left.format.cmp(&right.format));
        Ok(values)
    }

    /// Lists releases, optionally restricted to an asset.
    pub fn list_releases(&self, asset: Option<&Id>) -> Result<Vec<Release>> {
        let mut values = Vec::new();
        for id in self.ids_of_type(&oreg("OntologyRelease"), &self.metadata_graph)? {
            if let Ok(release) = self.get_release_by_iri(&id) {
                if asset.is_none_or(|asset| &release.asset == asset) {
                    values.push(release);
                }
            }
        }
        values.sort_by_key(|right| std::cmp::Reverse(right.published_at));
        Ok(values)
    }

    /// Returns the current release of an asset.
    pub fn current_release(&self, asset: &Id) -> Result<Option<Id>> {
        let quads = self.subject(asset, &self.metadata_graph)?;
        Ok(iri(&quads, &oreg("currentRelease")))
    }

    /// Updates the current release pointer.
    pub fn set_current_release(&self, asset: &Id, release: Option<&Id>) -> Result<()> {
        let mut additions = Vec::new();
        if let Some(release) = release {
            additions.push(iri_quad(
                asset,
                &oreg("currentRelease"),
                release,
                &self.metadata_graph,
            ));
            additions.push(iri_quad(
                asset,
                &dcat("hasCurrentVersion"),
                release,
                &self.metadata_graph,
            ));
        }
        let removals = self
            .store
            .get(
                QuadPattern::new()
                    .with_subject(Term::Iri(asset.clone()))
                    .with_predicate(oreg("currentRelease"))
                    .with_graph(self.metadata_graph.clone()),
            )?
            .into_iter()
            .chain(
                self.store.get(
                    QuadPattern::new()
                        .with_subject(Term::Iri(asset.clone()))
                        .with_predicate(dcat("hasCurrentVersion"))
                        .with_graph(self.metadata_graph.clone()),
                )?,
            )
            .collect();
        self.replace(removals, additions)
    }

    /// Finds a release by asset and version.
    pub fn find_release(&self, asset: &Id, version: &str) -> Result<Option<Release>> {
        let key = release_key(asset, version);
        match self.find_by_literal(
            &oreg("OntologyRelease"),
            &oreg("releaseKey"),
            &key,
            &self.metadata_graph,
        )? {
            Some(id) => self.get_release_by_iri(&id).map(Some),
            None => Ok(None),
        }
    }

    /// Updates a release channel.
    pub fn set_channel(&self, release: &Id, channel: &str) -> Result<()> {
        let additions = vec![string(
            release,
            &oreg("releaseChannel"),
            channel,
            &self.metadata_graph,
        )];
        self.replace_predicate(
            release,
            &oreg("releaseChannel"),
            &self.metadata_graph,
            additions,
        )
    }

    /// Deletes a staging graph.
    pub fn delete_graph(&self, graph: &GraphName) -> Result<()> {
        let quads = self
            .store
            .get(QuadPattern::new().with_graph(graph.clone()))?;
        if !quads.is_empty() {
            self.store.delete(&quads)?;
        }
        Ok(())
    }

    /// Creates a profile.
    pub fn create_profile(&self, profile: &Profile) -> Result<()> {
        let mut additions = vec![
            typed(&profile.id, &prof("Profile"), &self.profile_graph),
            typed(&profile.id, &oreg("Profile"), &self.profile_graph),
            string(
                &profile.id,
                &dcterms("title"),
                &profile.title,
                &self.profile_graph,
            ),
            time_quad(
                &profile.id,
                &dcterms("created"),
                &profile.created_at,
                &self.profile_graph,
            ),
        ];
        if let Some(description) = &profile.description {
            additions.push(string(
                &profile.id,
                &dcterms("description"),
                description,
                &self.profile_graph,
            ));
        }
        if let Some(hash) = &profile.shapes_hash {
            additions.push(string(
                &profile.id,
                &oreg("shapesHash"),
                hash,
                &self.profile_graph,
            ));
        }
        self.replace_subject(&profile.id, &self.profile_graph, additions)
    }

    /// Lists profiles.
    pub fn list_profiles(&self) -> Result<Vec<Profile>> {
        let mut values = Vec::new();
        for id in self.ids_of_type(&oreg("Profile"), &self.profile_graph)? {
            let quads = self.subject(&id, &self.profile_graph)?;
            values.push(Profile {
                id: id.clone(),
                title: literal(&quads, &dcterms("title"))
                    .unwrap_or_else(|| id.local_name().to_owned()),
                description: literal(&quads, &dcterms("description")),
                shapes_hash: literal(&quads, &oreg("shapesHash")),
                created_at: datetime(&quads, &dcterms("created")),
            });
        }
        values.sort_by(|left, right| left.title.cmp(&right.title));
        Ok(values)
    }

    /// Stores a validation report and links it from a candidate/release.
    pub fn insert_validation(&self, report: &ValidationReport, link: &Id) -> Result<()> {
        let graph = validation_graph(report.id.local_name())?;
        let mut additions = vec![
            typed(&report.id, &sh("ValidationReport"), &graph),
            typed(&report.id, &oreg("ValidationReport"), &graph),
            typed(&report.id, &prov("Entity"), &graph),
            iri_quad(&report.id, &oreg("target"), &report.target, &graph),
            bool_quad(&report.id, &sh("conforms"), report.conforms, &graph),
            bool_quad(
                &report.id,
                &oreg("externalShacl"),
                report.external_shacl,
                &graph,
            ),
            time_quad(
                &report.id,
                &prov("generatedAtTime"),
                &report.generated_at,
                &graph,
            ),
        ];
        for (index, issue) in report.issues.iter().enumerate() {
            let subject = Id::new(format!("{}#issue-{index}", report.id))?;
            additions.push(typed(&subject, &sh("ValidationResult"), &graph));
            additions.push(string(
                &subject,
                &sh("resultSeverity"),
                &issue.severity,
                &graph,
            ));
            additions.push(string(
                &subject,
                &sh("resultMessage"),
                &issue.message,
                &graph,
            ));
            if let Some(focus) = &issue.focus_node {
                additions.push(iri_quad(&subject, &sh("focusNode"), focus, &graph));
            }
            if let Some(path) = &issue.path {
                additions.push(string(&subject, &sh("resultPath"), path, &graph));
            }
        }
        self.store.insert(&additions)?;
        self.store.insert(&[iri_quad(
            link,
            &oreg("hasValidationReport"),
            &report.id,
            &self.metadata_graph,
        )])
    }

    /// Links an existing report to another candidate/release subject.
    pub fn link_validation(&self, link: &Id, report: &Id) -> Result<()> {
        self.store.insert(&[iri_quad(
            link,
            &oreg("hasValidationReport"),
            report,
            &self.metadata_graph,
        )])
    }

    /// Reads validation reports linked from a candidate or release.
    pub fn validation_reports(&self, link: &Id) -> Result<Vec<ValidationReport>> {
        let quads = self.subject(link, &self.metadata_graph)?;
        let mut values = Vec::new();
        for id in iris(&quads, &oreg("hasValidationReport")) {
            let graph = validation_graph(id.local_name())?;
            let report_quads = self.subject(&id, &graph)?;
            let mut issues = Vec::new();
            let issue_quads = self.store.get(
                QuadPattern::new()
                    .with_predicate(rdf_type())
                    .with_object(Term::Iri(sh("ValidationResult")))
                    .with_graph(graph.clone()),
            )?;
            for issue_quad in issue_quads {
                let Some(subject) = issue_quad.subject.as_iri() else {
                    continue;
                };
                let props = self.subject(subject, &graph)?;
                issues.push(ValidationIssue {
                    severity: literal(&props, &sh("resultSeverity"))
                        .unwrap_or_else(|| "error".to_owned()),
                    focus_node: iri(&props, &sh("focusNode")),
                    path: literal(&props, &sh("resultPath")),
                    message: literal(&props, &sh("resultMessage")).unwrap_or_default(),
                });
            }
            issues.sort_by(|left, right| left.message.cmp(&right.message));
            values.push(ValidationReport {
                id: id.clone(),
                target: iri(&report_quads, &oreg("target")).unwrap_or_else(|| link.clone()),
                conforms: boolean(&report_quads, &sh("conforms"), false),
                external_shacl: boolean(&report_quads, &oreg("externalShacl"), false),
                issues,
                generated_at: datetime(&report_quads, &prov("generatedAtTime")),
            });
        }
        values.sort_by_key(|right| std::cmp::Reverse(right.generated_at));
        Ok(values)
    }

    fn read_imports(&self, base: &Id, graph: &GraphName) -> Result<Vec<ImportDependency>> {
        let base_quads = self.subject(base, graph)?;
        let mut values = Vec::new();
        for subject in iris(&base_quads, &oreg("hasImport")) {
            let quads = self.subject(&subject, graph)?;
            let Some(import_iri) = iri(&quads, &oreg("importsOntology")) else {
                continue;
            };
            let resolved_release = iri(&quads, &oreg("resolvedRelease"));
            values.push(ImportDependency {
                iri: import_iri,
                version: literal(&quads, &oreg("importVersion")),
                resolved: resolved_release.is_some() || boolean(&quads, &oreg("resolved"), false),
                resolved_release,
            });
        }
        values.sort_by(|left, right| left.iri.cmp(&right.iri));
        Ok(values)
    }

    /// Ensures a term record exists by key.
    pub fn find_term(&self, key: &str, term: &str) -> Result<TermSummary> {
        let terms = self.terms(key)?;
        terms
            .into_iter()
            .find(|candidate| candidate.key == term || candidate.iri.as_str() == term)
            .ok_or_else(|| Error::NotFound {
                kind: "term",
                id: term.to_owned(),
            })
    }

    /// Registers the source artifact reference on a candidate.
    pub fn insert_artifact_ref(&self, subject: &Id, artifact: &ArtifactRef) -> Result<()> {
        let additions = vec![
            string(
                subject,
                &oreg("sourceArtifact"),
                &artifact.sha256,
                &self.metadata_graph,
            ),
            int_quad(
                subject,
                &oreg("sourceBytes"),
                artifact.byte_size as usize,
                &self.metadata_graph,
            ),
        ];
        self.store.insert(&additions)
    }
}

fn malformed(detail: &str) -> Error {
    Error::Registry {
        stage: "repository",
        detail: detail.to_owned(),
    }
}

fn stats_quads(subject: &Id, stats: &ReleaseStatistics, graph: &GraphName) -> Vec<Quad> {
    vec![
        int_quad(subject, &void("triples"), stats.quads, graph),
        int_quad(subject, &oreg("classCount"), stats.classes, graph),
        int_quad(
            subject,
            &oreg("objectPropertyCount"),
            stats.object_properties,
            graph,
        ),
        int_quad(
            subject,
            &oreg("datatypePropertyCount"),
            stats.datatype_properties,
            graph,
        ),
        int_quad(
            subject,
            &oreg("annotationPropertyCount"),
            stats.annotation_properties,
            graph,
        ),
        int_quad(subject, &oreg("individualCount"), stats.individuals, graph),
        int_quad(subject, &oreg("termCount"), stats.terms, graph),
    ]
}

fn read_stats(quads: &[Quad]) -> ReleaseStatistics {
    ReleaseStatistics {
        quads: integer(quads, &void("triples")),
        classes: integer(quads, &oreg("classCount")),
        object_properties: integer(quads, &oreg("objectPropertyCount")),
        datatype_properties: integer(quads, &oreg("datatypePropertyCount")),
        annotation_properties: integer(quads, &oreg("annotationPropertyCount")),
        individuals: integer(quads, &oreg("individualCount")),
        terms: integer(quads, &oreg("termCount")),
    }
}

fn import_quads(base: &Id, imports: &[ImportDependency], graph: &GraphName) -> Vec<Quad> {
    let mut quads = Vec::new();
    for dependency in imports {
        let local = format!("import:{}", hash_key(dependency.iri.as_str()));
        let subject = Id::new(format!("{}#{local}", base.as_str())).expect("generated import IRI");
        quads.push(typed(&subject, &oreg("ImportDependency"), graph));
        quads.push(iri_quad(
            &subject,
            &oreg("importsOntology"),
            &dependency.iri,
            graph,
        ));
        quads.push(bool_quad(
            &subject,
            &oreg("resolved"),
            dependency.resolved,
            graph,
        ));
        if let Some(version) = &dependency.version {
            quads.push(string(&subject, &oreg("importVersion"), version, graph));
        }
        if let Some(resolved) = &dependency.resolved_release {
            quads.push(iri_quad(
                &subject,
                &oreg("resolvedRelease"),
                resolved,
                graph,
            ));
        }
        quads.push(iri_quad(base, &oreg("hasImport"), &subject, graph));
    }
    quads
}
