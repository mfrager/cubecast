//! The RDF-backed SKOS taxonomy manager.

use std::collections::{BTreeMap, BTreeSet};
use std::slice;
use std::sync::Arc;

use chrono::Utc;
use ontology_core::error::{Error, Result};
use ontology_core::graph::GraphKind;
use ontology_core::id::Id;
use ontology_core::model::{GraphName, Quad, QuadPattern, Term};
use ontology_core::owl::Severity;
use ontology_core::rdf::RdfStore;
use ontology_core::skos::{
    Concept, ConceptCollection, ConceptMatch, ConceptRelation, ConceptScheme, ConceptTree,
    LabelKind, LocalizedText, SchemeVersion, SkosRelation, TaxonomyIssue, TaxonomyManager,
    TaxonomyQuery, TaxonomyReport,
};

use crate::encoding;

/// SKOS taxonomy management persisted as RDF.
///
/// Definitions are written to `system:taxonomy` by default.
pub struct SkosManager {
    store: Arc<dyn RdfStore>,
    graph: GraphName,
}

impl SkosManager {
    /// Creates a manager that stores taxonomy data in `system:taxonomy`.
    #[must_use]
    pub fn new(store: Arc<dyn RdfStore>) -> Self {
        Self {
            store,
            graph: GraphName::Iri(GraphKind::Taxonomy.iri()),
        }
    }

    /// Creates a manager that stores taxonomy data in `graph`.
    #[must_use]
    pub fn with_graph(store: Arc<dyn RdfStore>, graph: GraphName) -> Self {
        Self { store, graph }
    }

    /// The graph taxonomy operations use by default.
    #[must_use]
    pub fn graph(&self) -> &GraphName {
        &self.graph
    }

    fn replace(&self, removals: &[Quad], additions: &[Quad]) -> Result<()> {
        let mut transaction = self.store.transaction()?;
        transaction.delete(removals)?;
        transaction.insert(additions)?;
        transaction.commit()
    }

    fn all_schemes(&self) -> Result<Vec<ConceptScheme>> {
        let mut schemes = Vec::new();
        for id in encoding::scheme_ids(self.store.as_ref(), &self.graph)? {
            if let Some(scheme) = encoding::load_scheme(self.store.as_ref(), &self.graph, &id)? {
                schemes.push(scheme);
            }
        }
        schemes.sort_by(|left, right| left.id.cmp(&right.id));
        Ok(schemes)
    }

    fn all_concepts(&self) -> Result<Vec<Concept>> {
        let mut concepts = Vec::new();
        for id in encoding::concept_ids(self.store.as_ref(), &self.graph)? {
            if let Some(concept) = encoding::load_concept(self.store.as_ref(), &self.graph, &id)? {
                concepts.push(concept);
            }
        }
        concepts.sort_by(|left, right| left.id.cmp(&right.id));
        Ok(concepts)
    }

    fn require_scheme(&self, id: &Id) -> Result<ConceptScheme> {
        encoding::load_scheme(self.store.as_ref(), &self.graph, id)?.ok_or_else(|| {
            Error::NotFound {
                kind: "concept scheme",
                id: id.to_string(),
            }
        })
    }

    fn require_concept(&self, id: &Id) -> Result<Concept> {
        encoding::load_concept(self.store.as_ref(), &self.graph, id)?.ok_or_else(|| {
            Error::NotFound {
                kind: "concept",
                id: id.to_string(),
            }
        })
    }

    fn scheme_concepts(&self, scheme: &Id) -> Result<Vec<Concept>> {
        Ok(self
            .all_concepts()?
            .into_iter()
            .filter(|concept| concept.scheme.as_ref() == Some(scheme))
            .collect())
    }

    fn load_many(&self, ids: BTreeSet<Id>) -> Result<Vec<Concept>> {
        let mut concepts = Vec::new();
        for id in ids {
            if let Some(concept) = encoding::load_concept(self.store.as_ref(), &self.graph, &id)? {
                concepts.push(concept);
            }
        }
        concepts.sort_by(|left, right| left.id.cmp(&right.id));
        Ok(concepts)
    }

    fn reaches_up(&self, start: &Id, target: &Id) -> Result<bool> {
        Ok(encoding::ancestors_of(self.store.as_ref(), &self.graph, start)?.contains(target))
    }

