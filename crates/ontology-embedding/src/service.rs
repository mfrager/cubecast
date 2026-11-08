//! Embedding orchestration: derive vectors from instance attributes, store
//! metadata as RDF, and answer similarity queries through the vector index.

use std::collections::BTreeSet;
use std::sync::Arc;

use chrono::Utc;
use ontology_core::error::{Error, Result};
use ontology_core::graph::GraphKind;
use ontology_core::id::{Id, OperationId};
use ontology_core::model::{GraphName, Literal, Quad, QuadPattern, Term};
use ontology_core::rdf::{load_instance, RdfStore};
use ontology_core::semantic::{
    EmbeddingProvider, EmbeddingRecord, SimilarityMetric, VectorFilter, VectorIndex, VectorMatch,
    VectorRecord,
};
use sha2::{Digest, Sha256};

const RDF: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#";
const XSD: &str = "http://www.w3.org/2001/XMLSchema#";
const PROV: &str = "http://www.w3.org/ns/prov#";

fn iri(value: &str) -> Id {
    Id::new(value).expect("well-known vocabulary IRIs are valid")
}

fn rdf_type() -> Id {
    iri(&format!("{RDF}type"))
}

fn onto(local: &str) -> Id {
    Id::new(format!("urn:onto:schema:{local}")).expect("internal schema IRIs are valid")
}

fn text_hash(text: &str) -> String {
    let digest = Sha256::digest(text.as_bytes());
    let mut hex = String::with_capacity(digest.len() * 2);
    for byte in digest {
        use std::fmt::Write as _;
        let _ = write!(hex, "{byte:02x}");
    }
    hex
}

/// Embedding generation and similarity search.
pub struct EmbeddingService {
    store: Arc<dyn RdfStore>,
    provider: Arc<dyn EmbeddingProvider>,
    index: Arc<dyn VectorIndex>,
}

impl EmbeddingService {
    /// Creates a service.
    #[must_use]
    pub fn new(
        store: Arc<dyn RdfStore>,
        provider: Arc<dyn EmbeddingProvider>,
        index: Arc<dyn VectorIndex>,
    ) -> Self {
        Self {
            store,
            provider,
            index,
        }
    }

    /// The provider in use.
    #[must_use]
    pub fn provider(&self) -> &Arc<dyn EmbeddingProvider> {
        &self.provider
    }

    /// The metadata graph.
    #[must_use]
    pub fn metadata_graph() -> GraphName {
        GraphName::Iri(GraphKind::Provenance.iri())
    }

    /// Generates attribute-level and aggregate embeddings for an instance.
    ///
    /// # Errors
    ///
    /// Propagates storage and provider failures.
    pub async fn generate(
        &self,
        instance: &Id,
        attributes: &[Id],
        graph: Option<&GraphName>,
    ) -> Result<Vec<EmbeddingRecord>> {
        let loaded = load_instance(self.store.as_ref(), instance, graph)?;
        let mut texts: Vec<(Option<Id>, String)> = loaded
            .attribute_texts()
            .into_iter()
            .filter(|(attribute, _)| attributes.is_empty() || attributes.contains(attribute))
            .map(|(attribute, value)| (Some(attribute), value))
            .collect();
        if texts.is_empty() {
            return Err(Error::validation(format!(
                "instance {instance} has no literal attributes to embed"
            )));
        }
        let aggregate_text = texts
            .iter()
            .map(|(_, value)| value.as_str())
            .collect::<Vec<_>>()
            .join("\n");
        texts.push((None, aggregate_text));

        let inputs: Vec<String> = texts.iter().map(|(_, text)| text.clone()).collect();
        let vectors = self.provider.embed(&inputs).await?;
        let generated_at = Utc::now();
        let mut records = Vec::new();
        for ((attribute, text), embedding) in texts.into_iter().zip(vectors) {
            let record_id = Id::new(format!("urn:onto:embedding:{}", OperationId::generate()))?;
            let record = EmbeddingRecord {
                id: record_id,
                instance: instance.clone(),
                attribute,
                model_id: self.provider.model_id().to_owned(),
                model_version: self.provider.model_version().to_owned(),
                dimensions: embedding.vector.len(),
                text_hash: text_hash(&text),
                generated_at,
                vector: embedding.vector,
            };
            self.persist(&record)?;
            self.index
                .upsert(&[VectorRecord {
                    id: record.id.clone(),
                    instance: record.instance.clone(),
                    attribute: record.attribute.clone(),
                    model_id: record.model_id.clone(),
                    vector: record.vector.clone(),
                    text_hash: record.text_hash.clone(),
                    generated_at: record.generated_at,
                }])
                .await?;
            records.push(record);
        }
        Ok(records)
    }

