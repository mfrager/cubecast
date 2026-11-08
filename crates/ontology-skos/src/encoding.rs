//! Encoding of SKOS schemes, concepts, and collections as RDF quads.

use std::collections::{BTreeMap, BTreeSet};

use chrono::{DateTime, Utc};
use ontology_core::error::Result;
use ontology_core::id::{Id, OperationId};
use ontology_core::model::{GraphName, Literal, Quad, QuadPattern, Term};
use ontology_core::rdf::RdfStore;
use ontology_core::skos::{
    Concept, ConceptCollection, ConceptScheme, LocalizedText, SchemeVersion, SkosRelation,
};

const RDF: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#";
const OWL: &str = "http://www.w3.org/2002/07/owl#";
const SKOS: &str = "http://www.w3.org/2004/02/skos/core#";
const DCTERMS: &str = "http://purl.org/dc/terms/";

fn iri(namespace: &str, local: &str) -> Id {
    Id::new(format!("{namespace}{local}")).expect("well-known vocabulary IRIs are valid")
}

pub(crate) fn rdf_type() -> Id {
    iri(RDF, "type")
}

pub(crate) fn skos(local: &str) -> Id {
    iri(SKOS, local)
}

pub(crate) fn onto(local: &str) -> Id {
    Id::new(format!("urn:onto:schema:{local}")).expect("internal schema IRIs are valid")
}

pub(crate) fn owl_deprecated() -> Id {
    iri(OWL, "deprecated")
}

pub(crate) fn dcterms_is_version_of() -> Id {
    iri(DCTERMS, "isVersionOf")
}

pub(crate) fn scheme_class() -> Id {
    skos("ConceptScheme")
}

pub(crate) fn concept_class() -> Id {
    skos("Concept")
}

pub(crate) fn collection_class() -> Id {
    skos("Collection")
}

pub(crate) fn broader() -> Id {
    skos("broader")
}

pub(crate) fn related() -> Id {
    skos("related")
}

pub(crate) fn in_scheme() -> Id {
    skos("inScheme")
}

pub(crate) fn has_top_concept() -> Id {
    skos("hasTopConcept")
}

pub(crate) fn member() -> Id {
    skos("member")
}

pub(crate) fn relation_predicate(relation: SkosRelation) -> Id {
    skos(relation.local_name())
}

fn sorted(ids: &[Id]) -> Vec<Id> {
    let mut ids = ids.to_vec();
    ids.sort();
    ids.dedup();
    ids
}

fn iri_quad(subject: &Term, predicate: Id, object: &Id, graph: &GraphName) -> Quad {
    Quad::in_graph(
        subject.clone(),
        predicate,
        Term::Iri(object.clone()),
        graph.clone(),
    )
}

fn localized_quad(subject: &Term, predicate: Id, text: &LocalizedText, graph: &GraphName) -> Quad {
    let literal = match &text.language {
        Some(language) => Literal {
            value: text.value.clone(),
            language: Some(language.clone()),
            datatype: None,
        },
        None => Literal::simple(&text.value),
    };
    Quad::in_graph(
        subject.clone(),
        predicate,
        Term::Literal(literal),
        graph.clone(),
    )
}

fn texts_quads(
    subject: &Term,
    predicate: Id,
    values: &[LocalizedText],
    graph: &GraphName,
) -> Vec<Quad> {
    values
        .iter()
        .map(|text| localized_quad(subject, predicate.clone(), text, graph))
        .collect()
}

fn map_texts(values: &BTreeMap<String, String>) -> Vec<LocalizedText> {
    values
        .iter()
        .map(|(language, value)| LocalizedText::new(value, language))
        .collect()
}

fn canonical_pair(left: &Id, right: &Id) -> (Id, Id) {
    if left <= right {
        (left.clone(), right.clone())
    } else {
        (right.clone(), left.clone())
    }
}

