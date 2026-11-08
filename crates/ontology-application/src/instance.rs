//! Instance and statement management over the RDF store.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use chrono::Utc;
use ontology_core::error::{Error, Result};
use ontology_core::graph::GraphKind;
use ontology_core::id::{Id, OperationId};
use ontology_core::model::{
    GraphName, Instance, InstancePatch, InstanceQuery, InstanceSummary, Literal, NewInstance, Quad,
    QuadPattern, Relationship, RelationshipDirection, Statement, StatementInput, Term,
};
use ontology_core::namespace::{self, NamespaceRegistry};
use ontology_core::rdf::RdfStore;

/// Instance and statement operations.
///
/// Mutations default to the `data:asserted` graph; reads without an explicit
/// graph span every graph so callers can see asserted, inferred, and derived
/// statements side by side without merging them into one graph.
pub struct InstanceService {
    store: Arc<dyn RdfStore>,
}

impl InstanceService {
    /// Wraps a store.
    #[must_use]
    pub fn new(store: Arc<dyn RdfStore>) -> Self {
        Self { store }
    }

    /// Returns the underlying store.
    #[must_use]
    pub fn store(&self) -> &Arc<dyn RdfStore> {
        &self.store
    }

    /// The graph used when a mutation does not name one.
    #[must_use]
    pub fn default_graph() -> GraphName {
        GraphName::Iri(GraphKind::Asserted.iri())
    }

    /// Creates an instance.
    ///
    /// # Errors
    ///
    /// Returns [`Error::Conflict`] when the subject already has statements in
    /// the target graph and [`Error::Invalid`] when there is nothing to store.
    pub fn create_instance(&self, new: NewInstance) -> Result<Instance> {
        let graph = new.graph.clone().unwrap_or_else(Self::default_graph);
        let subject = Term::Iri(new.id.clone());
        if !self
            .store
            .get(
                QuadPattern::new()
                    .with_subject(subject.clone())
                    .with_graph(graph.clone()),
            )?
            .is_empty()
        {
            return Err(Error::Conflict {
                kind: "instance",
                id: new.id.to_string(),
            });
        }

        let mut quads = Vec::new();
        for class in &new.types {
            quads.push(Quad::in_graph(
                subject.clone(),
                Self::rdf_type(),
                Term::Iri(class.clone()),
                graph.clone(),
            ));
        }
        for (predicate, values) in &new.properties {
            for value in values {
                quads.push(Quad::in_graph(
                    subject.clone(),
                    predicate.clone(),
                    value.clone(),
                    graph.clone(),
                ));
            }
        }
        if quads.is_empty() {
            return Err(Error::invalid(
                "an instance needs at least one type or property",
            ));
        }

        let affected = quads.len();
        self.store.insert(&quads)?;
        self.record_provenance("create_instance", &graph, affected, new.source.as_deref())?;
        self.get_instance(&new.id, Some(&graph))
    }

    /// Loads an instance, optionally restricted to one graph.
    ///
    /// # Errors
    ///
    /// Returns [`Error::NotFound`] when the resource has no statements.
    pub fn get_instance(&self, id: &Id, graph: Option<&GraphName>) -> Result<Instance> {
        let quads = self.store.get(Self::subject_pattern(id, graph))?;
        if quads.is_empty() {
            return Err(Error::NotFound {
                kind: "instance",
                id: id.to_string(),
            });
        }
        let mut types: Vec<Id> = quads
            .iter()
            .filter(|quad| quad.predicate.as_str() == namespace::RDF_TYPE)
            .filter_map(|quad| match &quad.object {
                Term::Iri(class) => Some(class.clone()),
                _ => None,
            })
            .collect();
        types.sort();
        types.dedup();
        let instance_graph = graph.cloned().unwrap_or_else(|| quads[0].graph.clone());
        Ok(Instance {
            id: id.clone(),
            graph: instance_graph,
            types,
            statements: quads.iter().cloned().map(Statement::new).collect(),
        })
    }

