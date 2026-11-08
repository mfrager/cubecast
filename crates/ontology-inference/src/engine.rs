//! The RDF-backed inference and enrichment engine.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use chrono::Utc;
use ontology_core::error::Result;
use ontology_core::external::{ExternalOntologyRegistry, MappingQuery};
use ontology_core::graph::GraphKind;
use ontology_core::id::{Id, OperationId};
use ontology_core::model::{GraphName, Literal, Quad, QuadPattern, Term};
use ontology_core::owl::{OntologyManager, OwlProperty, PropertyQuery};
use ontology_core::rdf::{load_instance, RdfStore};
use ontology_core::semantic::{
    ApplyPolicy, AttributeMatch, Explanation, InferenceEngine, InferenceMethod, InferenceOptions,
    InferencePreview, InferenceReport, InferredClass, InferredProperty, InferredStatement,
    Rejection,
};
use ontology_core::skos::TaxonomyManager;

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

fn asserted_graph() -> GraphName {
    GraphName::Iri(GraphKind::Asserted.iri())
}

fn inferred_graph() -> GraphName {
    GraphName::Iri(GraphKind::Inferred.iri())
}

fn provenance_graph() -> GraphName {
    GraphName::Iri(GraphKind::Provenance.iri())
}

fn is_datatype(range: &Id) -> bool {
    range.as_str().starts_with(XSD)
}

fn insert_class(
    classes: &mut BTreeMap<Id, InferredClass>,
    class: Id,
    method: InferenceMethod,
    confidence: f64,
    reason: String,
) {
    match classes.get_mut(&class) {
        Some(existing) if existing.confidence >= confidence => {}
        _ => {
            classes.insert(
                class.clone(),
                InferredClass {
                    class,
                    method,
                    confidence,
                    reason,
                },
            );
        }
    }
}

fn insert_property(
    properties: &mut BTreeMap<(Id, Term), InferredProperty>,
    property: Id,
    value: Term,
    method: InferenceMethod,
    confidence: f64,
    reason: String,
) {
    let key = (property.clone(), value.clone());
    match properties.get(&key) {
        Some(existing) if existing.confidence >= confidence => {}
        _ => {
            properties.insert(
                key,
                InferredProperty {
                    property,
                    value,
                    method,
                    confidence,
                    reason,
                },
            );
        }
    }
}

fn rule_matches(rule: &ontology_core::semantic::ClassificationRule, value: &str) -> bool {
    let value = value.to_lowercase();
    let expected = rule.value.to_lowercase();
    match rule.operator {
        ontology_core::semantic::RuleOperator::Equals => value == expected,
        ontology_core::semantic::RuleOperator::Contains => value.contains(&expected),
        ontology_core::semantic::RuleOperator::Prefix => value.starts_with(&expected),
    }
}

/// OWL, mapping, classification, and rule inference.
pub struct RuleInferenceEngine {
    store: Arc<dyn RdfStore>,
    owl: Arc<dyn OntologyManager>,
    #[allow(dead_code)]
    taxonomy: Arc<dyn TaxonomyManager>,
    external: Option<Arc<dyn ExternalOntologyRegistry>>,
}

impl RuleInferenceEngine {
    /// Creates an engine without external mappings.
    #[must_use]
    pub fn new(
        store: Arc<dyn RdfStore>,
        owl: Arc<dyn OntologyManager>,
        taxonomy: Arc<dyn TaxonomyManager>,
    ) -> Self {
        Self {
            store,
            owl,
            taxonomy,
            external: None,
        }
    }

    /// Enables external class and property mappings.
    #[must_use]
    pub fn with_external(mut self, external: Arc<dyn ExternalOntologyRegistry>) -> Self {
        self.external = Some(external);
        self
    }

    async fn approved_classifications(&self, instance: &Id) -> Result<Vec<(Id, f64)>> {
        let graph = GraphName::Iri(GraphKind::Classification.iri());
        let quads = self.store.get(
            QuadPattern::new()
                .with_predicate(onto("instance"))
                .with_object(Term::Iri(instance.clone()))
                .with_graph(graph.clone()),
        )?;
        let mut results = Vec::new();
        for subject in quads.iter().filter_map(|quad| quad.subject.as_iri()) {
            let record = self.store.get(
                QuadPattern::new()
                    .with_subject(Term::Iri(subject.clone()))
                    .with_graph(graph.clone()),
            )?;
            let status = record.iter().find_map(|quad| {
                if quad.predicate != onto("status") {
                    return None;
                }
                match &quad.object {
                    Term::Literal(literal) => Some(literal.value.clone()),
                    _ => None,
                }
            });
            if status.as_deref() != Some("approved") {
                continue;
            }
            let concept = record
                .iter()
                .find(|quad| quad.predicate == onto("concept"))
                .and_then(|quad| quad.object.as_iri().cloned());
            let confidence = record
                .iter()
                .find(|quad| quad.predicate == onto("confidence"))
                .and_then(|quad| match &quad.object {
                    Term::Literal(literal) => literal.value.parse().ok(),
                    _ => None,
                });
            if let Some(concept) = concept {
                results.push((concept, confidence.unwrap_or(1.0)));
            }
        }
        Ok(results)
    }