    /// Searches for instances semantically similar to `instance`.
    ///
    /// # Errors
    ///
    /// Propagates storage and provider failures.
    pub async fn search(
        &self,
        instance: &Id,
        limit: usize,
        filter: Option<VectorFilter>,
    ) -> Result<Vec<VectorMatch>> {
        let loaded = load_instance(self.store.as_ref(), instance, None)?;
        let text = loaded
            .attribute_texts()
            .into_iter()
            .map(|(_, value)| value)
            .collect::<Vec<_>>()
            .join("\n");
        if text.is_empty() {
            return Err(Error::validation(format!(
                "instance {instance} has no literal attributes to compare"
            )));
        }
        let vector = self
            .provider
            .embed(&[text])
            .await?
            .into_iter()
            .next()
            .ok_or_else(|| Error::validation("embedding provider returned no vector"))?;
        self.index
            .search(&vector.vector, limit, filter, SimilarityMetric::Cosine)
            .await
    }

    /// Searches the index with an explicit vector.
    ///
    /// # Errors
    ///
    /// Propagates index failures.
    pub async fn search_vector(
        &self,
        vector: &[f32],
        limit: usize,
        filter: Option<VectorFilter>,
        metric: SimilarityMetric,
    ) -> Result<Vec<VectorMatch>> {
        self.index.search(vector, limit, filter, metric).await
    }

    /// Lists embedding metadata for an instance.
    ///
    /// # Errors
    ///
    /// Propagates storage failures.
    pub async fn list(&self, instance: &Id) -> Result<Vec<EmbeddingRecord>> {
        let graph = Self::metadata_graph();
        let quads = self.store.get(
            QuadPattern::new()
                .with_predicate(onto("instance"))
                .with_object(Term::Iri(instance.clone()))
                .with_graph(graph.clone()),
        )?;
        let mut records = Vec::new();
        for subject in quads.iter().filter_map(|quad| quad.subject.as_iri()) {
            let subject_quads = self.store.get(
                QuadPattern::new()
                    .with_subject(Term::Iri(subject.clone()))
                    .with_graph(graph.clone()),
            )?;
            if let Some(record) = load_record(subject, &subject_quads) {
                records.push(record);
            }
        }
        records.sort_by(|left, right| left.id.cmp(&right.id));
        Ok(records)
    }

    /// Deletes every embedding for an instance.
    ///
    /// # Errors
    ///
    /// Propagates storage failures.
    pub async fn delete(&self, instance: &Id) -> Result<usize> {
        let graph = Self::metadata_graph();
        let records = self.list(instance).await?;
        let mut removed = 0;
        for record in &records {
            let quads = self.store.get(
                QuadPattern::new()
                    .with_subject(Term::Iri(record.id.clone()))
                    .with_graph(graph.clone()),
            )?;
            self.store.delete(&quads)?;
            removed += quads.len();
        }
        let ids: Vec<Id> = records.into_iter().map(|record| record.id).collect();
        self.index.delete(&ids).await?;
        Ok(removed)
    }