    /// Lists typed instances, ordered by IRI.
    ///
    /// System graphs (schema, taxonomy, external) and the provenance graph are
    /// never listed: their resources are internal bookkeeping, not managed
    /// instances.
    ///
    /// # Errors
    ///
    /// Propagates storage failures.
    pub fn list_instances(&self, query: InstanceQuery) -> Result<Vec<InstanceSummary>> {
        if query.limit == 0 {
            return Ok(Vec::new());
        }
        let mut pattern = QuadPattern::new().with_predicate(Self::rdf_type());
        if let Some(graph) = &query.graph {
            pattern = pattern.with_graph(graph.clone());
        }
        let quads = self.store.get(pattern)?;
        let mut by_subject: BTreeMap<Id, Vec<Id>> = BTreeMap::new();
        for quad in quads {
            if let Some(kind) = quad.graph.as_iri().and_then(GraphKind::from_iri) {
                if kind.is_system() || kind == GraphKind::Provenance {
                    continue;
                }
            }
            let (Term::Iri(subject), Term::Iri(class)) = (&quad.subject, &quad.object) else {
                continue;
            };
            let classes = by_subject.entry(subject.clone()).or_default();
            if !classes.contains(class) {
                classes.push(class.clone());
            }
        }

        let type_filter = query.type_filter.clone();
        Ok(by_subject
            .into_iter()
            .filter(|(_, classes)| {
                type_filter
                    .as_ref()
                    .is_none_or(|filter| classes.contains(filter))
            })
            .skip(query.offset)
            .take(query.limit)
            .map(|(id, mut types)| {
                types.sort();
                InstanceSummary { id, types }
            })
            .collect())
    }

    /// Applies a partial update atomically.
    ///
    /// # Errors
    ///
    /// Returns [`Error::NotFound`] when the instance does not exist in the
    /// target graph, and propagates storage failures.
    pub fn update_instance(
        &self,
        id: &Id,
        patch: InstancePatch,
        graph: Option<&GraphName>,
        source: Option<&str>,
    ) -> Result<Instance> {
        let graph = graph.cloned().unwrap_or_else(Self::default_graph);
        let subject = Term::Iri(id.clone());
        let existing = self.store.get(
            QuadPattern::new()
                .with_subject(subject.clone())
                .with_graph(graph.clone()),
        )?;
        if existing.is_empty() {
            return Err(Error::NotFound {
                kind: "instance",
                id: id.to_string(),
            });
        }

        let mut removals = Vec::new();
        let mut additions = Vec::new();
        if let Some(types) = &patch.types {
            removals.extend(
                existing
                    .iter()
                    .filter(|quad| quad.predicate.as_str() == namespace::RDF_TYPE)
                    .cloned(),
            );
            for class in types {
                additions.push(Quad::in_graph(
                    subject.clone(),
                    Self::rdf_type(),
                    Term::Iri(class.clone()),
                    graph.clone(),
                ));
            }
        }
        for (predicate, values) in &patch.properties {
            removals.extend(
                existing
                    .iter()
                    .filter(|quad| &quad.predicate == predicate)
                    .cloned(),
            );
            if let Some(values) = values {
                for value in values {
                    additions.push(Quad::in_graph(
                        subject.clone(),
                        predicate.clone(),
                        value.clone(),
                        graph.clone(),
                    ));
                }
            }
        }

        let affected = removals.len() + additions.len();
        if !removals.is_empty() || !additions.is_empty() {
            let mut transaction = self.store.transaction()?;
            transaction.delete(&removals)?;
            transaction.insert(&additions)?;
            transaction.commit()?;
        }
        self.record_provenance("update_instance", &graph, affected, source)?;
        self.get_instance(id, Some(&graph))
    }

