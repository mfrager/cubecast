//! The RDF-backed external ontology registry and mapping store.

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::sync::Arc;

use chrono::{DateTime, Utc};
use ontology_core::error::{Error, Result};
use ontology_core::external::{
    ClassMapping, ExternalOntology, ExternalOntologyRegistry, ImportReport, ImportRequest,
    MappingIssue, MappingKind, MappingQuery, MappingReport, PropertyMapping,
};
use ontology_core::format::RdfFormat;
use ontology_core::graph::GraphKind;
use ontology_core::id::Id;
use ontology_core::model::{GraphName, Literal, Quad, QuadPattern, Term};
use ontology_core::owl::Severity;
use ontology_core::rdf::RdfStore;
use sha2::{Digest, Sha256};

const RDF: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#";
const RDFS: &str = "http://www.w3.org/2000/01/rdf-schema#";
const OWL: &str = "http://www.w3.org/2002/07/owl#";
const XSD: &str = "http://www.w3.org/2001/XMLSchema#";
const DCTERMS: &str = "http://purl.org/dc/terms/";
const PROV: &str = "http://www.w3.org/ns/prov#";

fn iri(value: &str) -> Id {
    Id::new(value).expect("well-known vocabulary IRIs are valid")
}

fn namespace_iri(namespace: &str, local: &str) -> Id {
    iri(&format!("{namespace}{local}"))
}

fn rdf_type() -> Id {
    namespace_iri(RDF, "type")
}

fn owl(local: &str) -> Id {
    namespace_iri(OWL, local)
}

fn rdfs(local: &str) -> Id {
    namespace_iri(RDFS, local)
}

fn dcterms(local: &str) -> Id {
    namespace_iri(DCTERMS, local)
}

fn prov(local: &str) -> Id {
    namespace_iri(PROV, local)
}

fn xsd(local: &str) -> Id {
    namespace_iri(XSD, local)
}

fn onto(local: &str) -> Id {
    Id::new(format!("urn:onto:schema:{local}")).expect("internal schema IRIs are valid")
}

fn literal_value(quads: &[Quad], predicate: &Id) -> Option<String> {
    quads
        .iter()
        .find(|quad| &quad.predicate == predicate)
        .and_then(|quad| match &quad.object {
            Term::Literal(literal) => Some(literal.value.clone()),
            _ => None,
        })
}

fn first_literal(quads: &[Quad], predicates: &[Id]) -> Option<String> {
    quads.iter().find_map(|quad| {
        if !predicates.contains(&quad.predicate) {
            return None;
        }
        match &quad.object {
            Term::Literal(literal) => Some(literal.value.clone()),
            _ => None,
        }
    })
}

fn is_typed(quads: &[Quad], class: &Id) -> bool {
    quads
        .iter()
        .any(|quad| quad.predicate == rdf_type() && quad.object == Term::Iri(class.clone()))
}

fn graph_for(ontology: &Id) -> Result<GraphName> {
    let digest = Sha256::digest(ontology.as_str().as_bytes());
    let mut hex = String::with_capacity(16);
    for byte in &digest[..8] {
        use std::fmt::Write as _;
        let _ = write!(hex, "{byte:02x}");
    }
    Ok(GraphName::Iri(Id::new(format!("urn:onto:external:{hex}"))?))
}

fn resolve_format(request: &ImportRequest) -> Result<RdfFormat> {
    if let Some(name) = &request.format {
        return name.parse();
    }
    RdfFormat::from_path(&request.source).ok_or_else(|| {
        Error::validation(format!(
            "cannot infer the RDF format of {:?}; set the format explicitly",
            request.source
        ))
    })
}

fn discover_ontology(parsed: &[Quad]) -> Result<Id> {
    let mut candidates: BTreeSet<Id> = parsed
        .iter()
        .filter(|quad| quad.predicate == rdf_type() && quad.object == Term::Iri(owl("Ontology")))
        .filter_map(|quad| quad.subject.as_iri().cloned())
        .collect();
    candidates
        .pop_first()
        .ok_or_else(|| Error::validation("document declares no owl:Ontology IRI"))
}