    fn check_cycle(&self, concept: &Id, parents: &[Id]) -> Result<()> {
        for parent in parents {
            if parent == concept || self.reaches_up(parent, concept)? {
                return Err(Error::validation(format!(
                    "concept {concept} cannot be placed under {parent}: that would create a hierarchy cycle"
                )));
            }
        }
        Ok(())
    }

    fn check_hierarchy(&self, concept: &Concept) -> Result<()> {
        self.check_cycle(&concept.id, &concept.broader)?;
        for child in &concept.narrower {
            self.check_cycle(child, slice::from_ref(&concept.id))?;
        }
        Ok(())
    }

    fn normalized_labels(
        &self,
        scheme: Option<&Id>,
        exclude: Option<&Id>,
    ) -> Result<Vec<(String, Id)>> {
        let mut labels = Vec::new();
        for concept in self.all_concepts()? {
            if exclude == Some(&concept.id) || concept.scheme.as_ref() != scheme {
                continue;
            }
            for label in &concept.pref_labels {
                labels.push((label.value.trim().to_lowercase(), concept.id.clone()));
            }
        }
        Ok(labels)
    }

    fn ensure_unique_labels(
        &self,
        scheme: Option<&Id>,
        exclude: Option<&Id>,
        labels: &[LocalizedText],
    ) -> Result<()> {
        let existing = self.normalized_labels(scheme, exclude)?;
        for label in labels {
            let needle = label.value.trim().to_lowercase();
            if let Some((_, concept)) = existing.iter().find(|(value, _)| value == &needle) {
                return Err(Error::Conflict {
                    kind: "concept preferred label",
                    id: concept.to_string(),
                });
            }
        }
        Ok(())
    }

    fn edge(source: &Id, relation: SkosRelation, target: &Id) -> (Id, Id, Id) {
        let predicate = match relation {
            SkosRelation::Broader | SkosRelation::Narrower => encoding::broader(),
            other => encoding::relation_predicate(other),
        };
        let (edge_source, edge_target) = match relation {
            SkosRelation::Broader => (source.clone(), target.clone()),
            SkosRelation::Narrower => (target.clone(), source.clone()),
            SkosRelation::Related
            | SkosRelation::ExactMatch
            | SkosRelation::CloseMatch
            | SkosRelation::RelatedMatch => {
                let (left, right) = if source <= target {
                    (source.clone(), target.clone())
                } else {
                    (target.clone(), source.clone())
                };
                (left, right)
            }
            SkosRelation::BroadMatch | SkosRelation::NarrowMatch => {
                (source.clone(), target.clone())
            }
        };
        (edge_source, predicate, edge_target)
    }

    fn relation_quad(&self, source: &Id, relation: SkosRelation, target: &Id) -> Quad {
        let (edge_source, predicate, edge_target) = Self::edge(source, relation, target);
        Quad::in_graph(
            Term::Iri(edge_source),
            predicate,
            Term::Iri(edge_target),
            self.graph.clone(),
        )
    }

    fn build_tree(
        &self,
        id: &Id,
        concepts: &BTreeMap<Id, Concept>,
        children: &BTreeMap<Id, Vec<Id>>,
        seen: &mut BTreeSet<Id>,
    ) -> ConceptTree {
        seen.insert(id.clone());
        let label = concepts
            .get(id)
            .and_then(|concept| concept.pref_labels.first())
            .map(|label| label.value.clone());
        let mut nested = Vec::new();
        for child in children.get(id).into_iter().flatten() {
            if !seen.contains(child) {
                nested.push(self.build_tree(child, concepts, children, seen));
            }
        }
        ConceptTree {
            concept: id.clone(),
            label,
            children: nested,
        }
    }
}

fn score(needle: &str, value: &str) -> Option<f32> {
    let value = value.trim().to_lowercase();
    if value == needle {
        Some(1.0)
    } else if value.starts_with(needle) {
        Some(0.9)
    } else if value.contains(needle) {
        Some(0.7)
    } else {
        None
    }
}