    /// Deletes an instance.
    ///
    /// With no graph the resource is removed from every graph it appears in.
    /// Returns the number of deleted quads.
    ///
    /// # Errors
    ///
    /// Returns [`Error::NotFound`] when the resource has no statements.
    pub fn delete_instance(&self, id: &Id, graph: Option<&GraphName>) -> Result<usize> {
        let quads = self.store.get(Self::subject_pattern(id, graph))?;
        if quads.is_empty() {
            return Err(Error::NotFound {
                kind: "instance",
                id: id.to_string(),
            });
        }
        self.store.delete(&quads)?;

        let mut per_graph: BTreeMap<GraphName, usize> = BTreeMap::new();
        for quad in &quads {
            *per_graph.entry(quad.graph.clone()).or_default() += 1;
        }
        for (graph, count) in per_graph {
            self.record_provenance("delete_instance", &graph, count, None)?;
        }
        Ok(quads.len())
    }

    /// Lists an instance's statements.
    ///
    /// # Errors
    ///
    /// Returns [`Error::NotFound`] when the resource has no statements.
    pub fn statements(&self, id: &Id, graph: Option<&GraphName>) -> Result<Vec<Statement>> {
        let quads = self.store.get(Self::subject_pattern(id, graph))?;
        if quads.is_empty() {
            return Err(Error::NotFound {
                kind: "instance",
                id: id.to_string(),
            });
        }
        Ok(quads.iter().cloned().map(Statement::new).collect())
    }

    /// Adds a batch of statements to an instance.
    ///
    /// # Errors
    ///
    /// Returns [`Error::NotFound`] when the instance does not exist in the
    /// target graph and [`Error::Validation`] for invalid terms.
    pub fn add_statements(
        &self,
        id: &Id,
        statements: &[StatementInput],
        graph: Option<&GraphName>,
        source: Option<&str>,
    ) -> Result<Vec<Statement>> {
        let graph = graph.cloned().unwrap_or_else(Self::default_graph);
        let subject = Term::Iri(id.clone());
        if self
            .store
            .get(
                QuadPattern::new()
                    .with_subject(subject.clone())
                    .with_graph(graph.clone()),
            )?
            .is_empty()
        {
            return Err(Error::NotFound {
                kind: "instance",
                id: id.to_string(),
            });
        }
        let quads: Vec<Quad> = statements
            .iter()
            .map(|statement| {
                Quad::in_graph(
                    subject.clone(),
                    statement.predicate.clone(),
                    statement.object.clone(),
                    graph.clone(),
                )
            })
            .collect();
        for quad in &quads {
            quad.validate()?;
        }
        self.store.insert(&quads)?;
        self.record_provenance("add_statements", &graph, quads.len(), source)?;
        self.statements(id, Some(&graph))
    }

    /// Deletes statements identified by their stable statement identifiers.
    ///
    /// Returns the number of deleted statements.
    ///
    /// # Errors
    ///
    /// Returns [`Error::NotFound`] when any statement identifier is unknown for
    /// this instance.
    pub fn delete_statements(
        &self,
        id: &Id,
        statement_ids: &[ontology_core::model::StatementId],
        graph: Option<&GraphName>,
    ) -> Result<usize> {
        let statements = self.statements(id, graph)?;
        let mut removals = Vec::with_capacity(statement_ids.len());
        for statement_id in statement_ids {
            let Some(statement) = statements
                .iter()
                .find(|statement| statement.id == *statement_id)
            else {
                return Err(Error::NotFound {
                    kind: "statement",
                    id: statement_id.to_string(),
                });
            };
            removals.push(statement.quad.clone());
        }
        self.store.delete(&removals)?;
        let target_graph = graph.cloned().unwrap_or_else(Self::default_graph);
        self.record_provenance("delete_statements", &target_graph, removals.len(), None)?;
        Ok(removals.len())
    }

    /// Lists incoming and outgoing relationships for a resource.
    ///
    /// # Errors
    ///
    /// Propagates storage failures.
    pub fn relationships(&self, id: &Id) -> Result<Vec<Relationship>> {
        let term = Term::Iri(id.clone());
        let mut relationships = Vec::new();
        for quad in self
            .store
            .get(QuadPattern::new().with_subject(term.clone()))?
        {
            relationships.push(Relationship {
                direction: RelationshipDirection::Outgoing,
                predicate: quad.predicate.clone(),
                target: quad.object.clone(),
                graph: quad.graph,
            });
        }
        for quad in self
            .store
            .get(QuadPattern::new().with_object(term.clone()))?
        {
            relationships.push(Relationship {
                direction: RelationshipDirection::Incoming,
                predicate: quad.predicate.clone(),
                target: quad.subject.clone(),
                graph: quad.graph,
            });
        }
        Ok(relationships)
    }