fn count_subjects(parsed: &[Quad], classes: &[Id]) -> usize {
    let mut subjects = BTreeSet::new();
    for quad in parsed {
        if quad.predicate != rdf_type() {
            continue;
        }
        if classes
            .iter()
            .any(|class| quad.object == Term::Iri(class.clone()))
        {
            if let Some(subject) = quad.subject.as_iri() {
                subjects.insert(subject.clone());
            }
        }
    }
    subjects.len()
}

fn namespace_of(term: &Id) -> Option<String> {
    let text = term.as_str();
    let index = text.rfind(|character| character == '#' || character == '/')?;
    if index == 0 {
        return None;
    }
    Some(text[..=index].to_owned())
}

fn discover_namespace(parsed: &[Quad], classes: &[Id]) -> Option<String> {
    let mut counts: BTreeMap<String, usize> = BTreeMap::new();
    for quad in parsed {
        if quad.predicate != rdf_type() {
            continue;
        }
        if !classes
            .iter()
            .any(|class| quad.object == Term::Iri(class.clone()))
        {
            continue;
        }
        if let Some(subject) = quad.subject.as_iri() {
            if let Some(namespace) = namespace_of(subject) {
                *counts.entry(namespace).or_default() += 1;
            }
        }
    }
    counts
        .into_iter()
        .max_by(|left, right| left.1.cmp(&right.1).then_with(|| right.0.cmp(&left.0)))
        .map(|(namespace, _)| namespace)
}

fn metadata_quads(record: &ExternalOntology, graph: &GraphName) -> Vec<Quad> {
    let subject = Term::Iri(record.id.clone());
    let mut quads = vec![
        Quad::in_graph(
            subject.clone(),
            rdf_type(),
            Term::Iri(onto("ExternalOntology")),
            graph.clone(),
        ),
        Quad::in_graph(
            subject.clone(),
            onto("source"),
            Term::string(&record.source),
            graph.clone(),
        ),
        Quad::in_graph(
            subject.clone(),
            onto("targetGraph"),
            Term::string(record.graph.to_string()),
            graph.clone(),
        ),
        Quad::in_graph(
            subject.clone(),
            prov("generatedAtTime"),
            Term::Literal(Literal::datetime(&record.imported_at)),
            graph.clone(),
        ),
        Quad::in_graph(
            subject.clone(),
            onto("quadCount"),
            Term::Literal(Literal::integer(record.quad_count as i64)),
            graph.clone(),
        ),
        Quad::in_graph(
            subject.clone(),
            onto("classCount"),
            Term::Literal(Literal::integer(record.class_count as i64)),
            graph.clone(),
        ),
        Quad::in_graph(
            subject.clone(),
            onto("propertyCount"),
            Term::Literal(Literal::integer(record.property_count as i64)),
            graph.clone(),
        ),
    ];
    if let Some(title) = &record.title {
        quads.push(Quad::in_graph(
            subject.clone(),
            rdfs("label"),
            Term::string(title),
            graph.clone(),
        ));
    }
    if let Some(version) = &record.version {
        quads.push(Quad::in_graph(
            subject.clone(),
            owl("versionInfo"),
            Term::string(version),
            graph.clone(),
        ));
    }
    if let Some(namespace) = &record.namespace {
        quads.push(Quad::in_graph(
            subject.clone(),
            onto("namespace"),
            Term::string(namespace),
            graph.clone(),
        ));
    }
    if let Some(format) = &record.format {
        quads.push(Quad::in_graph(
            subject.clone(),
            dcterms("format"),
            Term::string(format),
            graph.clone(),
        ));
    }
    quads
}

