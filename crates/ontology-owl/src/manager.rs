//! The RDF-backed OWL schema manager.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use chrono::Utc;
use ontology_core::error::{Error, Result};
use ontology_core::graph::GraphKind;
use ontology_core::id::Id;
use ontology_core::model::GraphName;
use ontology_core::owl::{
    ClassHierarchy, ClassQuery, GraphScope, Ontology, OntologyManager, OntologyVersion, OwlClass,
    OwlProperty, PropertyQuery, ValidationReport,
};
use ontology_core::rdf::RdfStore;

use crate::encoding;
use crate::validate;

/// OWL schema management persisted as RDF.
///
/// Definitions are written to `system:schema` by default. Reads and writes go
/// through [`RdfStore`], so the manager works with the Oxigraph adapter or any
/// other backend.
pub struct OwlManager {
    store: Arc<dyn RdfStore>,
    graph: GraphName,
}

impl OwlManager {
    /// Creates a manager that stores its schema in `system:schema`.
    #[must_use]
    pub fn new(store: Arc<dyn RdfStore>) -> Self {
        Self {
            store,
            graph: GraphName::Iri(GraphKind::Schema.iri()),
        }
    }

    /// Creates a manager that stores its schema in `graph`.
    #[must_use]
    pub fn with_graph(store: Arc<dyn RdfStore>, graph: GraphName) -> Self {
        Self { store, graph }
    }

    /// The graph schema operations use by default.
    #[must_use]
    pub fn graph(&self) -> &GraphName {
        &self.graph
    }

    fn resolved(&self, scope: &GraphScope) -> GraphName {
        match scope {
            GraphScope::Schema => self.graph.clone(),
            GraphScope::Graph(graph) => graph.clone(),
        }
    }

    fn all_classes(&self, graph: &GraphName) -> Result<Vec<OwlClass>> {
        let mut classes = Vec::new();
        for id in encoding::class_ids(self.store.as_ref(), graph)? {
            if let Some(class) = encoding::load_class(self.store.as_ref(), graph, &id)? {
                classes.push(class);
            }
        }
        classes.sort_by(|left, right| left.id.cmp(&right.id));
        Ok(classes)
    }

    fn all_properties(&self, graph: &GraphName) -> Result<Vec<OwlProperty>> {
        let mut properties = Vec::new();
        for id in encoding::property_ids(self.store.as_ref(), graph)? {
            if let Some(property) = encoding::load_property(self.store.as_ref(), graph, &id)? {
                properties.push(property);
            }
        }
        properties.sort_by(|left, right| left.id.cmp(&right.id));
        Ok(properties)
    }

    fn replace(
        &self,
        removals: &[ontology_core::model::Quad],
        additions: &[ontology_core::model::Quad],
    ) -> Result<()> {
        let mut transaction = self.store.transaction()?;
        transaction.delete(removals)?;
        transaction.insert(additions)?;
        transaction.commit()
    }

    fn ancestors(&self, class: &Id, classes: &BTreeMap<Id, OwlClass>) -> Vec<Id> {
        let mut seen = BTreeSet::new();
        let mut stack: Vec<Id> = classes
            .get(class)
            .map(|definition| definition.sub_class_of.clone())
            .unwrap_or_default();
        while let Some(current) = stack.pop() {
            if !seen.insert(current.clone()) {
                continue;
            }
            if let Some(definition) = classes.get(&current) {
                stack.extend(definition.sub_class_of.iter().cloned());
            }
        }
        seen.into_iter().collect()
    }

    fn descendants(&self, class: &Id, classes: &BTreeMap<Id, OwlClass>) -> Vec<Id> {
        let mut children: BTreeMap<Id, Vec<Id>> = BTreeMap::new();
        for definition in classes.values() {
            for parent in &definition.sub_class_of {
                children
                    .entry(parent.clone())
                    .or_default()
                    .push(definition.id.clone());
            }
        }
        let mut seen = BTreeSet::new();
        let mut stack: Vec<Id> = children.get(class).cloned().unwrap_or_default();
        while let Some(current) = stack.pop() {
            if !seen.insert(current.clone()) {
                continue;
            }
            if let Some(next) = children.get(&current) {
                stack.extend(next.iter().cloned());
            }
        }
        seen.into_iter().collect()
    }
}