fn deprecated_quad(subject: &Term, graph: &GraphName) -> Quad {
    Quad::in_graph(
        subject.clone(),
        owl_deprecated(),
        Term::Literal(Literal::boolean(true)),
        graph.clone(),
    )
}

/// Builds the quads for a scheme definition.
pub(crate) fn scheme_quads(scheme: &ConceptScheme, graph: &GraphName) -> Vec<Quad> {
    let subject = Term::Iri(scheme.id.clone());
    let mut quads = vec![Quad::in_graph(
        subject.clone(),
        rdf_type(),
        Term::Iri(scheme_class()),
        graph.clone(),
    )];
    quads.extend(texts_quads(
        &subject,
        skos("prefLabel"),
        &map_texts(&scheme.labels),
        graph,
    ));
    quads.extend(texts_quads(
        &subject,
        skos("definition"),
        &map_texts(&scheme.definitions),
        graph,
    ));
    for concept in sorted(&scheme.top_concepts) {
        quads.push(iri_quad(&subject, has_top_concept(), &concept, graph));
    }
    if let Some(version) = &scheme.version {
        quads.push(Quad::in_graph(
            subject.clone(),
            onto("version"),
            Term::string(version),
            graph.clone(),
        ));
    }
    quads
}

/// Builds the quads for a recorded scheme version.
///
/// # Errors
///
/// Returns an error when the generated version IRI cannot be built.
pub(crate) fn scheme_version_quads(
    scheme: &Id,
    version: Option<&str>,
    recorded_at: &DateTime<Utc>,
    graph: &GraphName,
) -> Result<Vec<Quad>> {
    let record = Id::new(format!(
        "urn:onto:schema:scheme-version:{}",
        OperationId::generate()
    ))?;
    let subject = Term::Iri(record);
    let mut quads = vec![
        Quad::in_graph(
            subject.clone(),
            rdf_type(),
            Term::Iri(onto("TaxonomyVersion")),
            graph.clone(),
        ),
        iri_quad(&subject, dcterms_is_version_of(), scheme, graph),
        Quad::in_graph(
            subject.clone(),
            onto("recordedAt"),
            Term::Literal(Literal::datetime(recorded_at)),
            graph.clone(),
        ),
    ];
    if let Some(version) = version {
        quads.push(Quad::in_graph(
            subject.clone(),
            onto("version"),
            Term::string(version),
            graph.clone(),
        ));
    }
    Ok(quads)
}

/// Builds the quads for a concept definition, canonicalizing relations.
pub(crate) fn concept_quads(concept: &Concept, graph: &GraphName) -> Vec<Quad> {
    let subject = Term::Iri(concept.id.clone());
    let mut quads = vec![Quad::in_graph(
        subject.clone(),
        rdf_type(),
        Term::Iri(concept_class()),
        graph.clone(),
    )];
    if let Some(scheme) = &concept.scheme {
        quads.push(iri_quad(&subject, in_scheme(), scheme, graph));
    }
    quads.extend(texts_quads(
        &subject,
        skos("prefLabel"),
        &concept.pref_labels,
        graph,
    ));
    quads.extend(texts_quads(
        &subject,
        skos("altLabel"),
        &concept.alt_labels,
        graph,
    ));
    quads.extend(texts_quads(
        &subject,
        skos("hiddenLabel"),
        &concept.hidden_labels,
        graph,
    ));
    quads.extend(texts_quads(
        &subject,
        skos("definition"),
        &concept.definitions,
        graph,
    ));
    for notation in &concept.notations {
        quads.push(Quad::in_graph(
            subject.clone(),
            skos("notation"),
            Term::string(notation),
            graph.clone(),
        ));
    }
    for parent in sorted(&concept.broader) {
        quads.push(iri_quad(&subject, broader(), &parent, graph));
    }
    for child in sorted(&concept.narrower) {
        quads.push(iri_quad(&Term::Iri(child), broader(), &concept.id, graph));
    }
    for other in sorted(&concept.related) {
        let (source, target) = canonical_pair(&concept.id, &other);
        quads.push(iri_quad(&Term::Iri(source), related(), &target, graph));
    }
    if concept.deprecated {
        quads.push(deprecated_quad(&subject, graph));
    }
    quads
}