fn class_mapping_quads(mapping: &ClassMapping, graph: &GraphName) -> Vec<Quad> {
    let subject = Term::Iri(mapping.id.clone());
    let mut quads = vec![
        Quad::in_graph(
            subject.clone(),
            rdf_type(),
            Term::Iri(onto("ClassMapping")),
            graph.clone(),
        ),
        Quad::in_graph(
            subject.clone(),
            onto("externalClass"),
            Term::Iri(mapping.external.clone()),
            graph.clone(),
        ),
        Quad::in_graph(
            subject.clone(),
            onto("internalClass"),
            Term::Iri(mapping.internal.clone()),
            graph.clone(),
        ),
        Quad::in_graph(
            subject.clone(),
            onto("mappingKind"),
            Term::string(mapping.kind.as_str()),
            graph.clone(),
        ),
        Quad::in_graph(
            subject.clone(),
            onto("recordedAt"),
            Term::Literal(Literal::datetime(&mapping.recorded_at)),
            graph.clone(),
        ),
    ];
    if let Some(confidence) = mapping.confidence {
        quads.push(Quad::in_graph(
            subject,
            onto("confidence"),
            Term::Literal(Literal::typed(confidence.to_string(), xsd("decimal"))),
            graph.clone(),
        ));
    }
    quads
}

fn property_mapping_quads(mapping: &PropertyMapping, graph: &GraphName) -> Vec<Quad> {
    let subject = Term::Iri(mapping.id.clone());
    let mut quads = vec![
        Quad::in_graph(
            subject.clone(),
            rdf_type(),
            Term::Iri(onto("PropertyMapping")),
            graph.clone(),
        ),
        Quad::in_graph(
            subject.clone(),
            onto("externalProperty"),
            Term::Iri(mapping.external.clone()),
            graph.clone(),
        ),
        Quad::in_graph(
            subject.clone(),
            onto("internalProperty"),
            Term::Iri(mapping.internal.clone()),
            graph.clone(),
        ),
        Quad::in_graph(
            subject.clone(),
            onto("mappingKind"),
            Term::string(mapping.kind.as_str()),
            graph.clone(),
        ),
        Quad::in_graph(
            subject.clone(),
            onto("recordedAt"),
            Term::Literal(Literal::datetime(&mapping.recorded_at)),
            graph.clone(),
        ),
    ];
    if let Some(confidence) = mapping.confidence {
        quads.push(Quad::in_graph(
            subject,
            onto("confidence"),
            Term::Literal(Literal::typed(confidence.to_string(), xsd("decimal"))),
            graph.clone(),
        ));
    }
    quads
}

fn confidence_value(quads: &[Quad]) -> Option<f64> {
    literal_value(quads, &onto("confidence")).and_then(|value| value.parse().ok())
}

fn recorded_at_value(quads: &[Quad]) -> DateTime<Utc> {
    quads
        .iter()
        .find(|quad| quad.predicate == onto("recordedAt"))
        .and_then(|quad| match &quad.object {
            Term::Literal(literal) => DateTime::parse_from_rfc3339(&literal.value).ok(),
            _ => None,
        })
        .map_or_else(Utc::now, |value| value.with_timezone(&Utc))
}

/// External ontology registry persisted as RDF.
pub struct ExternalOntologyManager {
    store: Arc<dyn RdfStore>,
    metadata_graph: GraphName,
}

impl ExternalOntologyManager {
    /// Creates a registry whose metadata lives in `system:external`.
    #[must_use]
    pub fn new(store: Arc<dyn RdfStore>) -> Self {
        Self {
            store,
            metadata_graph: GraphName::Iri(GraphKind::External.iri()),
        }
    }

    /// Creates a registry whose metadata lives in `graph`.
    #[must_use]
    pub fn with_graph(store: Arc<dyn RdfStore>, graph: GraphName) -> Self {
        Self {
            store,
            metadata_graph: graph,
        }
    }

    /// The graph registry metadata is written to.
    #[must_use]
    pub fn metadata_graph(&self) -> &GraphName {
        &self.metadata_graph
    }