    fn record_provenance(&self, instance: &Id, applied: usize) -> Result<()> {
        let record = Id::new(format!("urn:onto:inference:{}", OperationId::generate()))?;
        let graph = provenance_graph();
        let subject = Term::Iri(record);
        self.store.insert(&[
            Quad::in_graph(
                subject.clone(),
                rdf_type(),
                Term::Iri(onto("Inference")),
                graph.clone(),
            ),
            Quad::in_graph(
                subject.clone(),
                onto("instance"),
                Term::Iri(instance.clone()),
                graph.clone(),
            ),
            Quad::in_graph(
                subject.clone(),
                onto("strategy"),
                Term::string("inference"),
                graph.clone(),
            ),
            Quad::in_graph(
                subject.clone(),
                onto("status"),
                Term::string("applied"),
                graph.clone(),
            ),
            Quad::in_graph(
                subject.clone(),
                onto("quadCount"),
                Term::Literal(Literal::integer(applied as i64)),
                graph.clone(),
            ),
            Quad::in_graph(
                subject,
                onto("recordedAt"),
                Term::Literal(Literal::datetime(&Utc::now())),
                graph,
            ),
        ])
    }
}

#[async_trait::async_trait]
impl InferenceEngine for RuleInferenceEngine {
    async fn infer_classes(
        &self,
        instance: &Id,
        options: InferenceOptions,
    ) -> Result<Vec<InferredClass>> {
        Ok(self.preview(instance, options).await?.classes)
    }

    async fn infer_properties(
        &self,
        instance: &Id,
        options: InferenceOptions,
    ) -> Result<Vec<InferredProperty>> {
        Ok(self.preview(instance, options).await?.properties)
    }