/// Builds the quads for a collection definition.
pub(crate) fn collection_quads(collection: &ConceptCollection, graph: &GraphName) -> Vec<Quad> {
    let subject = Term::Iri(collection.id.clone());
    let mut quads = vec![Quad::in_graph(
        subject.clone(),
        rdf_type(),
        Term::Iri(collection_class()),
        graph.clone(),
    )];
    if let Some(scheme) = &collection.scheme {
        quads.push(iri_quad(&subject, in_scheme(), scheme, graph));
    }
    quads.extend(texts_quads(
        &subject,
        skos("prefLabel"),
        &map_texts(&collection.labels),
        graph,
    ));
    for concept in sorted(&collection.members) {
        quads.push(iri_quad(&subject, member(), &concept, graph));
    }
    quads
}

fn subject_quads(store: &dyn RdfStore, subject: &Term, graph: &GraphName) -> Result<Vec<Quad>> {
    store.get(
        QuadPattern::new()
            .with_subject(subject.clone())
            .with_graph(graph.clone()),
    )
}

fn is_typed(quads: &[Quad], class: &Id) -> bool {
    quads
        .iter()
        .any(|quad| quad.predicate == rdf_type() && quad.object == Term::Iri(class.clone()))
}

fn collect_text(quads: &[Quad], predicate: &Id, target: &mut BTreeMap<String, String>) {
    for quad in quads {
        if &quad.predicate != predicate {
            continue;
        }
        if let Term::Literal(literal) = &quad.object {
            target.insert(
                literal.language.clone().unwrap_or_default(),
                literal.value.clone(),
            );
        }
    }
}

fn collect_texts(quads: &[Quad], predicate: &Id, target: &mut Vec<LocalizedText>) {
    for quad in quads {
        if &quad.predicate != predicate {
            continue;
        }
        if let Term::Literal(literal) = &quad.object {
            target.push(LocalizedText {
                value: literal.value.clone(),
                language: literal.language.clone(),
            });
        }
    }
    target.sort();
    target.dedup();
}