    fn replace(&self, removals: &[Quad], additions: &[Quad]) -> Result<()> {
        let mut transaction = self.store.transaction()?;
        transaction.delete(removals)?;
        transaction.insert(additions)?;
        transaction.commit()
    }

    fn load(&self, id: &Id) -> Result<Option<ExternalOntology>> {
        let quads = self.store.get(
            QuadPattern::new()
                .with_subject(Term::Iri(id.clone()))
                .with_graph(self.metadata_graph.clone()),
        )?;
        if !is_typed(&quads, &onto("ExternalOntology")) {
            return Ok(None);
        }
        let graph_text = literal_value(&quads, &onto("targetGraph"))
            .ok_or_else(|| Error::validation(format!("registry entry {id} has no target graph")))?;
        let graph = GraphName::Iri(Id::new(graph_text)?);
        let imported_at = quads
            .iter()
            .find(|quad| quad.predicate == prov("generatedAtTime"))
            .and_then(|quad| match &quad.object {
                Term::Literal(literal) => DateTime::parse_from_rfc3339(&literal.value).ok(),
                _ => None,
            })
            .map_or_else(Utc::now, |value| value.with_timezone(&Utc));
        Ok(Some(ExternalOntology {
            id: id.clone(),
            source: literal_value(&quads, &onto("source")).unwrap_or_default(),
            graph,
            title: first_literal(&quads, &[rdfs("label"), dcterms("title")]),
            version: literal_value(&quads, &owl("versionInfo")),
            namespace: literal_value(&quads, &onto("namespace")),
            format: literal_value(&quads, &dcterms("format")),
            imported_at,
            quad_count: literal_value(&quads, &onto("quadCount"))
                .and_then(|value| value.parse().ok())
                .unwrap_or_default(),
            class_count: literal_value(&quads, &onto("classCount"))
                .and_then(|value| value.parse().ok())
                .unwrap_or_default(),
            property_count: literal_value(&quads, &onto("propertyCount"))
                .and_then(|value| value.parse().ok())
                .unwrap_or_default(),
        }))
    }

    fn registry_ids(&self) -> Result<Vec<Id>> {
        let quads = self.store.get(
            QuadPattern::new()
                .with_predicate(rdf_type())
                .with_object(Term::Iri(onto("ExternalOntology")))
                .with_graph(self.metadata_graph.clone()),
        )?;
        let mut ids: Vec<Id> = quads
            .iter()
            .filter_map(|quad| quad.subject.as_iri().cloned())
            .collect();
        ids.sort();
        ids.dedup();
        Ok(ids)
    }

    fn class_mappings(&self) -> Result<Vec<ClassMapping>> {
        let ids = self.mapping_ids(&onto("ClassMapping"))?;
        let mut mappings = Vec::new();
        for id in ids {
            if let Some(mapping) = self.load_class_mapping(&id)? {
                mappings.push(mapping);
            }
        }
        mappings.sort_by(|left, right| left.id.cmp(&right.id));
        Ok(mappings)
    }

    fn property_mappings(&self) -> Result<Vec<PropertyMapping>> {
        let ids = self.mapping_ids(&onto("PropertyMapping"))?;
        let mut mappings = Vec::new();
        for id in ids {
            if let Some(mapping) = self.load_property_mapping(&id)? {
                mappings.push(mapping);
            }
        }
        mappings.sort_by(|left, right| left.id.cmp(&right.id));
        Ok(mappings)
    }

    fn mapping_ids(&self, class: &Id) -> Result<Vec<Id>> {
        let quads = self.store.get(
            QuadPattern::new()
                .with_predicate(rdf_type())
                .with_object(Term::Iri(class.clone()))
                .with_graph(self.metadata_graph.clone()),
        )?;
        let mut ids: Vec<Id> = quads
            .iter()
            .filter_map(|quad| quad.subject.as_iri().cloned())
            .collect();
        ids.sort();
        ids.dedup();
        Ok(ids)
    }