#[async_trait::async_trait]
impl OntologyManager for OwlManager {
    async fn create_class(&self, class: OwlClass) -> Result<Id> {
        if encoding::load_class(self.store.as_ref(), &self.graph, &class.id)?.is_some() {
            return Err(Error::Conflict {
                kind: "owl class",
                id: class.id.to_string(),
            });
        }
        self.store
            .insert(&encoding::class_quads(&class, &self.graph))?;
        Ok(class.id)
    }

    async fn update_class(&self, class: OwlClass) -> Result<Id> {
        let existing = encoding::load_class(self.store.as_ref(), &self.graph, &class.id)?
            .ok_or_else(|| Error::NotFound {
                kind: "owl class",
                id: class.id.to_string(),
            })?;
        let removals =
            encoding::class_removal_quads(self.store.as_ref(), &self.graph, &existing.id)?;
        self.replace(&removals, &encoding::class_quads(&class, &self.graph))?;
        Ok(class.id)
    }

    async fn delete_class(&self, id: &Id) -> Result<usize> {
        if encoding::load_class(self.store.as_ref(), &self.graph, id)?.is_none() {
            return Err(Error::NotFound {
                kind: "owl class",
                id: id.to_string(),
            });
        }
        let removals = encoding::class_removal_quads(self.store.as_ref(), &self.graph, id)?;
        self.store.delete(&removals)?;
        Ok(removals.len())
    }

    async fn create_property(&self, property: OwlProperty) -> Result<Id> {
        if encoding::load_property(self.store.as_ref(), &self.graph, &property.id)?.is_some() {
            return Err(Error::Conflict {
                kind: "owl property",
                id: property.id.to_string(),
            });
        }
        self.store
            .insert(&encoding::property_quads(&property, &self.graph))?;
        Ok(property.id)
    }

    async fn update_property(&self, property: OwlProperty) -> Result<Id> {
        if encoding::load_property(self.store.as_ref(), &self.graph, &property.id)?.is_none() {
            return Err(Error::NotFound {
                kind: "owl property",
                id: property.id.to_string(),
            });
        }
        let removals = self.store.get(
            ontology_core::model::QuadPattern::new()
                .with_subject(ontology_core::model::Term::Iri(property.id.clone()))
                .with_graph(self.graph.clone()),
        )?;
        self.replace(&removals, &encoding::property_quads(&property, &self.graph))?;
        Ok(property.id)
    }

    async fn delete_property(&self, id: &Id) -> Result<usize> {
        if encoding::load_property(self.store.as_ref(), &self.graph, id)?.is_none() {
            return Err(Error::NotFound {
                kind: "owl property",
                id: id.to_string(),
            });
        }
        let removals = self.store.get(
            ontology_core::model::QuadPattern::new()
                .with_subject(ontology_core::model::Term::Iri(id.clone()))
                .with_graph(self.graph.clone()),
        )?;
        self.store.delete(&removals)?;
        Ok(removals.len())
    }

    async fn get_class(&self, id: &Id) -> Result<OwlClass> {
        encoding::load_class(self.store.as_ref(), &self.graph, id)?.ok_or_else(|| Error::NotFound {
            kind: "owl class",
            id: id.to_string(),
        })
    }

    async fn get_property(&self, id: &Id) -> Result<OwlProperty> {
        encoding::load_property(self.store.as_ref(), &self.graph, id)?.ok_or_else(|| {
            Error::NotFound {
                kind: "owl property",
                id: id.to_string(),
            }
        })
    }