fn label_match(
    concept: &Concept,
    needle: &str,
    labels: &[LocalizedText],
    kind: LabelKind,
    matches: &mut Vec<ConceptMatch>,
) {
    for label in labels {
        if let Some(score) = score(needle, &label.value) {
            matches.push(ConceptMatch {
                concept: concept.id.clone(),
                scheme: concept.scheme.clone(),
                label: label.value.clone(),
                language: label.language.clone(),
                kind,
                score,
            });
        }
    }
}

#[async_trait::async_trait]
impl TaxonomyManager for SkosManager {
    async fn create_scheme(&self, scheme: ConceptScheme) -> Result<Id> {
        if encoding::load_scheme(self.store.as_ref(), &self.graph, &scheme.id)?.is_some() {
            return Err(Error::Conflict {
                kind: "concept scheme",
                id: scheme.id.to_string(),
            });
        }
        let mut quads = encoding::scheme_quads(&scheme, &self.graph);
        quads.extend(encoding::scheme_version_quads(
            &scheme.id,
            scheme.version.as_deref(),
            &Utc::now(),
            &self.graph,
        )?);
        self.store.insert(&quads)?;
        Ok(scheme.id)
    }

    async fn update_scheme(&self, scheme: ConceptScheme) -> Result<Id> {
        self.require_scheme(&scheme.id)?;
        let removals = self.store.get(
            QuadPattern::new()
                .with_subject(Term::Iri(scheme.id.clone()))
                .with_graph(self.graph.clone()),
        )?;
        let mut additions = encoding::scheme_quads(&scheme, &self.graph);
        additions.extend(encoding::scheme_version_quads(
            &scheme.id,
            scheme.version.as_deref(),
            &Utc::now(),
            &self.graph,
        )?);
        self.replace(&removals, &additions)?;
        Ok(scheme.id)
    }

    async fn delete_scheme(&self, id: &Id) -> Result<usize> {
        self.require_scheme(id)?;
        let concepts = self.scheme_concepts(id)?;
        if !concepts.is_empty() {
            return Err(Error::validation(format!(
                "scheme {id} still holds {} concepts; move or delete them first",
                concepts.len()
            )));
        }
        let mut removals = self.store.get(
            QuadPattern::new()
                .with_subject(Term::Iri(id.clone()))
                .with_graph(self.graph.clone()),
        )?;
        let versions = self.store.get(
            QuadPattern::new()
                .with_predicate(encoding::dcterms_is_version_of())
                .with_object(Term::Iri(id.clone()))
                .with_graph(self.graph.clone()),
        )?;
        for version in versions {
            if let Some(record) = version.subject.as_iri() {
                removals.extend(
                    self.store.get(
                        QuadPattern::new()
                            .with_subject(Term::Iri(record.clone()))
                            .with_graph(self.graph.clone()),
                    )?,
                );
            }
        }
        self.store.delete(&removals)?;
        Ok(removals.len())
    }

    async fn get_scheme(&self, id: &Id) -> Result<ConceptScheme> {
        self.require_scheme(id)
    }

    async fn list_schemes(&self) -> Result<Vec<ConceptScheme>> {
        self.all_schemes()
    }

    async fn scheme_versions(&self, id: &Id) -> Result<Vec<SchemeVersion>> {
        self.require_scheme(id)?;
        let quads = self.store.get(
            QuadPattern::new()
                .with_predicate(encoding::dcterms_is_version_of())
                .with_object(Term::Iri(id.clone()))
                .with_graph(self.graph.clone()),
        )?;
        let subjects: BTreeSet<Id> = quads
            .iter()
            .filter_map(|quad| quad.subject.as_iri().cloned())
            .collect();
        let mut versions = Vec::new();
        for subject in subjects {
            if let Some(version) =
                encoding::load_scheme_version(self.store.as_ref(), &self.graph, &subject)?
            {
                versions.push(version);
            }
        }
        versions.sort_by(|left, right| left.recorded_at.cmp(&right.recorded_at));
        Ok(versions)
    }

    async fn create_concept(&self, concept: Concept) -> Result<Id> {
        if encoding::load_concept(self.store.as_ref(), &self.graph, &concept.id)?.is_some() {
            return Err(Error::Conflict {
                kind: "concept",
                id: concept.id.to_string(),
            });
        }
        if let Some(scheme) = &concept.scheme {
            self.require_scheme(scheme)?;
        }
        self.ensure_unique_labels(concept.scheme.as_ref(), None, &concept.pref_labels)?;
        self.check_hierarchy(&concept)?;
        self.store
            .insert(&encoding::concept_quads(&concept, &self.graph))?;
        Ok(concept.id)
    }