    fn load_class_mapping(&self, id: &Id) -> Result<Option<ClassMapping>> {
        let quads = self.store.get(
            QuadPattern::new()
                .with_subject(Term::Iri(id.clone()))
                .with_graph(self.metadata_graph.clone()),
        )?;
        if !is_typed(&quads, &onto("ClassMapping")) {
            return Ok(None);
        }
        let (Some(external), Some(internal), Some(kind)) = (
            quads
                .iter()
                .find(|quad| quad.predicate == onto("externalClass"))
                .and_then(|quad| quad.object.as_iri().cloned()),
            quads
                .iter()
                .find(|quad| quad.predicate == onto("internalClass"))
                .and_then(|quad| quad.object.as_iri().cloned()),
            literal_value(&quads, &onto("mappingKind")),
        ) else {
            return Ok(None);
        };
        let Some(kind) = MappingKind::parse(&kind) else {
            return Ok(None);
        };
        Ok(Some(ClassMapping {
            id: id.clone(),
            external,
            internal,
            kind,
            confidence: confidence_value(&quads),
            recorded_at: recorded_at_value(&quads),
        }))
    }

    fn load_property_mapping(&self, id: &Id) -> Result<Option<PropertyMapping>> {
        let quads = self.store.get(
            QuadPattern::new()
                .with_subject(Term::Iri(id.clone()))
                .with_graph(self.metadata_graph.clone()),
        )?;
        if !is_typed(&quads, &onto("PropertyMapping")) {
            return Ok(None);
        }
        let (Some(external), Some(internal), Some(kind)) = (
            quads
                .iter()
                .find(|quad| quad.predicate == onto("externalProperty"))
                .and_then(|quad| quad.object.as_iri().cloned()),
            quads
                .iter()
                .find(|quad| quad.predicate == onto("internalProperty"))
                .and_then(|quad| quad.object.as_iri().cloned()),
            literal_value(&quads, &onto("mappingKind")),
        ) else {
            return Ok(None);
        };
        let Some(kind) = MappingKind::parse(&kind) else {
            return Ok(None);
        };
        Ok(Some(PropertyMapping {
            id: id.clone(),
            external,
            internal,
            kind,
            confidence: confidence_value(&quads),
            recorded_at: recorded_at_value(&quads),
        }))
    }

    fn read_source(source: &str) -> Result<Vec<u8>> {
        if source.starts_with("http://") || source.starts_with("https://") {
            return Err(Error::Unsupported {
                operation: "remote ontology resolution".to_owned(),
            });
        }
        let path = source.strip_prefix("file://").unwrap_or(source);
        fs::read(path).map_err(|error| Error::validation(format!("cannot read {path}: {error}")))
    }

    fn declared(&self, term: &Id, classes: &[Id], graph: Option<&GraphName>) -> Result<bool> {
        let mut pattern = QuadPattern::new()
            .with_subject(Term::Iri(term.clone()))
            .with_predicate(rdf_type());
        if let Some(graph) = graph {
            pattern = pattern.with_graph(graph.clone());
        }
        let quads = self.store.get(pattern)?;
        Ok(quads.iter().any(|quad| {
            classes
                .iter()
                .any(|class| quad.object == Term::Iri(class.clone()))
        }))
    }
}

#[async_trait::async_trait]
impl ExternalOntologyRegistry for ExternalOntologyManager {
    async fn import(&self, request: ImportRequest) -> Result<ImportReport> {
        let data = Self::read_source(&request.source)?;
        self.import_from_slice(request, &data).await
    }

