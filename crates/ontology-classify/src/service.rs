//! Classification persistence and review around the matching classifier.

use std::sync::Arc;

use chrono::Utc;
use ontology_core::error::{Error, Result};
use ontology_core::graph::GraphKind;
use ontology_core::id::{Id, OperationId};
use ontology_core::model::{GraphName, Literal, Quad, QuadPattern, Term};
use ontology_core::rdf::{load_instance, RdfStore};
use ontology_core::semantic::{
    AttributeMatch, Classification, ClassificationBatch, ClassificationOptions, ClassificationRule,
    ClassificationStrategy, Classifier, EmbeddingProvider, Explanation, ReviewStatus,
};
use ontology_core::skos::TaxonomyManager;

use crate::MatchingClassifier;

const RDF: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#";
const XSD: &str = "http://www.w3.org/2001/XMLSchema#";

fn iri(value: &str) -> Id {
    Id::new(value).expect("well-known vocabulary IRIs are valid")
}

fn rdf_type() -> Id {
    iri(&format!("{RDF}type"))
}

fn onto(local: &str) -> Id {
    Id::new(format!("urn:onto:schema:{local}")).expect("internal schema IRIs are valid")
}

fn classification_graph() -> GraphName {
    GraphName::Iri(GraphKind::Classification.iri())
}

/// Classification engine with RDF persistence.
pub struct ClassificationService {
    store: Arc<dyn RdfStore>,
    classifier: MatchingClassifier,
}

impl ClassificationService {
    /// Creates a service over the managed taxonomies.
    #[must_use]
    pub fn new(store: Arc<dyn RdfStore>, taxonomy: Arc<dyn TaxonomyManager>) -> Self {
        Self {
            store,
            classifier: MatchingClassifier::new(taxonomy),
        }
    }

    /// Enables semantic matching.
    #[must_use]
    pub fn with_embedding_provider(mut self, provider: Arc<dyn EmbeddingProvider>) -> Self {
        self.classifier = self.classifier.with_embedding_provider(provider);
        self
    }

    /// Adds attribute rules.
    #[must_use]
    pub fn with_rules(mut self, rules: Vec<ClassificationRule>) -> Self {
        self.classifier = self.classifier.with_rules(rules);
        self
    }

    /// Classifies one instance and stores the results.
    ///
    /// # Errors
    ///
    /// Propagates storage, taxonomy, and provider failures.
    pub async fn classify(
        &self,
        instance: &Id,
        options: ClassificationOptions,
    ) -> Result<Vec<Classification>> {
        let loaded = load_instance(self.store.as_ref(), instance, options.graph.as_ref())?;
        let results = self.classifier.classify(&loaded, options.clone()).await?;
        if options.persist {
            self.persist(&results)?;
        }
        Ok(results)
    }

    /// Classifies several instances and stores the results.
    ///
    /// # Errors
    ///
    /// Propagates storage, taxonomy, and provider failures.
    pub async fn classify_batch(
        &self,
        instances: &[Id],
        options: ClassificationOptions,
    ) -> Result<ClassificationBatch> {
        let mut loaded = Vec::with_capacity(instances.len());
        for instance in instances {
            loaded.push(load_instance(
                self.store.as_ref(),
                instance,
                options.graph.as_ref(),
            )?);
        }
        let batch = self
            .classifier
            .classify_batch(&loaded, options.clone())
            .await?;
        if options.persist {
            self.persist(&batch.classifications)?;
        }
        Ok(batch)
    }

    /// Lists stored classifications for an instance.
    ///
    /// # Errors
    ///
    /// Propagates storage failures.
    pub async fn list(&self, instance: &Id) -> Result<Vec<Classification>> {
        let graph = classification_graph();
        let quads = self.store.get(
            QuadPattern::new()
                .with_predicate(onto("instance"))
                .with_object(Term::Iri(instance.clone()))
                .with_graph(graph.clone()),
        )?;
        let mut results = Vec::new();
        for subject in quads.iter().filter_map(|quad| quad.subject.as_iri()) {
            let subject_quads = self.store.get(
                QuadPattern::new()
                    .with_subject(Term::Iri(subject.clone()))
                    .with_graph(graph.clone()),
            )?;
            if let Some(classification) = load_classification(subject, &subject_quads) {
                results.push(classification);
            }
        }
        results.sort_by(|left, right| left.id.cmp(&right.id));
        Ok(results)
    }