    async fn update_concept(&self, concept: Concept) -> Result<Id> {
        self.require_concept(&concept.id)?;
        if let Some(scheme) = &concept.scheme {
            self.require_scheme(scheme)?;
        }
        self.ensure_unique_labels(
            concept.scheme.as_ref(),
            Some(&concept.id),
            &concept.pref_labels,
        )?;
        self.check_hierarchy(&concept)?;
        let removals = self.store.get(
            QuadPattern::new()
                .with_subject(Term::Iri(concept.id.clone()))
                .with_graph(self.graph.clone()),
        )?;
        self.replace(&removals, &encoding::concept_quads(&concept, &self.graph))?;
        Ok(concept.id)
    }

    async fn delete_concept(&self, id: &Id) -> Result<usize> {
        self.require_concept(id)?;
        let removals = encoding::concept_removal_quads(self.store.as_ref(), &self.graph, id)?;
        self.store.delete(&removals)?;
        Ok(removals.len())
    }

    async fn get_concept(&self, id: &Id) -> Result<Concept> {
        self.require_concept(id)
    }

    async fn list_concepts(&self, query: TaxonomyQuery) -> Result<Vec<Concept>> {
        if query.limit == 0 {
            return Ok(Vec::new());
        }
        let needle = query
            .label_contains
            .as_ref()
            .map(|text| text.trim().to_lowercase());
        Ok(self
            .all_concepts()?
            .into_iter()
            .filter(|concept| {
                query
                    .scheme
                    .as_ref()
                    .is_none_or(|scheme| concept.scheme.as_ref() == Some(scheme))
            })
            .filter(|concept| {
                needle.as_ref().is_none_or(|needle| {
                    concept
                        .pref_labels
                        .iter()
                        .chain(&concept.alt_labels)
                        .chain(&concept.hidden_labels)
                        .any(|label| label.value.to_lowercase().contains(needle))
                        || concept
                            .notations
                            .iter()
                            .any(|notation| notation.to_lowercase().contains(needle))
                })
            })
            .skip(query.offset)
            .take(query.limit)
            .collect())
    }

    async fn search(&self, query: &str, scheme: Option<&Id>) -> Result<Vec<ConceptMatch>> {
        let needle = query.trim().to_lowercase();
        if needle.is_empty() {
            return Ok(Vec::new());
        }
        let mut matches = Vec::new();
        for concept in self.all_concepts()? {
            if scheme.is_some_and(|scheme| concept.scheme.as_ref() != Some(scheme)) {
                continue;
            }
            label_match(
                &concept,
                &needle,
                &concept.pref_labels,
                LabelKind::Preferred,
                &mut matches,
            );
            label_match(
                &concept,
                &needle,
                &concept.alt_labels,
                LabelKind::Alternative,
                &mut matches,
            );
            label_match(
                &concept,
                &needle,
                &concept.hidden_labels,
                LabelKind::Hidden,
                &mut matches,
            );
            for notation in &concept.notations {
                if let Some(score) = score(&needle, notation) {
                    matches.push(ConceptMatch {
                        concept: concept.id.clone(),
                        scheme: concept.scheme.clone(),
                        label: notation.clone(),
                        language: None,
                        kind: LabelKind::Notation,
                        score,
                    });
                }
            }
        }
        matches.sort_by(|left, right| {
            right
                .score
                .partial_cmp(&left.score)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then_with(|| left.concept.cmp(&right.concept))
                .then_with(|| left.label.cmp(&right.label))
        });
        matches.truncate(50);
        Ok(matches)
    }

    async fn add_relation(&self, source: &Id, relation: SkosRelation, target: &Id) -> Result<()> {
        self.require_concept(source)?;
        self.require_concept(target)?;
        match relation {
            SkosRelation::Broader => self.check_cycle(source, slice::from_ref(target))?,
            SkosRelation::Narrower => self.check_cycle(target, slice::from_ref(source))?,
            _ => {}
        }
        self.store
            .insert(&[self.relation_quad(source, relation, target)])?;
        Ok(())
    }