    async fn import_from_slice(&self, request: ImportRequest, data: &[u8]) -> Result<ImportReport> {
        let format = resolve_format(&request)?;
        let base = request
            .source
            .contains("://")
            .then_some(request.source.as_str());
        let parsed = ontology_rdf::parse_quads(format, data, base)?;
        let ontology = match &request.ontology {
            Some(id) => id.clone(),
            None => discover_ontology(&parsed)?,
        };
        let existing = self.load(&ontology)?;
        if let Some(existing) = &existing {
            if existing.source != request.source {
                return Err(Error::Conflict {
                    kind: "external ontology",
                    id: ontology.to_string(),
                });
            }
        }
        let graph = request
            .graph
            .clone()
            .or_else(|| existing.as_ref().map(|record| record.graph.clone()))
            .unwrap_or(graph_for(&ontology)?);

        let class_types = [owl("Class")];
        let property_types = [
            owl("ObjectProperty"),
            owl("DatatypeProperty"),
            owl("AnnotationProperty"),
        ];
        let class_count = count_subjects(&parsed, &class_types);
        let property_count = count_subjects(&parsed, &property_types);
        let vocabulary: Vec<Id> = class_types
            .iter()
            .chain(property_types.iter())
            .cloned()
            .collect();
        let namespace = discover_namespace(&parsed, &vocabulary);
        let ontology_quads: Vec<Quad> = parsed
            .iter()
            .filter(|quad| quad.subject.as_iri() == Some(&ontology))
            .cloned()
            .collect();
        let quad_count = parsed.len();
        let record = ExternalOntology {
            id: ontology.clone(),
            source: request.source.clone(),
            graph: graph.clone(),
            title: first_literal(&ontology_quads, &[rdfs("label"), dcterms("title")]),
            version: literal_value(&ontology_quads, &owl("versionInfo")),
            namespace,
            format: Some(format.as_str().to_owned()),
            imported_at: Utc::now(),
            quad_count,
            class_count,
            property_count,
        };

        let mut removals = BTreeSet::new();
        if let Some(previous) = existing.as_ref().map(|record| record.graph.clone()) {
            removals.extend(self.store.get(QuadPattern::new().with_graph(previous))?);
        }
        removals.extend(
            self.store
                .get(QuadPattern::new().with_graph(graph.clone()))?,
        );
        removals.extend(
            self.store.get(
                QuadPattern::new()
                    .with_subject(Term::Iri(ontology.clone()))
                    .with_graph(self.metadata_graph.clone()),
            )?,
        );
        let removals: Vec<Quad> = removals.into_iter().collect();

        let mut additions = BTreeSet::new();
        additions.extend(
            parsed
                .into_iter()
                .map(|quad| quad.with_graph(graph.clone())),
        );
        additions.extend(metadata_quads(&record, &self.metadata_graph));
        let additions: Vec<Quad> = additions.into_iter().collect();

        self.replace(&removals, &additions)?;
        Ok(ImportReport {
            ontology,
            graph,
            inserted: quad_count as u64,
            class_count,
            property_count,
            already_present: existing.is_some(),
        })
    }

    async fn sync(&self, id: &Id) -> Result<ImportReport> {
        let record = self.get(id).await?;
        let data = Self::read_source(&record.source)?;
        let request = ImportRequest {
            source: record.source,
            ontology: Some(id.clone()),
            format: record.format,
            graph: Some(record.graph),
        };
        self.import_from_slice(request, &data).await
    }

    async fn get(&self, id: &Id) -> Result<ExternalOntology> {
        self.load(id)?.ok_or_else(|| Error::NotFound {
            kind: "external ontology",
            id: id.to_string(),
        })
    }

    async fn list(&self) -> Result<Vec<ExternalOntology>> {
        let mut records = Vec::new();
        for id in self.registry_ids()? {
            if let Some(record) = self.load(&id)? {
                records.push(record);
            }
        }
        Ok(records)
    }

    async fn delete(&self, id: &Id) -> Result<usize> {
        let record = self.get(id).await?;
        let mut removals = self
            .store
            .get(QuadPattern::new().with_graph(record.graph.clone()))?;
        removals.extend(
            self.store.get(
                QuadPattern::new()
                    .with_subject(Term::Iri(id.clone()))
                    .with_graph(self.metadata_graph.clone()),
            )?,
        );
        self.store.delete(&removals)?;
        Ok(removals.len())
    }