    /// Records a manual override, replacing automatic results for the concept.
    ///
    /// # Errors
    ///
    /// Propagates storage failures and rejects confidence outside `0.0..=1.0`.
    pub async fn override_classification(
        &self,
        instance: &Id,
        concept: &Id,
        confidence: f64,
    ) -> Result<Classification> {
        if !(0.0..=1.0).contains(&confidence) {
            return Err(Error::validation("confidence must be between 0.0 and 1.0"));
        }
        for existing in self
            .list(instance)
            .await?
            .into_iter()
            .filter(|record| &record.concept == concept)
        {
            self.delete_record(&existing.id)?;
        }
        let classification = Classification {
            id: Id::new(format!(
                "urn:onto:classification:{}",
                OperationId::generate()
            ))?,
            instance: instance.clone(),
            concept: concept.clone(),
            scheme: None,
            confidence,
            strategy: ClassificationStrategy::Manual,
            matched_attributes: Vec::new(),
            taxonomy_version: None,
            manual: true,
            status: ReviewStatus::Approved,
            classified_at: Utc::now(),
            explanation: Explanation {
                summary: "manual override".to_owned(),
                strategies: vec![ClassificationStrategy::Manual],
                ..Explanation::default()
            },
        };
        self.persist(std::slice::from_ref(&classification))?;
        Ok(classification)
    }

    /// Approves or rejects a classification.
    ///
    /// # Errors
    ///
    /// Returns [`Error::NotFound`] when the record does not exist.
    pub async fn review(&self, id: &Id, approved: bool) -> Result<Classification> {
        let graph = classification_graph();
        let quads = self.store.get(
            QuadPattern::new()
                .with_subject(Term::Iri(id.clone()))
                .with_graph(graph.clone()),
        )?;
        let Some(mut classification) = load_classification(id, &quads) else {
            return Err(Error::NotFound {
                kind: "classification",
                id: id.to_string(),
            });
        };
        classification.status = if approved {
            ReviewStatus::Approved
        } else {
            ReviewStatus::Rejected
        };
        let removals: Vec<Quad> = quads
            .into_iter()
            .filter(|quad| quad.predicate == onto("status"))
            .collect();
        let mut transaction = self.store.transaction()?;
        transaction.delete(&removals)?;
        transaction.insert(&[Quad::in_graph(
            Term::Iri(id.clone()),
            onto("status"),
            Term::string(classification.status.as_str()),
            graph,
        )])?;
        transaction.commit()?;
        Ok(classification)
    }

    /// Deletes automatic classifications and classifies again.
    ///
    /// # Errors
    ///
    /// Propagates storage, taxonomy, and provider failures.
    pub async fn reclassify(
        &self,
        instance: &Id,
        options: ClassificationOptions,
    ) -> Result<Vec<Classification>> {
        for existing in self
            .list(instance)
            .await?
            .into_iter()
            .filter(|record| !record.manual)
        {
            self.delete_record(&existing.id)?;
        }
        self.classify(instance, options).await
    }

    /// Deletes every classification for an instance.
    ///
    /// # Errors
    ///
    /// Propagates storage failures.
    pub async fn delete_for_instance(&self, instance: &Id) -> Result<usize> {
        let mut removed = 0;
        for record in self.list(instance).await? {
            removed += self.delete_record(&record.id)?;
        }
        Ok(removed)
    }

    fn delete_record(&self, id: &Id) -> Result<usize> {
        let quads = self.store.get(
            QuadPattern::new()
                .with_subject(Term::Iri(id.clone()))
                .with_graph(classification_graph()),
        )?;
        self.store.delete(&quads)?;
        Ok(quads.len())
    }

    fn persist(&self, classifications: &[Classification]) -> Result<()> {
        let mut quads = Vec::new();
        for classification in classifications {
            quads.extend(classification_quads(classification));
        }
        if quads.is_empty() {
            return Ok(());
        }
        self.store.insert(&quads)
    }
}