    async fn remove_relation(
        &self,
        source: &Id,
        relation: SkosRelation,
        target: &Id,
    ) -> Result<()> {
        self.require_concept(source)?;
        self.store
            .delete(&[self.relation_quad(source, relation, target)])?;
        Ok(())
    }

    async fn relations(&self, id: &Id) -> Result<Vec<ConceptRelation>> {
        self.require_concept(id)?;
        let subject = Term::Iri(id.clone());
        let outgoing = self.store.get(
            QuadPattern::new()
                .with_subject(subject.clone())
                .with_graph(self.graph.clone()),
        )?;
        let mut relations = Vec::new();
        for quad in outgoing {
            let Some(target) = quad.object.as_iri().cloned() else {
                continue;
            };
            if quad.predicate == encoding::broader() {
                relations.push(ConceptRelation {
                    source: id.clone(),
                    relation: SkosRelation::Broader,
                    target,
                });
            } else if let Some(relation) = SkosRelation::ALL.into_iter().find(|candidate| {
                !candidate.is_hierarchical()
                    && encoding::relation_predicate(*candidate) == quad.predicate
            }) {
                relations.push(ConceptRelation {
                    source: id.clone(),
                    relation,
                    target,
                });
            }
        }
        let incoming = self.store.get(
            QuadPattern::new()
                .with_predicate(encoding::broader())
                .with_object(subject.clone())
                .with_graph(self.graph.clone()),
        )?;
        for quad in incoming {
            if let Some(child) = quad.subject.as_iri().cloned() {
                relations.push(ConceptRelation {
                    source: id.clone(),
                    relation: SkosRelation::Narrower,
                    target: child,
                });
            }
        }
        for relation in [
            SkosRelation::Related,
            SkosRelation::ExactMatch,
            SkosRelation::CloseMatch,
            SkosRelation::RelatedMatch,
        ] {
            let inbound = self.store.get(
                QuadPattern::new()
                    .with_predicate(encoding::relation_predicate(relation))
                    .with_object(subject.clone())
                    .with_graph(self.graph.clone()),
            )?;
            for quad in inbound {
                if let Some(other) = quad.subject.as_iri().cloned() {
                    relations.push(ConceptRelation {
                        source: id.clone(),
                        relation,
                        target: other,
                    });
                }
            }
        }
        relations.sort_by(|left, right| {
            left.target
                .cmp(&right.target)
                .then_with(|| left.relation.cmp(&right.relation))
        });
        Ok(relations)
    }

    async fn descendants(&self, id: &Id) -> Result<Vec<Concept>> {
        self.require_concept(id)?;
        let mut ids = encoding::descendants_of(self.store.as_ref(), &self.graph, id)?;
        ids.remove(id);
        self.load_many(ids)
    }

    async fn ancestors(&self, id: &Id) -> Result<Vec<Concept>> {
        self.require_concept(id)?;
        let mut ids = encoding::ancestors_of(self.store.as_ref(), &self.graph, id)?;
        ids.remove(id);
        self.load_many(ids)
    }

    async fn tree(&self, scheme: &Id) -> Result<Vec<ConceptTree>> {
        let definition = self.require_scheme(scheme)?;
        let concepts = self.scheme_concepts(scheme)?;
        let by_id: BTreeMap<Id, Concept> = concepts
            .iter()
            .map(|concept| (concept.id.clone(), concept.clone()))
            .collect();
        let mut children: BTreeMap<Id, Vec<Id>> = BTreeMap::new();
        for concept in &concepts {
            for parent in &concept.broader {
                if by_id.contains_key(parent) {
                    children
                        .entry(parent.clone())
                        .or_default()
                        .push(concept.id.clone());
                }
            }
        }
        let mut roots: BTreeSet<Id> = definition
            .top_concepts
            .iter()
            .filter(|concept| by_id.contains_key(*concept))
            .cloned()
            .collect();
        for concept in &concepts {
            if !concept
                .broader
                .iter()
                .any(|parent| by_id.contains_key(parent))
            {
                roots.insert(concept.id.clone());
            }
        }
        let mut seen = BTreeSet::new();
        Ok(roots
            .into_iter()
            .map(|root| self.build_tree(&root, &by_id, &children, &mut seen))
            .collect())
    }