    async fn map_class(&self, mapping: ClassMapping) -> Result<Id> {
        if self.class_mappings()?.iter().any(|existing| {
            existing.external == mapping.external
                && existing.internal == mapping.internal
                && existing.kind == mapping.kind
        }) {
            return Err(Error::Conflict {
                kind: "class mapping",
                id: mapping.id.to_string(),
            });
        }
        self.store
            .insert(&class_mapping_quads(&mapping, &self.metadata_graph))?;
        Ok(mapping.id)
    }

    async fn map_property(&self, mapping: PropertyMapping) -> Result<Id> {
        if self.property_mappings()?.iter().any(|existing| {
            existing.external == mapping.external
                && existing.internal == mapping.internal
                && existing.kind == mapping.kind
        }) {
            return Err(Error::Conflict {
                kind: "property mapping",
                id: mapping.id.to_string(),
            });
        }
        self.store
            .insert(&property_mapping_quads(&mapping, &self.metadata_graph))?;
        Ok(mapping.id)
    }

    async fn list_class_mappings(&self, query: MappingQuery) -> Result<Vec<ClassMapping>> {
        if query.limit == 0 {
            return Ok(Vec::new());
        }
        Ok(self
            .class_mappings()?
            .into_iter()
            .filter(|mapping| {
                query
                    .external
                    .as_ref()
                    .is_none_or(|external| &mapping.external == external)
            })
            .filter(|mapping| {
                query
                    .internal
                    .as_ref()
                    .is_none_or(|internal| &mapping.internal == internal)
            })
            .skip(query.offset)
            .take(query.limit)
            .collect())
    }

    async fn list_property_mappings(&self, query: MappingQuery) -> Result<Vec<PropertyMapping>> {
        if query.limit == 0 {
            return Ok(Vec::new());
        }
        Ok(self
            .property_mappings()?
            .into_iter()
            .filter(|mapping| {
                query
                    .external
                    .as_ref()
                    .is_none_or(|external| &mapping.external == external)
            })
            .filter(|mapping| {
                query
                    .internal
                    .as_ref()
                    .is_none_or(|internal| &mapping.internal == internal)
            })
            .skip(query.offset)
            .take(query.limit)
            .collect())
    }

    async fn validate_mappings(&self) -> Result<MappingReport> {
        let class_types = [owl("Class")];
        let property_types = [
            owl("ObjectProperty"),
            owl("DatatypeProperty"),
            owl("AnnotationProperty"),
        ];
        let schema = GraphName::Iri(GraphKind::Schema.iri());
        let mut issues = Vec::new();
        let mut checked = 0;

        for mapping in self.class_mappings()? {
            checked += 1;
            if !self.declared(&mapping.external, &class_types, None)? {
                issues.push(MappingIssue {
                    mapping: mapping.id,
                    severity: Severity::Warning,
                    message: format!(
                        "external class {} is not declared by any imported ontology",
                        mapping.external
                    ),
                });
                continue;
            }
            if !self.declared(&mapping.internal, &class_types, Some(&schema))? {
                issues.push(MappingIssue {
                    mapping: mapping.id,
                    severity: Severity::Warning,
                    message: format!(
                        "internal class {} is not declared in the schema graph",
                        mapping.internal
                    ),
                });
            }
        }
        for mapping in self.property_mappings()? {
            checked += 1;
            if !self.declared(&mapping.external, &property_types, None)? {
                issues.push(MappingIssue {
                    mapping: mapping.id,
                    severity: Severity::Warning,
                    message: format!(
                        "external property {} is not declared by any imported ontology",
                        mapping.external
                    ),
                });
                continue;
            }
            if !self.declared(&mapping.internal, &property_types, Some(&schema))? {
                issues.push(MappingIssue {
                    mapping: mapping.id,
                    severity: Severity::Warning,
                    message: format!(
                        "internal property {} is not declared in the schema graph",
                        mapping.internal
                    ),
                });
            }
        }
        Ok(MappingReport { checked, issues })
    }
}