    async fn preview(&self, instance: &Id, options: InferenceOptions) -> Result<InferencePreview> {
        let graph = options.graph.clone().unwrap_or_else(asserted_graph);
        let quads = self.store.get(
            QuadPattern::new()
                .with_subject(Term::Iri(instance.clone()))
                .with_graph(graph.clone()),
        )?;
        let loaded = load_instance(self.store.as_ref(), instance, Some(&graph))?;
        let texts = loaded.attribute_texts();
        let asserted: BTreeSet<(Term, Id, Term)> = quads
            .iter()
            .map(|quad| {
                (
                    quad.subject.clone(),
                    quad.predicate.clone(),
                    quad.object.clone(),
                )
            })
            .collect();
        let types: BTreeSet<Id> = quads
            .iter()
            .filter(|quad| quad.predicate == rdf_type())
            .filter_map(|quad| quad.object.as_iri().cloned())
            .collect();
        let schema: BTreeMap<Id, OwlProperty> = self
            .owl
            .list_properties(PropertyQuery::new())
            .await?
            .into_iter()
            .map(|property| (property.id.clone(), property))
            .collect();

        let mut classes: BTreeMap<Id, InferredClass> = BTreeMap::new();
        let mut properties: BTreeMap<(Id, Term), InferredProperty> = BTreeMap::new();
        let mut explanation = Explanation::default();
        let target = options.target_graph.clone().unwrap_or_else(inferred_graph);
        let mut extra: BTreeMap<Quad, InferredStatement> = BTreeMap::new();

        for type_ in &types {
            if let Ok(hierarchy) = self.owl.hierarchy(type_).await {
                for ancestor in hierarchy.ancestors {
                    insert_class(
                        &mut classes,
                        ancestor.clone(),
                        InferenceMethod::Subclass,
                        1.0,
                        format!("{ancestor} is a superclass of {type_}"),
                    );
                }
            }
        }

        for quad in &quads {
            let Some(property) = schema.get(&quad.predicate) else {
                continue;
            };
            for domain in &property.domain {
                insert_class(
                    &mut classes,
                    domain.clone(),
                    InferenceMethod::Domain,
                    0.9,
                    format!("domain of {}", quad.predicate),
                );
            }
            for range in &property.range {
                if is_datatype(range) {
                    continue;
                }
                let Term::Iri(object) = &quad.object else {
                    continue;
                };
                let statement = Quad::in_graph(
                    Term::Iri(object.clone()),
                    rdf_type(),
                    Term::Iri(range.clone()),
                    target.clone(),
                );
                extra.entry(statement.clone()).or_insert(InferredStatement {
                    quad: statement,
                    method: InferenceMethod::Range,
                    confidence: 0.9,
                    reason: format!("range of {}", quad.predicate),
                });
                if object == instance {
                    insert_class(
                        &mut classes,
                        range.clone(),
                        InferenceMethod::Range,
                        0.9,
                        format!("range of {}", quad.predicate),
                    );
                }
            }
            for parent in &property.sub_property_of {
                insert_property(
                    &mut properties,
                    parent.clone(),
                    quad.object.clone(),
                    InferenceMethod::Subproperty,
                    1.0,
                    format!("{parent} is a superproperty of {}", quad.predicate),
                );
            }
        }

        if options.include_external {
            if let Some(registry) = &self.external {
                for type_ in &types {
                    let mappings = registry
                        .list_class_mappings(MappingQuery {
                            external: Some(type_.clone()),
                            internal: None,
                            limit: 100,
                            offset: 0,
                        })
                        .await?;
                    for mapping in mappings {
                        insert_class(
                            &mut classes,
                            mapping.internal.clone(),
                            InferenceMethod::Mapping,
                            mapping.confidence.unwrap_or(1.0),
                            format!("mapped from external class {type_}"),
                        );
                        explanation.mappings.push(mapping.id);
                    }
                }
                for quad in &quads {
                    let mappings = registry
                        .list_property_mappings(MappingQuery {
                            external: Some(quad.predicate.clone()),
                            internal: None,
                            limit: 100,
                            offset: 0,
                        })
                        .await?;
                    for mapping in mappings {
                        insert_property(
                            &mut properties,
                            mapping.internal.clone(),
                            quad.object.clone(),
                            InferenceMethod::Mapping,
                            mapping.confidence.unwrap_or(1.0),
                            format!("mapped from external property {}", quad.predicate),
                        );
                        explanation.mappings.push(mapping.id);
                    }
                }
            }
        }

        if options.include_classification {
            for (concept, confidence) in self.approved_classifications(instance).await? {
                insert_class(
                    &mut classes,
                    concept.clone(),
                    InferenceMethod::Classification,
                    confidence,
                    "approved classification".to_owned(),
                );
                insert_property(
                    &mut properties,
                    onto("classifiedAs"),
                    Term::Iri(concept),
                    InferenceMethod::Classification,
                    confidence,
                    "approved classification".to_owned(),
                );
            }
        }

        for rule in &options.rules {
            for (attribute, value) in &texts {
                if *attribute == rule.attribute && rule_matches(rule, value) {
                    insert_class(
                        &mut classes,
                        rule.concept.clone(),
                        InferenceMethod::Rule,
                        rule.confidence,
                        format!("rule {}", rule.name),
                    );
                    explanation.rules.push(rule.name.clone());
                    explanation.matched.push(AttributeMatch {
                        attribute: attribute.clone(),
                        value: value.clone(),
                        label: rule.name.clone(),
                    });
                }
            }
        }

        let mut statements = extra;
        for (class, inferred) in &classes {
            let quad = Quad::in_graph(
                Term::Iri(instance.clone()),
                rdf_type(),
                Term::Iri(class.clone()),
                target.clone(),
            );
            statements.entry(quad.clone()).or_insert(InferredStatement {
                quad,
                method: inferred.method,
                confidence: inferred.confidence,
                reason: inferred.reason.clone(),
            });
        }
        for ((property, value), inferred) in &properties {
            let quad = Quad::in_graph(
                Term::Iri(instance.clone()),
                property.clone(),
                value.clone(),
                target.clone(),
            );
            statements.entry(quad.clone()).or_insert(InferredStatement {
                quad,
                method: inferred.method,
                confidence: inferred.confidence,
                reason: inferred.reason.clone(),
            });
        }

        let mut kept = BTreeMap::new();
        let mut rejections = Vec::new();
        for (quad, statement) in statements {
            let key = (
                quad.subject.clone(),
                quad.predicate.clone(),
                quad.object.clone(),
            );
            if asserted.contains(&key) {
                rejections.push(Rejection {
                    concept: instance.clone(),
                    score: statement.confidence,
                    reason: "statement is already asserted".to_owned(),
                });
                continue;
            }
            kept.insert(quad, statement);
        }

        let mut applicable_properties = Vec::new();
        let mut applicable_classes = Vec::new();
        if options.include_inherited {
            if let Some(first) = types.iter().next() {
                if let Ok(properties) = self.owl.applicable_properties(first).await {
                    applicable_properties =
                        properties.into_iter().map(|property| property.id).collect();
                }
            }
            let predicates: Vec<Id> = quads
                .iter()
                .map(|quad| quad.predicate.clone())
                .collect::<BTreeSet<_>>()
                .into_iter()
                .collect();
            if let Ok(applicable) = self.owl.applicable_classes(&predicates).await {
                applicable_classes = applicable.into_iter().map(|class| class.id).collect();
            }
        }

        explanation.summary = format!(
            "{} classes and {} properties proposed for {instance}",
            classes.len(),
            properties.len()
        );
        explanation.inherited_properties = applicable_properties.clone();
        explanation.validation.push(format!(
            "{} statements proposed, {} skipped because they are already asserted",
            kept.len(),
            rejections.len()
        ));
        explanation.rejections = rejections.clone();

        Ok(InferencePreview {
            instance: instance.clone(),
            classes: classes.into_values().collect(),
            properties: properties.into_values().collect(),
            statements: kept.into_values().collect(),
            applicable_properties,
            applicable_classes,
            explanation,
            rejections,
        })
    }