    async fn move_concept(
        &self,
        id: &Id,
        new_broader: Option<&Id>,
        scheme: Option<&Id>,
    ) -> Result<()> {
        let concept = self.require_concept(id)?;
        if let Some(scheme) = scheme {
            self.require_scheme(scheme)?;
        }
        if let Some(parent) = new_broader {
            self.require_concept(parent)?;
            self.check_cycle(id, slice::from_ref(parent))?;
        }
        let mut removals: Vec<Quad> = self.store.get(
            QuadPattern::new()
                .with_subject(Term::Iri(id.clone()))
                .with_predicate(encoding::broader())
                .with_graph(self.graph.clone()),
        )?;
        if scheme.is_some() {
            removals.extend(
                self.store.get(
                    QuadPattern::new()
                        .with_subject(Term::Iri(id.clone()))
                        .with_predicate(encoding::in_scheme())
                        .with_graph(self.graph.clone()),
                )?,
            );
        }
        let mut additions = Vec::new();
        if let Some(parent) = new_broader {
            additions.push(Quad::in_graph(
                Term::Iri(id.clone()),
                encoding::broader(),
                Term::Iri(parent.clone()),
                self.graph.clone(),
            ));
        }
        if let Some(scheme) = scheme {
            additions.push(Quad::in_graph(
                Term::Iri(id.clone()),
                encoding::in_scheme(),
                Term::Iri(scheme.clone()),
                self.graph.clone(),
            ));
        }
        let _ = concept;
        self.replace(&removals, &additions)
    }

    async fn merge_schemes(&self, source: &Id, target: &Id) -> Result<TaxonomyReport> {
        let source_scheme = self.require_scheme(source)?;
        self.require_scheme(target)?;
        if source == target {
            return Err(Error::validation("a scheme cannot be merged into itself"));
        }
        let concepts = self.scheme_concepts(source)?;
        let mut issues = Vec::new();
        let mut target_labels: BTreeSet<String> = self
            .scheme_concepts(target)?
            .iter()
            .flat_map(|concept| {
                concept
                    .pref_labels
                    .iter()
                    .map(|label| label.value.trim().to_lowercase())
            })
            .collect();
        for concept in &concepts {
            for label in &concept.pref_labels {
                let needle = label.value.trim().to_lowercase();
                if !target_labels.insert(needle) {
                    issues.push(TaxonomyIssue {
                        code: "duplicate-label".to_owned(),
                        severity: Severity::Warning,
                        concept: concept.id.clone(),
                        message: format!(
                            "preferred label {:?} already exists in the target scheme",
                            label.value
                        ),
                    });
                }
            }
        }

        let mut removals = Vec::new();
        let mut additions = Vec::new();
        for concept in &concepts {
            removals.push(Quad::in_graph(
                Term::Iri(concept.id.clone()),
                encoding::in_scheme(),
                Term::Iri(source.clone()),
                self.graph.clone(),
            ));
            additions.push(Quad::in_graph(
                Term::Iri(concept.id.clone()),
                encoding::in_scheme(),
                Term::Iri(target.clone()),
                self.graph.clone(),
            ));
        }
        for top in &source_scheme.top_concepts {
            additions.push(Quad::in_graph(
                Term::Iri(target.clone()),
                encoding::has_top_concept(),
                Term::Iri(top.clone()),
                self.graph.clone(),
            ));
        }
        removals.extend(
            self.store.get(
                QuadPattern::new()
                    .with_subject(Term::Iri(source.clone()))
                    .with_graph(self.graph.clone()),
            )?,
        );
        let versions = self.store.get(
            QuadPattern::new()
                .with_predicate(encoding::dcterms_is_version_of())
                .with_object(Term::Iri(source.clone()))
                .with_graph(self.graph.clone()),
        )?;
        for version in versions {
            if let Some(record) = version.subject.as_iri() {
                removals.extend(
                    self.store.get(
                        QuadPattern::new()
                            .with_subject(Term::Iri(record.clone()))
                            .with_graph(self.graph.clone()),
                    )?,
                );
            }
        }
        self.replace(&removals, &additions)?;
        issues.sort_by(|left, right| {
            left.concept
                .cmp(&right.concept)
                .then_with(|| left.code.cmp(&right.code))
        });
        Ok(TaxonomyReport {
            scheme: Some(target.clone()),
            checked_concepts: concepts.len(),
            issues,
        })
    }