    fn persist(&self, record: &EmbeddingRecord) -> Result<()> {
        let graph = Self::metadata_graph();
        let subject = Term::Iri(record.id.clone());
        let mut quads = vec![
            Quad::in_graph(
                subject.clone(),
                rdf_type(),
                Term::Iri(onto("Embedding")),
                graph.clone(),
            ),
            Quad::in_graph(
                subject.clone(),
                onto("instance"),
                Term::Iri(record.instance.clone()),
                graph.clone(),
            ),
            Quad::in_graph(
                subject.clone(),
                onto("modelId"),
                Term::string(&record.model_id),
                graph.clone(),
            ),
            Quad::in_graph(
                subject.clone(),
                onto("dimensions"),
                Term::Literal(Literal::integer(record.dimensions as i64)),
                graph.clone(),
            ),
            Quad::in_graph(
                subject.clone(),
                onto("sourceHash"),
                Term::string(&record.text_hash),
                graph.clone(),
            ),
            Quad::in_graph(
                subject.clone(),
                onto("version"),
                Term::string(&record.model_version),
                graph.clone(),
            ),
            Quad::in_graph(
                subject.clone(),
                iri(&format!("{PROV}generatedAtTime")),
                Term::Literal(Literal::datetime(&record.generated_at)),
                graph.clone(),
            ),
        ];
        if let Some(attribute) = &record.attribute {
            quads.push(Quad::in_graph(
                subject,
                onto("attribute"),
                Term::string(attribute.as_str()),
                graph,
            ));
        }
        self.store.insert(&quads)
    }

    /// Returns the distinct instances with stored embeddings.
    ///
    /// # Errors
    ///
    /// Propagates storage failures.
    pub fn embedded_instances(&self) -> Result<BTreeSet<Id>> {
        let quads = self.store.get(
            QuadPattern::new()
                .with_predicate(onto("instance"))
                .with_graph(Self::metadata_graph()),
        )?;
        Ok(quads
            .into_iter()
            .filter_map(|quad| quad.object.into_iri())
            .collect())
    }
}

fn load_record(subject: &Id, quads: &[Quad]) -> Option<EmbeddingRecord> {
    if !quads
        .iter()
        .any(|quad| quad.predicate == rdf_type() && quad.object == Term::Iri(onto("Embedding")))
    {
        return None;
    }
    let literal = |predicate: &Id| -> Option<(String, Option<Id>)> {
        quads
            .iter()
            .find(|quad| &quad.predicate == predicate)
            .and_then(|quad| match &quad.object {
                Term::Literal(literal) => Some((literal.value.clone(), literal.datatype.clone())),
                _ => None,
            })
    };
    let instance = quads
        .iter()
        .find(|quad| quad.predicate == onto("instance"))
        .and_then(|quad| quad.object.as_iri().cloned())?;
    let model_id = literal(&onto("modelId"))?.0;
    let model_version = literal(&onto("version")).map_or_else(String::new, |pair| pair.0);
    let dimensions = literal(&onto("dimensions"))?.0.parse().ok()?;
    let text_hash = literal(&onto("sourceHash"))?.0;
    let attribute = literal(&onto("attribute")).and_then(|pair| Id::new(pair.0).ok());
    let generated_at = quads
        .iter()
        .find(|quad| quad.predicate == iri(&format!("{PROV}generatedAtTime")))
        .and_then(|quad| match &quad.object {
            Term::Literal(literal) => chrono::DateTime::parse_from_rfc3339(&literal.value).ok(),
            _ => None,
        })
        .map_or_else(Utc::now, |value| value.with_timezone(&Utc));
    let _ = iri(&format!("{XSD}decimal"));
    Some(EmbeddingRecord {
        id: subject.clone(),
        instance,
        attribute,
        model_id,
        model_version,
        dimensions,
        text_hash,
        generated_at,
        vector: Vec::new(),
    })
}