    /// Returns the distinct graphs holding statements about `id`.
    ///
    /// # Errors
    ///
    /// Propagates storage failures.
    pub fn instance_graphs(&self, id: &Id) -> Result<BTreeSet<GraphName>> {
        Ok(self
            .store
            .get(Self::subject_pattern(id, None))?
            .into_iter()
            .map(|quad| quad.graph)
            .collect())
    }

    fn subject_pattern(id: &Id, graph: Option<&GraphName>) -> QuadPattern {
        let pattern = QuadPattern::new().with_subject(Term::Iri(id.clone()));
        match graph {
            Some(graph) => pattern.with_graph(graph.clone()),
            None => pattern,
        }
    }

    fn rdf_type() -> Id {
        Id::new(namespace::RDF_TYPE).expect("rdf:type is a valid IRI")
    }

    fn record_provenance(
        &self,
        operation: &str,
        graph: &GraphName,
        affected: usize,
        source: Option<&str>,
    ) -> Result<()> {
        let record = Id::new(format!("urn:onto:provenance:{}", OperationId::generate()))?;
        let subject = Term::Iri(record);
        let provenance_graph = GraphName::Iri(GraphKind::Provenance.iri());
        let mut quads = vec![
            Quad::in_graph(
                subject.clone(),
                Self::rdf_type(),
                Term::Iri(NamespaceRegistry::onto("ProvenanceRecord")),
                provenance_graph.clone(),
            ),
            Quad::in_graph(
                subject.clone(),
                NamespaceRegistry::onto("operation"),
                Term::string(operation),
                provenance_graph.clone(),
            ),
            Quad::in_graph(
                subject.clone(),
                NamespaceRegistry::onto("targetGraph"),
                Term::string(graph.to_string()),
                provenance_graph.clone(),
            ),
            Quad::in_graph(
                subject.clone(),
                NamespaceRegistry::onto("quadCount"),
                Term::Literal(Literal::integer(affected as i64)),
                provenance_graph.clone(),
            ),
            Quad::in_graph(
                subject.clone(),
                NamespaceRegistry::onto("recordedAt"),
                Term::Literal(Literal::datetime(&Utc::now())),
                provenance_graph.clone(),
            ),
        ];
        if let Some(source) = source {
            quads.push(Quad::in_graph(
                subject,
                NamespaceRegistry::onto("source"),
                Term::string(source),
                provenance_graph,
            ));
        }
        self.store.insert(&quads)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::InstanceService;
    use ontology_core::model::{InstancePatch, InstanceQuery, NewInstance, Quad, StatementInput};
    use ontology_rdf::OxigraphStore;

    fn service() -> InstanceService {
        InstanceService::new(Arc::new(OxigraphStore::in_memory().expect("memory store")))
    }

    fn id(value: &str) -> Id {
        Id::new(format!("https://example.org/{value}")).expect("test IRI")
    }

    fn provenance_operation() -> Id {
        Id::new("urn:onto:schema:operation").expect("operation IRI")
    }

    #[test]
    fn creates_gets_and_lists_instances() {
        let service = service();
        let instance = service
            .create_instance(
                NewInstance::new(id("widget"))
                    .with_type(id("Product"))
                    .with_property(id("name"), Term::string("Widget"))
                    .with_source("unit-test"),
            )
            .expect("create");
        assert_eq!(instance.id, id("widget"));
        assert_eq!(instance.types, vec![id("Product")]);
        assert_eq!(instance.statements.len(), 2);

        assert_eq!(
            service.get_instance(&id("widget"), None).expect("get").id,
            id("widget")
        );
        let listed = service
            .list_instances(InstanceQuery::default())
            .expect("list");
        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0].id, id("widget"));