    async fn apply(
        &self,
        instance: &Id,
        policy: ApplyPolicy,
        options: InferenceOptions,
    ) -> Result<InferenceReport> {
        let preview = self.preview(instance, options).await?;
        let (applied, applied_count, skipped, requires_approval) = match policy {
            ApplyPolicy::Suggest => (false, 0, preview.statements.len(), false),
            ApplyPolicy::Review => (false, 0, preview.statements.len(), true),
            ApplyPolicy::Automatic { threshold } => {
                let selected: Vec<&InferredStatement> = preview
                    .statements
                    .iter()
                    .filter(|statement| statement.confidence >= threshold)
                    .collect();
                let quads: Vec<Quad> = selected
                    .iter()
                    .map(|statement| statement.quad.clone())
                    .collect();
                let count = quads.len();
                if !quads.is_empty() {
                    self.store.insert(&quads)?;
                    self.record_provenance(instance, count)?;
                }
                (true, count, preview.statements.len() - count, false)
            }
        };
        Ok(InferenceReport {
            instance: instance.clone(),
            policy,
            preview,
            applied,
            requires_approval,
            applied_count,
            skipped,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ontology_core::semantic::{ClassificationRule, RuleOperator};

    fn id(value: &str) -> Id {
        Id::new(format!("https://example.org/{value}")).expect("test IRI")
    }

    fn rule(operator: RuleOperator, expected: &str) -> ClassificationRule {
        ClassificationRule::new(
            "test",
            id("category"),
            operator,
            expected,
            id("target"),
            0.8,
        )
    }

    #[test]
    fn rules_match_case_insensitively() {
        assert!(rule_matches(
            &rule(RuleOperator::Equals, "Storage"),
            "storage"
        ));
        assert!(rule_matches(
            &rule(RuleOperator::Contains, "drive"),
            "hard DRIVE"
        ));
        assert!(rule_matches(
            &rule(RuleOperator::Prefix, "net"),
            "Network switch"
        ));
        assert!(!rule_matches(
            &rule(RuleOperator::Equals, "storage"),
            "stored"
        ));
        assert!(!rule_matches(
            &rule(RuleOperator::Prefix, "net"),
            "ethernet"
        ));
    }

    #[test]
    fn higher_confidence_class_wins() {
        let mut classes = BTreeMap::new();
        insert_class(
            &mut classes,
            id("Thing"),
            InferenceMethod::Subclass,
            0.5,
            "first".to_owned(),
        );
        insert_class(
            &mut classes,
            id("Thing"),
            InferenceMethod::Rule,
            0.9,
            "second".to_owned(),
        );
        assert_eq!(classes[&id("Thing")].method, InferenceMethod::Rule);
        assert_eq!(classes[&id("Thing")].confidence, 0.9);
        insert_class(
            &mut classes,
            id("Thing"),
            InferenceMethod::Domain,
            0.7,
            "third".to_owned(),
        );
        assert_eq!(classes[&id("Thing")].method, InferenceMethod::Rule);
    }

    #[test]
    fn properties_are_keyed_by_predicate_and_value() {
        let mut properties = BTreeMap::new();
        insert_property(
            &mut properties,
            id("name"),
            Term::string("Widget"),
            InferenceMethod::Subproperty,
            0.6,
            "first".to_owned(),
        );
        insert_property(
            &mut properties,
            id("name"),
            Term::string("Widget"),
            InferenceMethod::Mapping,
            0.9,
            "second".to_owned(),
        );
        insert_property(
            &mut properties,
            id("name"),
            Term::string("Other"),
            InferenceMethod::Mapping,
            0.5,
            "third".to_owned(),
        );
        assert_eq!(properties.len(), 2);
        assert_eq!(
            properties[&(id("name"), Term::string("Widget"))].confidence,
            0.9
        );
    }

    #[test]
    fn recognizes_xml_schema_ranges() {
        assert!(is_datatype(
            &Id::new("http://www.w3.org/2001/XMLSchema#string").unwrap()
        ));
        assert!(!is_datatype(&id("Product")));
    }
}