    async fn list_classes(&self, query: ClassQuery) -> Result<Vec<OwlClass>> {
        if query.limit == 0 {
            return Ok(Vec::new());
        }
        let needle = query
            .label_contains
            .as_ref()
            .map(|text| text.to_lowercase());
        let classes = self.all_classes(&self.graph)?;
        Ok(classes
            .into_iter()
            .filter(|class| {
                query
                    .parent
                    .as_ref()
                    .is_none_or(|parent| class.sub_class_of.contains(parent))
            })
            .filter(|class| {
                needle.as_ref().is_none_or(|needle| {
                    class
                        .labels
                        .values()
                        .any(|label| label.to_lowercase().contains(needle))
                })
            })
            .skip(query.offset)
            .take(query.limit)
            .collect())
    }

    async fn list_properties(&self, query: PropertyQuery) -> Result<Vec<OwlProperty>> {
        if query.limit == 0 {
            return Ok(Vec::new());
        }
        let needle = query
            .label_contains
            .as_ref()
            .map(|text| text.to_lowercase());
        let properties = self.all_properties(&self.graph)?;
        Ok(properties
            .into_iter()
            .filter(|property| query.kind.is_none_or(|kind| property.kind == kind))
            .filter(|property| {
                needle.as_ref().is_none_or(|needle| {
                    property
                        .labels
                        .values()
                        .any(|label| label.to_lowercase().contains(needle))
                })
            })
            .skip(query.offset)
            .take(query.limit)
            .collect())
    }

    async fn hierarchy(&self, class: &Id) -> Result<ClassHierarchy> {
        let classes = self.all_classes(&self.graph)?;
        let map: BTreeMap<Id, OwlClass> = classes
            .iter()
            .map(|definition| (definition.id.clone(), definition.clone()))
            .collect();
        let definition = map.get(class).ok_or_else(|| Error::NotFound {
            kind: "owl class",
            id: class.to_string(),
        })?;
        let mut direct_superclasses = definition.sub_class_of.clone();
        direct_superclasses.sort();
        direct_superclasses.dedup();
        let mut direct_subclasses: Vec<Id> = classes
            .iter()
            .filter(|candidate| candidate.sub_class_of.contains(class))
            .map(|candidate| candidate.id.clone())
            .collect();
        direct_subclasses.sort();
        Ok(ClassHierarchy {
            class: class.clone(),
            direct_superclasses,
            direct_subclasses,
            ancestors: self.ancestors(class, &map),
            descendants: self.descendants(class, &map),
        })
    }

    async fn validate(&self, scope: GraphScope) -> Result<ValidationReport> {
        let graph = self.resolved(&scope);
        let classes = self.all_classes(&graph)?;
        let properties = self.all_properties(&graph)?;
        Ok(validate::validate(&scope, &classes, &properties))
    }

    async fn create_ontology(&self, ontology: Ontology) -> Result<Id> {
        if encoding::load_ontology(self.store.as_ref(), &self.graph, &ontology.id)?.is_some() {
            return Err(Error::Conflict {
                kind: "ontology",
                id: ontology.id.to_string(),
            });
        }
        let mut quads = encoding::ontology_quads(&ontology, &self.graph);
        quads.extend(encoding::version_quads(
            &ontology.id,
            ontology.version_info.as_deref(),
            ontology.version_iri.as_ref(),
            &Utc::now(),
            &self.graph,
        )?);
        self.store.insert(&quads)?;
        Ok(ontology.id)
    }

    async fn get_ontology(&self, id: &Id) -> Result<Ontology> {
        encoding::load_ontology(self.store.as_ref(), &self.graph, id)?.ok_or_else(|| {
            Error::NotFound {
                kind: "ontology",
                id: id.to_string(),
            }
        })
    }

    async fn list_ontologies(&self) -> Result<Vec<Ontology>> {
        let mut ontologies = Vec::new();
        for id in encoding::ontology_ids(self.store.as_ref(), &self.graph)? {
            if let Some(ontology) = encoding::load_ontology(self.store.as_ref(), &self.graph, &id)?
            {
                ontologies.push(ontology);
            }
        }
        ontologies.sort_by(|left, right| left.id.cmp(&right.id));
        Ok(ontologies)
    }