    async fn validate(&self, scheme: Option<&Id>) -> Result<TaxonomyReport> {
        let scope = scheme.cloned();
        if let Some(scheme) = scheme {
            self.require_scheme(scheme)?;
        }
        let mut concepts = self.all_concepts()?;
        if let Some(scheme) = scheme {
            concepts.retain(|concept| concept.scheme.as_ref() == Some(scheme));
        }
        let by_id: BTreeMap<Id, Concept> = concepts
            .iter()
            .map(|concept| (concept.id.clone(), concept.clone()))
            .collect();
        let mut issues = Vec::new();
        let mut labels: BTreeMap<(String, String), Vec<Id>> = BTreeMap::new();

        for concept in &concepts {
            if concept.pref_labels.is_empty() {
                issues.push(TaxonomyIssue {
                    code: "missing-pref-label".to_owned(),
                    severity: Severity::Warning,
                    concept: concept.id.clone(),
                    message: "concept has no preferred label".to_owned(),
                });
            }
            if concept.scheme.is_none() {
                issues.push(TaxonomyIssue {
                    code: "missing-scheme".to_owned(),
                    severity: Severity::Warning,
                    concept: concept.id.clone(),
                    message: "concept does not belong to a scheme".to_owned(),
                });
            }
            for parent in &concept.broader {
                if !by_id.contains_key(parent) {
                    issues.push(TaxonomyIssue {
                        code: "unknown-broader".to_owned(),
                        severity: Severity::Warning,
                        concept: concept.id.clone(),
                        message: format!("broader concept {parent} is not defined in scope"),
                    });
                }
            }
            for label in &concept.pref_labels {
                labels
                    .entry((
                        concept
                            .scheme
                            .clone()
                            .map_or_else(String::new, |id| id.to_string()),
                        label.value.trim().to_lowercase(),
                    ))
                    .or_default()
                    .push(concept.id.clone());
            }
            for related in &concept.related {
                if self.reaches_up(&concept.id, related)?
                    || self.reaches_up(related, &concept.id)?
                {
                    issues.push(TaxonomyIssue {
                        code: "related-overlaps-hierarchy".to_owned(),
                        severity: Severity::Warning,
                        concept: concept.id.clone(),
                        message: format!(
                            "related concept {related} is also an ancestor or descendant"
                        ),
                    });
                }
            }
            let mut seen = BTreeSet::new();
            let mut stack = concept.broader.clone();
            while let Some(current) = stack.pop() {
                if !seen.insert(current.clone()) {
                    continue;
                }
                if current == concept.id {
                    issues.push(TaxonomyIssue {
                        code: "hierarchy-cycle".to_owned(),
                        severity: Severity::Error,
                        concept: concept.id.clone(),
                        message: "concept is its own ancestor".to_owned(),
                    });
                    break;
                }
                if let Some(parent) = by_id.get(&current) {
                    stack.extend(parent.broader.iter().cloned());
                }
            }
        }

        for ((_, label), concepts) in labels {
            if concepts.len() > 1 {
                for concept in concepts {
                    issues.push(TaxonomyIssue {
                        code: "duplicate-label".to_owned(),
                        severity: Severity::Warning,
                        concept,
                        message: format!(
                            "preferred label {label:?} is used by more than one concept"
                        ),
                    });
                }
            }
        }

        if let Some(scheme) = scope.as_ref() {
            let definition = self.require_scheme(scheme)?;
            for top in &definition.top_concepts {
                match by_id.get(top) {
                    None => issues.push(TaxonomyIssue {
                        code: "top-concept-not-in-scheme".to_owned(),
                        severity: Severity::Warning,
                        concept: top.clone(),
                        message: "scheme lists a top concept that is not part of it".to_owned(),
                    }),
                    Some(_) => {}
                }
            }
        }

        issues.sort_by(|left, right| {
            left.concept
                .cmp(&right.concept)
                .then_with(|| left.code.cmp(&right.code))
        });
        Ok(TaxonomyReport {
            scheme: scope,
            checked_concepts: concepts.len(),
            issues,
        })
    }