fn collect_iris(quads: &[Quad], predicate: &Id) -> Vec<Id> {
    let mut values: Vec<Id> = quads
        .iter()
        .filter(|quad| &quad.predicate == predicate)
        .filter_map(|quad| quad.object.as_iri().cloned())
        .collect();
    values.sort();
    values.dedup();
    values
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

/// Loads a scheme definition, if the IRI is declared as `skos:ConceptScheme`.
///
/// # Errors
///
/// Propagates storage failures.
pub(crate) fn load_scheme(
    store: &dyn RdfStore,
    graph: &GraphName,
    id: &Id,
) -> Result<Option<ConceptScheme>> {
    let quads = subject_quads(store, &Term::Iri(id.clone()), graph)?;
    if !is_typed(&quads, &scheme_class()) {
        return Ok(None);
    }
    let mut scheme = ConceptScheme::new(id.clone());
    collect_text(&quads, &skos("prefLabel"), &mut scheme.labels);
    collect_text(&quads, &skos("definition"), &mut scheme.definitions);
    scheme.top_concepts = collect_iris(&quads, &has_top_concept());
    scheme.version = literal_value(&quads, &onto("version"));
    Ok(Some(scheme))
}

/// Loads a concept, deriving narrower and related relations from the
/// canonical edges.
///
/// # Errors
///
/// Propagates storage failures.
pub(crate) fn load_concept(
    store: &dyn RdfStore,
    graph: &GraphName,
    id: &Id,
) -> Result<Option<Concept>> {
    let quads = subject_quads(store, &Term::Iri(id.clone()), graph)?;
    if !is_typed(&quads, &concept_class()) {
        return Ok(None);
    }
    let mut concept = Concept::new(id.clone());
    concept.scheme = quads
        .iter()
        .find(|quad| quad.predicate == in_scheme())
        .and_then(|quad| quad.object.as_iri().cloned());
    collect_texts(&quads, &skos("prefLabel"), &mut concept.pref_labels);
    collect_texts(&quads, &skos("altLabel"), &mut concept.alt_labels);
    collect_texts(&quads, &skos("hiddenLabel"), &mut concept.hidden_labels);
    collect_texts(&quads, &skos("definition"), &mut concept.definitions);
    concept.notations = quads
        .iter()
        .filter(|quad| quad.predicate == skos("notation"))
        .filter_map(|quad| match &quad.object {
            Term::Literal(literal) => Some(literal.value.clone()),
            _ => None,
        })
        .collect();
    concept.notations.sort();
    concept.notations.dedup();
    concept.broader = collect_iris(&quads, &broader());

    let incoming = store.get(
        QuadPattern::new()
            .with_predicate(broader())
            .with_object(Term::Iri(id.clone()))
            .with_graph(graph.clone()),
    )?;
    let mut narrower: Vec<Id> = incoming
        .into_iter()
        .filter_map(|quad| quad.subject.into_iri())
        .filter(|child| child != id)
        .collect();
    narrower.sort();
    narrower.dedup();
    concept.narrower = narrower;

    let mut related_values = collect_iris(&quads, &related());
    let inbound_related = store.get(
        QuadPattern::new()
            .with_predicate(related())
            .with_object(Term::Iri(id.clone()))
            .with_graph(graph.clone()),
    )?;
    related_values.extend(
        inbound_related
            .into_iter()
            .filter_map(|quad| quad.subject.into_iri()),
    );
    related_values.sort();
    related_values.dedup();
    related_values.retain(|other| other != id);
    concept.related = related_values;

    concept.deprecated = quads.iter().any(|quad| {
        quad.predicate == owl_deprecated()
            && matches!(&quad.object, Term::Literal(literal) if literal.value == "true")
    });
    Ok(Some(concept))
}

/// Loads a collection definition.
///
/// # Errors
///
/// Propagates storage failures.
pub(crate) fn load_collection(
    store: &dyn RdfStore,
    graph: &GraphName,
    id: &Id,
) -> Result<Option<ConceptCollection>> {
    let quads = subject_quads(store, &Term::Iri(id.clone()), graph)?;
    if !is_typed(&quads, &collection_class()) {
        return Ok(None);
    }
    let mut collection = ConceptCollection::new(id.clone());
    collection.scheme = quads
        .iter()
        .find(|quad| quad.predicate == in_scheme())
        .and_then(|quad| quad.object.as_iri().cloned());
    collect_text(&quads, &skos("prefLabel"), &mut collection.labels);
    collection.members = collect_iris(&quads, &member());
    Ok(Some(collection))
}

/// Loads a recorded scheme version.
///
/// # Errors
///
/// Propagates storage failures.
pub(crate) fn load_scheme_version(
    store: &dyn RdfStore,
    graph: &GraphName,
    subject: &Id,
) -> Result<Option<SchemeVersion>> {
    let quads = subject_quads(store, &Term::Iri(subject.clone()), graph)?;
    let Some(scheme) = quads
        .iter()
        .find(|quad| quad.predicate == dcterms_is_version_of())
        .and_then(|quad| quad.object.as_iri().cloned())
    else {
        return Ok(None);
    };
    let recorded_at = quads
        .iter()
        .find(|quad| quad.predicate == onto("recordedAt"))
        .and_then(|quad| match &quad.object {
            Term::Literal(literal) => DateTime::parse_from_rfc3339(&literal.value).ok(),
            _ => None,
        })
        .map_or_else(Utc::now, |value| value.with_timezone(&Utc));
    Ok(Some(SchemeVersion {
        scheme,
        version: literal_value(&quads, &onto("version")),
        recorded_at,
    }))
}

fn subjects(quads: Vec<Quad>) -> Vec<Id> {
    let mut ids: Vec<Id> = quads
        .into_iter()
        .filter_map(|quad| quad.subject.into_iri())
        .collect();
    ids.sort();
    ids.dedup();
    ids
}

fn ids_typed(store: &dyn RdfStore, graph: &GraphName, class: &Id) -> Result<Vec<Id>> {
    let quads = store.get(
        QuadPattern::new()
            .with_predicate(rdf_type())
            .with_object(Term::Iri(class.clone()))
            .with_graph(graph.clone()),
    )?;
    Ok(subjects(quads))
}

/// Returns every scheme IRI declared in `graph`.
///
/// # Errors
///
/// Propagates storage failures.
pub(crate) fn scheme_ids(store: &dyn RdfStore, graph: &GraphName) -> Result<Vec<Id>> {
    ids_typed(store, graph, &scheme_class())
}

/// Returns every concept IRI declared in `graph`.
///
/// # Errors
///
/// Propagates storage failures.
pub(crate) fn concept_ids(store: &dyn RdfStore, graph: &GraphName) -> Result<Vec<Id>> {
    ids_typed(store, graph, &concept_class())
}

/// Returns every collection IRI declared in `graph`.
///
/// # Errors
///
/// Propagates storage failures.
pub(crate) fn collection_ids(store: &dyn RdfStore, graph: &GraphName) -> Result<Vec<Id>> {
    ids_typed(store, graph, &collection_class())
}

/// Collects a concept's own quads plus edges other concepts point at it with,
/// so deleting or replacing it does not leave dangling hierarchy or
/// associative relations.
///
/// # Errors
///
/// Propagates storage failures.
pub(crate) fn concept_removal_quads(
    store: &dyn RdfStore,
    graph: &GraphName,
    id: &Id,
) -> Result<Vec<Quad>> {
    let mut removals = subject_quads(store, &Term::Iri(id.clone()), graph)?;
    for predicate in [broader(), related(), member(), has_top_concept()] {
        removals.extend(
            store.get(
                QuadPattern::new()
                    .with_predicate(predicate)
                    .with_object(Term::Iri(id.clone()))
                    .with_graph(graph.clone()),
            )?,
        );
    }
    Ok(removals)
}

/// Returns the set of concept IRIs reachable upwards from `id`, including
/// `id` itself when a cycle reaches it.
///
/// # Errors
///
/// Propagates storage failures.
pub(crate) fn ancestors_of(
    store: &dyn RdfStore,
    graph: &GraphName,
    id: &Id,
) -> Result<BTreeSet<Id>> {
    let mut seen = BTreeSet::new();
    let mut stack = vec![id.clone()];
    while let Some(current) = stack.pop() {
        if !seen.insert(current.clone()) {
            continue;
        }
        let quads = store.get(
            QuadPattern::new()
                .with_subject(Term::Iri(current))
                .with_predicate(broader())
                .with_graph(graph.clone()),
        )?;
        stack.extend(quads.into_iter().filter_map(|quad| quad.object.into_iri()));
    }
    Ok(seen)
}

/// Returns the set of concept IRIs reachable downwards from `id`.
///
/// # Errors
///
/// Propagates storage failures.
pub(crate) fn descendants_of(
    store: &dyn RdfStore,
    graph: &GraphName,
    id: &Id,
) -> Result<BTreeSet<Id>> {
    let mut seen = BTreeSet::new();
    let mut stack = vec![id.clone()];
    while let Some(current) = stack.pop() {
        if !seen.insert(current.clone()) {
            continue;
        }
        let quads = store.get(
            QuadPattern::new()
                .with_predicate(broader())
                .with_object(Term::Iri(current))
                .with_graph(graph.clone()),
        )?;
        stack.extend(quads.into_iter().filter_map(|quad| quad.subject.into_iri()));
    }
    Ok(seen)
}