    async fn update_ontology(&self, ontology: Ontology) -> Result<Id> {
        if encoding::load_ontology(self.store.as_ref(), &self.graph, &ontology.id)?.is_none() {
            return Err(Error::NotFound {
                kind: "ontology",
                id: ontology.id.to_string(),
            });
        }
        let removals = self.store.get(
            ontology_core::model::QuadPattern::new()
                .with_subject(ontology_core::model::Term::Iri(ontology.id.clone()))
                .with_graph(self.graph.clone()),
        )?;
        let mut additions = encoding::ontology_quads(&ontology, &self.graph);
        additions.extend(encoding::version_quads(
            &ontology.id,
            ontology.version_info.as_deref(),
            ontology.version_iri.as_ref(),
            &Utc::now(),
            &self.graph,
        )?);
        self.replace(&removals, &additions)?;
        Ok(ontology.id)
    }

    async fn delete_ontology(&self, id: &Id) -> Result<usize> {
        if encoding::load_ontology(self.store.as_ref(), &self.graph, id)?.is_none() {
            return Err(Error::NotFound {
                kind: "ontology",
                id: id.to_string(),
            });
        }
        let pattern = ontology_core::model::QuadPattern::new().with_graph(self.graph.clone());
        let mut removals = self.store.get(
            pattern
                .clone()
                .with_subject(ontology_core::model::Term::Iri(id.clone())),
        )?;
        let versions = self.store.get(
            pattern
                .clone()
                .with_predicate(encoding::dcterms_is_version_of())
                .with_object(ontology_core::model::Term::Iri(id.clone())),
        )?;
        let subjects: BTreeSet<Id> = versions
            .iter()
            .filter_map(|quad| quad.subject.as_iri().cloned())
            .collect();
        for subject in subjects {
            removals.extend(
                self.store.get(
                    pattern
                        .clone()
                        .with_subject(ontology_core::model::Term::Iri(subject)),
                )?,
            );
        }
        self.store.delete(&removals)?;
        Ok(removals.len())
    }

    async fn ontology_versions(&self, id: &Id) -> Result<Vec<OntologyVersion>> {
        let quads = self.store.get(
            ontology_core::model::QuadPattern::new()
                .with_predicate(encoding::dcterms_is_version_of())
                .with_object(ontology_core::model::Term::Iri(id.clone()))
                .with_graph(self.graph.clone()),
        )?;
        let subjects: BTreeSet<Id> = quads
            .iter()
            .filter_map(|quad| quad.subject.as_iri().cloned())
            .collect();
        let mut versions = Vec::new();
        for subject in subjects {
            if let Some(version) =
                encoding::load_version(self.store.as_ref(), &self.graph, &subject)?
            {
                versions.push(version);
            }
        }
        versions.sort_by(|left, right| left.recorded_at.cmp(&right.recorded_at));
        Ok(versions)
    }

    async fn applicable_properties(&self, class: &Id) -> Result<Vec<OwlProperty>> {
        let classes = self.all_classes(&self.graph)?;
        let map: BTreeMap<Id, OwlClass> = classes
            .iter()
            .map(|definition| (definition.id.clone(), definition.clone()))
            .collect();
        if !map.contains_key(class) {
            return Err(Error::NotFound {
                kind: "owl class",
                id: class.to_string(),
            });
        }
        let mut applicable: BTreeSet<Id> = self.ancestors(class, &map).into_iter().collect();
        applicable.insert(class.clone());
        Ok(self
            .all_properties(&self.graph)?
            .into_iter()
            .filter(|property| {
                property.domain.is_empty()
                    || property
                        .domain
                        .iter()
                        .any(|domain| applicable.contains(domain))
            })
            .collect())
    }

    async fn applicable_classes(&self, properties: &[Id]) -> Result<Vec<OwlClass>> {
        let wanted: BTreeSet<&Id> = properties.iter().collect();
        Ok(self
            .all_classes(&self.graph)?
            .into_iter()
            .filter(|class| {
                class
                    .restrictions
                    .iter()
                    .any(|restriction| wanted.contains(restriction.property()))
            })
            .collect())
    }
}