    async fn create_collection(&self, collection: ConceptCollection) -> Result<Id> {
        if encoding::load_collection(self.store.as_ref(), &self.graph, &collection.id)?.is_some() {
            return Err(Error::Conflict {
                kind: "concept collection",
                id: collection.id.to_string(),
            });
        }
        if let Some(scheme) = &collection.scheme {
            self.require_scheme(scheme)?;
        }
        self.store
            .insert(&encoding::collection_quads(&collection, &self.graph))?;
        Ok(collection.id)
    }

    async fn get_collection(&self, id: &Id) -> Result<ConceptCollection> {
        encoding::load_collection(self.store.as_ref(), &self.graph, id)?.ok_or_else(|| {
            Error::NotFound {
                kind: "concept collection",
                id: id.to_string(),
            }
        })
    }

    async fn list_collections(&self, scheme: Option<&Id>) -> Result<Vec<ConceptCollection>> {
        let mut collections = Vec::new();
        for id in encoding::collection_ids(self.store.as_ref(), &self.graph)? {
            if let Some(collection) =
                encoding::load_collection(self.store.as_ref(), &self.graph, &id)?
            {
                if scheme.is_none_or(|scheme| collection.scheme.as_ref() == Some(scheme)) {
                    collections.push(collection);
                }
            }
        }
        Ok(collections)
    }

    async fn delete_collection(&self, id: &Id) -> Result<usize> {
        self.get_collection(id).await?;
        let removals = self.store.get(
            QuadPattern::new()
                .with_subject(Term::Iri(id.clone()))
                .with_graph(self.graph.clone()),
        )?;
        self.store.delete(&removals)?;
        Ok(removals.len())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn id(value: &str) -> Id {
        Id::new(format!("https://example.org/{value}")).expect("test IRI")
    }

    #[test]
    fn canonicalizes_hierarchical_and_symmetric_edges() {
        let parent = id("parent");
        let child = id("child");

        let (source, predicate, target) = SkosManager::edge(&child, SkosRelation::Broader, &parent);
        assert_eq!((source, target), (child.clone(), parent.clone()));
        assert_eq!(predicate, encoding::broader());

        let (source, predicate, target) =
            SkosManager::edge(&child, SkosRelation::Narrower, &parent);
        assert_eq!((source, target), (parent.clone(), child.clone()));
        assert_eq!(predicate, encoding::broader());

        let (source, _, target) = SkosManager::edge(&parent, SkosRelation::Related, &child);
        assert_eq!(source, child);
        assert_eq!(target, parent);

        let (source, predicate, target) =
            SkosManager::edge(&parent, SkosRelation::BroadMatch, &child);
        assert_eq!((source, target), (parent, child));
        assert_eq!(
            predicate,
            encoding::relation_predicate(SkosRelation::BroadMatch)
        );
    }

    #[test]
    fn search_scores_exact_before_prefix_before_contains() {
        assert_eq!(score("dog", "dog"), Some(1.0));
        assert!((score("dog", "dogma").expect("prefix") - 0.9).abs() < f32::EPSILON);
        assert!((score("dog", "hotdog").expect("contains") - 0.7).abs() < f32::EPSILON);
        assert_eq!(score("dog", "cat"), None);
        assert_eq!(score("dog", " DOG "), Some(1.0));
    }

    #[test]
    fn label_matching_preserves_language_and_kind() {
        let concept = Concept::new(id("dog"))
            .with_pref_label("Dog", "en")
            .with_alt_label("Doggo", "en");
        let mut matches = Vec::new();
        label_match(
            &concept,
            "dog",
            &concept.pref_labels,
            LabelKind::Preferred,
            &mut matches,
        );
        label_match(
            &concept,
            "dog",
            &concept.alt_labels,
            LabelKind::Alternative,
            &mut matches,
        );
        assert_eq!(matches.len(), 2);
        assert_eq!(matches[0].language.as_deref(), Some("en"));
        assert!(matches
            .iter()
            .any(|hit| hit.kind == LabelKind::Preferred && hit.score == 1.0));
        assert!(matches
            .iter()
            .any(|hit| hit.kind == LabelKind::Alternative && hit.score == 0.9));
    }
}