fn classification_quads(classification: &Classification) -> Vec<Quad> {
    let graph = classification_graph();
    let subject = Term::Iri(classification.id.clone());
    let mut quads = vec![
        Quad::in_graph(
            subject.clone(),
            rdf_type(),
            Term::Iri(onto("Classification")),
            graph.clone(),
        ),
        Quad::in_graph(
            subject.clone(),
            onto("instance"),
            Term::Iri(classification.instance.clone()),
            graph.clone(),
        ),
        Quad::in_graph(
            subject.clone(),
            onto("concept"),
            Term::Iri(classification.concept.clone()),
            graph.clone(),
        ),
        Quad::in_graph(
            subject.clone(),
            onto("confidence"),
            Term::Literal(Literal::typed(
                classification.confidence.to_string(),
                iri(&format!("{XSD}decimal")),
            )),
            graph.clone(),
        ),
        Quad::in_graph(
            subject.clone(),
            onto("strategy"),
            Term::string(classification.strategy.as_str()),
            graph.clone(),
        ),
        Quad::in_graph(
            subject.clone(),
            onto("status"),
            Term::string(classification.status.as_str()),
            graph.clone(),
        ),
        Quad::in_graph(
            subject.clone(),
            onto("recordedAt"),
            Term::Literal(Literal::datetime(&classification.classified_at)),
            graph.clone(),
        ),
    ];
    if let Some(scheme) = &classification.scheme {
        quads.push(Quad::in_graph(
            subject.clone(),
            onto("scheme"),
            Term::Iri(scheme.clone()),
            graph.clone(),
        ));
    }
    if classification.manual {
        quads.push(Quad::in_graph(
            subject,
            onto("manualOverride"),
            Term::Literal(Literal::boolean(true)),
            graph,
        ));
    }
    quads
}

fn load_classification(subject: &Id, quads: &[Quad]) -> Option<Classification> {
    if !quads.iter().any(|quad| {
        quad.predicate == rdf_type() && quad.object == Term::Iri(onto("Classification"))
    }) {
        return None;
    }
    let value = |predicate: &Id| -> Option<String> {
        quads
            .iter()
            .find(|quad| &quad.predicate == predicate)
            .and_then(|quad| match &quad.object {
                Term::Literal(literal) => Some(literal.value.clone()),
                _ => None,
            })
    };
    let iri_value = |predicate: &Id| -> Option<Id> {
        quads
            .iter()
            .find(|quad| &quad.predicate == predicate)
            .and_then(|quad| quad.object.as_iri().cloned())
    };
    Some(Classification {
        id: subject.clone(),
        instance: iri_value(&onto("instance"))?,
        concept: iri_value(&onto("concept"))?,
        scheme: iri_value(&onto("scheme")),
        confidence: value(&onto("confidence"))?.parse().ok()?,
        strategy: ClassificationStrategy::parse(&value(&onto("strategy"))?)?,
        matched_attributes: Vec::new(),
        taxonomy_version: None,
        manual: quads.iter().any(|quad| {
            quad.predicate == onto("manualOverride")
                && matches!(&quad.object, Term::Literal(literal) if literal.value == "true")
        }),
        status: value(&onto("status"))
            .and_then(|value| ReviewStatus::parse(&value))
            .unwrap_or(ReviewStatus::Pending),
        classified_at: quads
            .iter()
            .find(|quad| quad.predicate == onto("recordedAt"))
            .and_then(|quad| match &quad.object {
                Term::Literal(literal) => chrono::DateTime::parse_from_rfc3339(&literal.value).ok(),
                _ => None,
            })
            .map_or_else(Utc::now, |value| value.with_timezone(&Utc)),
        explanation: Explanation {
            summary: "stored classification".to_owned(),
            strategies: Vec::new(),
            matched: Vec::new(),
            rules: Vec::new(),
            inherited_properties: Vec::new(),
            mappings: Vec::new(),
            validation: Vec::new(),
            rejections: Vec::new(),
        },
    })
}

impl ClassificationService {
    /// Stored attribute matches are not persisted; this exists so callers can
    /// build a synthetic match when presenting overrides.
    #[must_use]
    pub fn manual_match(attribute: Id, value: impl Into<String>) -> AttributeMatch {
        AttributeMatch {
            attribute,
            value: value.into(),
            label: String::new(),
        }
    }
}