        let filtered = service
            .list_instances(InstanceQuery {
                type_filter: Some(id("Other")),
                ..InstanceQuery::default()
            })
            .expect("filtered");
        assert!(filtered.is_empty());
    }

    #[test]
    fn rejects_duplicates_and_empty_instances() {
        let service = service();
        service
            .create_instance(NewInstance::new(id("duplicate")).with_type(id("Thing")))
            .expect("first create");
        assert!(matches!(
            service.create_instance(NewInstance::new(id("duplicate")).with_type(id("Thing"))),
            Err(Error::Conflict { .. })
        ));
        assert!(matches!(
            service.create_instance(NewInstance::new(id("empty"))),
            Err(Error::Invalid { .. })
        ));
    }

    #[test]
    fn patches_replace_types_and_clear_properties_atomically() {
        let service = service();
        service
            .create_instance(
                NewInstance::new(id("patched"))
                    .with_type(id("Old"))
                    .with_property(id("name"), Term::string("Old name"))
                    .with_property(id("description"), Term::string("Old description")),
            )
            .expect("create");
        let patch = InstancePatch {
            types: Some(vec![id("New")]),
            properties: BTreeMap::from([
                (id("name"), Some(vec![Term::string("New name")])),
                (id("description"), None),
            ]),
        };
        let updated = service
            .update_instance(&id("patched"), patch, None, Some("test"))
            .expect("update");
        assert_eq!(updated.types, vec![id("New")]);
        assert!(!updated
            .statements
            .iter()
            .any(|statement| statement.quad.predicate == id("description")));
        assert!(updated.statements.iter().any(|statement| {
            statement.quad.predicate == id("name")
                && statement.quad.object == Term::string("New name")
        }));
    }

    #[test]
    fn adds_and_deletes_statements_by_stable_id() {
        let service = service();
        service
            .create_instance(NewInstance::new(id("statement")).with_type(id("Thing")))
            .expect("create");
        let statements = service
            .add_statements(
                &id("statement"),
                &[StatementInput {
                    predicate: id("name"),
                    object: Term::string("Statement"),
                }],
                None,
                None,
            )
            .expect("add");
        let name = statements
            .iter()
            .find(|statement| statement.quad.predicate == id("name"))
            .expect("name statement")
            .clone();
        assert_eq!(
            service
                .delete_statements(&id("statement"), &[name.id.clone()], None)
                .expect("delete"),
            1
        );
        assert!(!service
            .statements(&id("statement"), None)
            .expect("statements")
            .iter()
            .any(|statement| statement.quad.predicate == id("name")));
        assert!(matches!(
            service.delete_statements(&id("statement"), &[name.id.clone()], None),
            Err(Error::NotFound { .. })
        ));
    }

    #[test]
    fn writes_provenance_for_mutations() {
        let service = service();
        service
            .create_instance(NewInstance::new(id("provenance")).with_type(id("Thing")))
            .expect("create");
        let graph = GraphName::Iri(GraphKind::Provenance.iri());
        let quads = service
            .store()
            .get(
                QuadPattern::new()
                    .with_graph(graph)
                    .with_predicate(provenance_operation()),
            )
            .expect("provenance");
        assert!(quads
            .iter()
            .any(|quad| quad.object == Term::string("create_instance")));
    }

    #[test]
    fn deletes_an_instance_across_every_graph() {
        let service = service();
        service
            .create_instance(NewInstance::new(id("multi")).with_type(id("Thing")))
            .expect("create");
        service
            .store()
            .insert(&[Quad::in_graph(
                Term::Iri(id("multi")),
                id("derived"),
                Term::string("value"),
                GraphName::Iri(GraphKind::Inferred.iri()),
            )])
            .expect("insert inferred");
        assert_eq!(
            service.delete_instance(&id("multi"), None).expect("delete"),
            2
        );
        assert!(matches!(
            service.get_instance(&id("multi"), None),
            Err(Error::NotFound { .. })
        ));
    }
}
