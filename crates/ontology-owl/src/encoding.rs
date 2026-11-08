//! Encoding of OWL definitions as RDF quads and decoding back.

use std::collections::{BTreeMap, BTreeSet};

use chrono::{DateTime, Utc};
use ontology_core::error::Result;
use ontology_core::id::{Id, OperationId};
use ontology_core::model::{GraphName, Literal, Quad, QuadPattern, Term};
use ontology_core::namespace::{self, NamespaceRegistry, OWL, RDF, RDFS, XSD};
use ontology_core::owl::{
    Ontology, OntologyVersion, OwlClass, OwlProperty, PropertyCharacteristic, PropertyKind,
    Restriction,
};
use ontology_core::rdf::RdfStore;

fn iri(namespace: &str, local: &str) -> Id {
    Id::new(format!("{namespace}{local}")).expect("well-known vocabulary IRIs are valid")
}

pub(crate) fn rdf_type() -> Id {
    iri(RDF, "type")
}

pub(crate) fn rdfs_label() -> Id {
    iri(RDFS, "label")
}

pub(crate) fn rdfs_comment() -> Id {
    iri(RDFS, "comment")
}

pub(crate) fn rdfs_sub_class_of() -> Id {
    iri(RDFS, "subClassOf")
}

pub(crate) fn rdfs_sub_property_of() -> Id {
    iri(RDFS, "subPropertyOf")
}

pub(crate) fn rdfs_domain() -> Id {
    iri(RDFS, "domain")
}

pub(crate) fn rdfs_range() -> Id {
    iri(RDFS, "range")
}

pub(crate) fn rdfs_literal() -> Id {
    iri(RDFS, "Literal")
}

pub(crate) fn owl_class() -> Id {
    iri(OWL, "Class")
}

pub(crate) fn owl_ontology() -> Id {
    iri(OWL, "Ontology")
}

pub(crate) fn owl_restriction() -> Id {
    iri(OWL, "Restriction")
}

pub(crate) fn owl_equivalent_class() -> Id {
    iri(OWL, "equivalentClass")
}

pub(crate) fn owl_disjoint_with() -> Id {
    iri(OWL, "disjointWith")
}

pub(crate) fn owl_equivalent_property() -> Id {
    iri(OWL, "equivalentProperty")
}

pub(crate) fn owl_inverse_of() -> Id {
    iri(OWL, "inverseOf")
}

pub(crate) fn owl_deprecated() -> Id {
    iri(OWL, "deprecated")
}

pub(crate) fn owl_on_property() -> Id {
    iri(OWL, "onProperty")
}

pub(crate) fn owl_on_class() -> Id {
    iri(OWL, "onClass")
}

pub(crate) fn owl_some_values_from() -> Id {
    iri(OWL, "someValuesFrom")
}

pub(crate) fn owl_all_values_from() -> Id {
    iri(OWL, "allValuesFrom")
}

pub(crate) fn owl_has_value() -> Id {
    iri(OWL, "hasValue")
}

pub(crate) fn owl_min_cardinality() -> Id {
    iri(OWL, "minCardinality")
}

pub(crate) fn owl_max_cardinality() -> Id {
    iri(OWL, "maxCardinality")
}

pub(crate) fn owl_cardinality() -> Id {
    iri(OWL, "cardinality")
}

pub(crate) fn owl_min_qualified_cardinality() -> Id {
    iri(OWL, "minQualifiedCardinality")
}

pub(crate) fn owl_max_qualified_cardinality() -> Id {
    iri(OWL, "maxQualifiedCardinality")
}

pub(crate) fn owl_qualified_cardinality() -> Id {
    iri(OWL, "qualifiedCardinality")
}

pub(crate) fn owl_version_info() -> Id {
    iri(OWL, "versionInfo")
}

pub(crate) fn owl_version_iri() -> Id {
    iri(OWL, "versionIRI")
}

pub(crate) fn owl_imports() -> Id {
    iri(OWL, "imports")
}

pub(crate) fn dcterms_is_version_of() -> Id {
    iri(namespace::DCTERMS, "isVersionOf")
}

pub(crate) fn onto_term(local: &str) -> Id {
    NamespaceRegistry::onto(local)
}

fn xsd_non_negative_integer() -> Id {
    iri(XSD, "nonNegativeInteger")
}

/// Returns the OWL class IRI for a property kind.
pub(crate) fn property_type_iri(kind: PropertyKind) -> Id {
    iri(OWL, kind.local_name())
}

fn characteristic_iri(characteristic: PropertyCharacteristic) -> Id {
    iri(OWL, characteristic.local_name())
}

fn sorted(ids: &[Id]) -> Vec<Id> {
    let mut ids = ids.to_vec();
    ids.sort();
    ids.dedup();
    ids
}

fn literal_for_label(value: &str, language: &str) -> Literal {
    if language.is_empty() {
        Literal::simple(value)
    } else {
        Literal {
            value: value.to_owned(),
            language: Some(language.to_owned()),
            datatype: None,
        }
    }
}

fn text_quads(
    subject: &Term,
    predicate: Id,
    values: &BTreeMap<String, String>,
    graph: &GraphName,
) -> Vec<Quad> {
    values
        .iter()
        .map(|(language, value)| {
            Quad::in_graph(
                subject.clone(),
                predicate.clone(),
                Term::Literal(literal_for_label(value, language)),
                graph.clone(),
            )
        })
        .collect()
}

fn iri_quad(subject: &Term, predicate: Id, object: &Id, graph: &GraphName) -> Quad {
    Quad::in_graph(
        subject.clone(),
        predicate,
        Term::Iri(object.clone()),
        graph.clone(),
    )
}

fn deprecated_quad(subject: &Term, graph: &GraphName) -> Quad {
    Quad::in_graph(
        subject.clone(),
        owl_deprecated(),
        Term::Literal(Literal::boolean(true)),
        graph.clone(),
    )
}

fn restriction_quads(restriction: &Restriction, node: &str, graph: &GraphName) -> Vec<Quad> {
    let subject = Term::BlankNode(node.to_owned());
    let mut quads = vec![
        Quad::in_graph(
            subject.clone(),
            rdf_type(),
            Term::Iri(owl_restriction()),
            graph.clone(),
        ),
        iri_quad(&subject, owl_on_property(), restriction.property(), graph),
    ];
    let cardinality_quad = |predicate: Id, cardinality: u32, class: Option<&Id>| {
        let mut quads = vec![Quad::in_graph(
            subject.clone(),
            predicate,
            Term::Literal(Literal::typed(
                cardinality.to_string(),
                xsd_non_negative_integer(),
            )),
            graph.clone(),
        )];
        if let Some(class) = class {
            quads.push(iri_quad(&subject, owl_on_class(), class, graph));
        }
        quads
    };
    match restriction {
        Restriction::SomeValuesFrom { class, .. } => {
            quads.push(iri_quad(&subject, owl_some_values_from(), class, graph));
        }
        Restriction::AllValuesFrom { class, .. } => {
            quads.push(iri_quad(&subject, owl_all_values_from(), class, graph));
        }
        Restriction::HasValue { value, .. } => {
            quads.push(Quad::in_graph(
                subject.clone(),
                owl_has_value(),
                value.clone(),
                graph.clone(),
            ));
        }
        Restriction::MinCardinality {
            cardinality, class, ..
        } => {
            let predicate = if class.is_some() {
                owl_min_qualified_cardinality()
            } else {
                owl_min_cardinality()
            };
            quads.extend(cardinality_quad(predicate, *cardinality, class.as_ref()));
        }
        Restriction::MaxCardinality {
            cardinality, class, ..
        } => {
            let predicate = if class.is_some() {
                owl_max_qualified_cardinality()
            } else {
                owl_max_cardinality()
            };
            quads.extend(cardinality_quad(predicate, *cardinality, class.as_ref()));
        }
        Restriction::ExactCardinality {
            cardinality, class, ..
        } => {
            let predicate = if class.is_some() {
                owl_qualified_cardinality()
            } else {
                owl_cardinality()
            };
            quads.extend(cardinality_quad(predicate, *cardinality, class.as_ref()));
        }
    }
    quads
}

/// Builds the quads for a class definition.
pub(crate) fn class_quads(class: &OwlClass, graph: &GraphName) -> Vec<Quad> {
    let subject = Term::Iri(class.id.clone());
    let mut quads = vec![Quad::in_graph(
        subject.clone(),
        rdf_type(),
        Term::Iri(owl_class()),
        graph.clone(),
    )];
    quads.extend(text_quads(&subject, rdfs_label(), &class.labels, graph));
    quads.extend(text_quads(&subject, rdfs_comment(), &class.comments, graph));
    for superclass in sorted(&class.sub_class_of) {
        quads.push(iri_quad(&subject, rdfs_sub_class_of(), &superclass, graph));
    }
    for equivalent in sorted(&class.equivalent_classes) {
        quads.push(iri_quad(
            &subject,
            owl_equivalent_class(),
            &equivalent,
            graph,
        ));
    }
    for disjoint in sorted(&class.disjoint_with) {
        quads.push(iri_quad(&subject, owl_disjoint_with(), &disjoint, graph));
    }
    for restriction in &class.restrictions {
        let node = format!("r{}", OperationId::generate());
        quads.extend(restriction_quads(restriction, &node, graph));
        quads.push(Quad::in_graph(
            subject.clone(),
            rdfs_sub_class_of(),
            Term::BlankNode(node),
            graph.clone(),
        ));
    }
    if class.deprecated {
        quads.push(deprecated_quad(&subject, graph));
    }
    quads
}

/// Builds the quads for a property definition.
pub(crate) fn property_quads(property: &OwlProperty, graph: &GraphName) -> Vec<Quad> {
    let subject = Term::Iri(property.id.clone());
    let mut quads = vec![Quad::in_graph(
        subject.clone(),
        rdf_type(),
        Term::Iri(property_type_iri(property.kind)),
        graph.clone(),
    )];
    quads.extend(text_quads(&subject, rdfs_label(), &property.labels, graph));
    quads.extend(text_quads(
        &subject,
        rdfs_comment(),
        &property.comments,
        graph,
    ));
    for parent in sorted(&property.sub_property_of) {
        quads.push(iri_quad(&subject, rdfs_sub_property_of(), &parent, graph));
    }
    for equivalent in sorted(&property.equivalent_properties) {
        quads.push(iri_quad(
            &subject,
            owl_equivalent_property(),
            &equivalent,
            graph,
        ));
    }
    for domain in sorted(&property.domain) {
        quads.push(iri_quad(&subject, rdfs_domain(), &domain, graph));
    }
    for range in sorted(&property.range) {
        quads.push(iri_quad(&subject, rdfs_range(), &range, graph));
    }
    if let Some(inverse) = &property.inverse_of {
        quads.push(iri_quad(&subject, owl_inverse_of(), inverse, graph));
    }
    for characteristic in &property.characteristics {
        quads.push(Quad::in_graph(
            subject.clone(),
            rdf_type(),
            Term::Iri(characteristic_iri(*characteristic)),
            graph.clone(),
        ));
    }
    if property.deprecated {
        quads.push(deprecated_quad(&subject, graph));
    }
    quads
}

/// Builds the quads for an ontology resource.
pub(crate) fn ontology_quads(ontology: &Ontology, graph: &GraphName) -> Vec<Quad> {
    let subject = Term::Iri(ontology.id.clone());
    let mut quads = vec![Quad::in_graph(
        subject.clone(),
        rdf_type(),
        Term::Iri(owl_ontology()),
        graph.clone(),
    )];
    quads.extend(text_quads(&subject, rdfs_label(), &ontology.labels, graph));
    quads.extend(text_quads(
        &subject,
        rdfs_comment(),
        &ontology.comments,
        graph,
    ));
    if let Some(version_info) = &ontology.version_info {
        quads.push(Quad::in_graph(
            subject.clone(),
            owl_version_info(),
            Term::string(version_info),
            graph.clone(),
        ));
    }
    if let Some(version_iri) = &ontology.version_iri {
        quads.push(iri_quad(&subject, owl_version_iri(), version_iri, graph));
    }
    for import in sorted(&ontology.imports) {
        quads.push(iri_quad(&subject, owl_imports(), &import, graph));
    }
    quads
}

/// Builds a version record for an ontology.
///
/// # Errors
///
/// Returns an error when the generated version IRI cannot be built.
pub(crate) fn version_quads(
    ontology: &Id,
    version_info: Option<&str>,
    version_iri: Option<&Id>,
    recorded_at: &DateTime<Utc>,
    graph: &GraphName,
) -> Result<Vec<Quad>> {
    let record = Id::new(format!(
        "urn:onto:schema:version:{}",
        OperationId::generate()
    ))?;
    let subject = Term::Iri(record);
    let mut quads = vec![
        Quad::in_graph(
            subject.clone(),
            rdf_type(),
            Term::Iri(onto_term("OntologyVersion")),
            graph.clone(),
        ),
        iri_quad(&subject, dcterms_is_version_of(), ontology, graph),
        Quad::in_graph(
            subject.clone(),
            onto_term("recordedAt"),
            Term::Literal(Literal::datetime(recorded_at)),
            graph.clone(),
        ),
    ];
    if let Some(version_info) = version_info {
        quads.push(Quad::in_graph(
            subject.clone(),
            onto_term("version"),
            Term::string(version_info),
            graph.clone(),
        ));
    }
    if let Some(version_iri) = version_iri {
        quads.push(iri_quad(&subject, owl_version_iri(), version_iri, graph));
    }
    Ok(quads)
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

/// Loads a class definition, if the IRI is declared as `owl:Class`.
///
/// # Errors
///
/// Propagates storage failures.
pub(crate) fn load_class(
    store: &dyn RdfStore,
    graph: &GraphName,
    id: &Id,
) -> Result<Option<OwlClass>> {
    let quads = subject_quads(store, &Term::Iri(id.clone()), graph)?;
    if !is_typed(&quads, &owl_class()) {
        return Ok(None);
    }
    let mut class = OwlClass::new(id.clone());
    collect_text(&quads, &rdfs_label(), &mut class.labels);
    collect_text(&quads, &rdfs_comment(), &mut class.comments);
    class.sub_class_of = collect_iris(&quads, &rdfs_sub_class_of());
    class.equivalent_classes = collect_iris(&quads, &owl_equivalent_class());
    class.disjoint_with = collect_iris(&quads, &owl_disjoint_with());
    class.deprecated = quads.iter().any(|quad| {
        quad.predicate == owl_deprecated()
            && matches!(&quad.object, Term::Literal(literal) if literal.value == "true")
    });

    let mut restrictions = Vec::new();
    for quad in &quads {
        if quad.predicate != rdfs_sub_class_of() {
            continue;
        }
        if let Term::BlankNode(node) = &quad.object {
            if let Some(restriction) = load_restriction(store, graph, node)? {
                restrictions.push(restriction);
            }
        }
    }
    class.restrictions = restrictions;
    Ok(Some(class))
}

fn load_restriction(
    store: &dyn RdfStore,
    graph: &GraphName,
    node: &str,
) -> Result<Option<Restriction>> {
    let quads = subject_quads(store, &Term::BlankNode(node.to_owned()), graph)?;
    let Some(property) = quads
        .iter()
        .find(|quad| quad.predicate == owl_on_property())
        .and_then(|quad| quad.object.as_iri().cloned())
    else {
        return Ok(None);
    };
    let on_class = quads
        .iter()
        .find(|quad| quad.predicate == owl_on_class())
        .and_then(|quad| quad.object.as_iri().cloned());
    let cardinality = |predicate: &Id| -> Option<u32> {
        quads
            .iter()
            .find(|quad| &quad.predicate == predicate)
            .and_then(|quad| match &quad.object {
                Term::Literal(literal) => literal.value.parse().ok(),
                _ => None,
            })
    };

    if let Some(class) = quads
        .iter()
        .find(|quad| quad.predicate == owl_some_values_from())
        .and_then(|quad| quad.object.as_iri().cloned())
    {
        return Ok(Some(Restriction::SomeValuesFrom { property, class }));
    }
    if let Some(class) = quads
        .iter()
        .find(|quad| quad.predicate == owl_all_values_from())
        .and_then(|quad| quad.object.as_iri().cloned())
    {
        return Ok(Some(Restriction::AllValuesFrom { property, class }));
    }
    if let Some(value) = quads
        .iter()
        .find(|quad| quad.predicate == owl_has_value())
        .map(|quad| quad.object.clone())
    {
        return Ok(Some(Restriction::HasValue { property, value }));
    }
    for (predicate, variant) in [
        (owl_min_cardinality(), 0_u8),
        (owl_min_qualified_cardinality(), 0),
        (owl_max_cardinality(), 1),
        (owl_max_qualified_cardinality(), 1),
        (owl_cardinality(), 2),
        (owl_qualified_cardinality(), 2),
    ] {
        if let Some(cardinality) = cardinality(&predicate) {
            let class = if predicate == owl_min_qualified_cardinality()
                || predicate == owl_max_qualified_cardinality()
                || predicate == owl_qualified_cardinality()
            {
                on_class.clone()
            } else {
                None
            };
            return Ok(Some(match variant {
                0 => Restriction::MinCardinality {
                    property,
                    cardinality,
                    class,
                },
                1 => Restriction::MaxCardinality {
                    property,
                    cardinality,
                    class,
                },
                _ => Restriction::ExactCardinality {
                    property,
                    cardinality,
                    class,
                },
            }));
        }
    }
    Ok(None)
}

fn kind_from_type(id: &Id) -> Option<PropertyKind> {
    if *id == property_type_iri(PropertyKind::Object) {
        Some(PropertyKind::Object)
    } else if *id == property_type_iri(PropertyKind::Datatype) {
        Some(PropertyKind::Datatype)
    } else if *id == property_type_iri(PropertyKind::Annotation) {
        Some(PropertyKind::Annotation)
    } else {
        None
    }
}

/// Loads a property definition, if the IRI is declared as a managed property.
///
/// # Errors
///
/// Propagates storage failures.
pub(crate) fn load_property(
    store: &dyn RdfStore,
    graph: &GraphName,
    id: &Id,
) -> Result<Option<OwlProperty>> {
    let quads = subject_quads(store, &Term::Iri(id.clone()), graph)?;
    let kind = quads
        .iter()
        .filter(|quad| quad.predicate == rdf_type())
        .filter_map(|quad| quad.object.as_iri())
        .find_map(kind_from_type);
    let Some(kind) = kind else {
        return Ok(None);
    };

    let mut property = OwlProperty::new(id.clone(), kind);
    collect_text(&quads, &rdfs_label(), &mut property.labels);
    collect_text(&quads, &rdfs_comment(), &mut property.comments);
    property.sub_property_of = collect_iris(&quads, &rdfs_sub_property_of());
    property.equivalent_properties = collect_iris(&quads, &owl_equivalent_property());
    property.domain = collect_iris(&quads, &rdfs_domain());
    property.range = collect_iris(&quads, &rdfs_range());
    property.inverse_of = quads
        .iter()
        .find(|quad| quad.predicate == owl_inverse_of())
        .and_then(|quad| quad.object.as_iri().cloned());
    property.characteristics = PropertyCharacteristic::ALL
        .into_iter()
        .filter(|characteristic| {
            quads.iter().any(|quad| {
                quad.predicate == rdf_type()
                    && quad.object == Term::Iri(characteristic_iri(*characteristic))
            })
        })
        .collect();
    property.deprecated = quads.iter().any(|quad| {
        quad.predicate == owl_deprecated()
            && matches!(&quad.object, Term::Literal(literal) if literal.value == "true")
    });
    Ok(Some(property))
}

/// Loads an ontology definition, if the IRI is declared `owl:Ontology`.
///
/// # Errors
///
/// Propagates storage failures.
pub(crate) fn load_ontology(
    store: &dyn RdfStore,
    graph: &GraphName,
    id: &Id,
) -> Result<Option<Ontology>> {
    let quads = subject_quads(store, &Term::Iri(id.clone()), graph)?;
    if !is_typed(&quads, &owl_ontology()) {
        return Ok(None);
    }
    let mut ontology = Ontology::new(id.clone());
    collect_text(&quads, &rdfs_label(), &mut ontology.labels);
    collect_text(&quads, &rdfs_comment(), &mut ontology.comments);
    ontology.version_info = quads
        .iter()
        .find(|quad| quad.predicate == owl_version_info())
        .and_then(|quad| match &quad.object {
            Term::Literal(literal) => Some(literal.value.clone()),
            _ => None,
        });
    ontology.version_iri = quads
        .iter()
        .find(|quad| quad.predicate == owl_version_iri())
        .and_then(|quad| quad.object.as_iri().cloned());
    ontology.imports = collect_iris(&quads, &owl_imports());
    Ok(Some(ontology))
}

/// Loads a recorded ontology version.
///
/// # Errors
///
/// Propagates storage failures.
pub(crate) fn load_version(
    store: &dyn RdfStore,
    graph: &GraphName,
    subject: &Id,
) -> Result<Option<OntologyVersion>> {
    let quads = subject_quads(store, &Term::Iri(subject.clone()), graph)?;
    let Some(ontology) = quads
        .iter()
        .find(|quad| quad.predicate == dcterms_is_version_of())
        .and_then(|quad| quad.object.as_iri().cloned())
    else {
        return Ok(None);
    };
    let version_info = quads
        .iter()
        .find(|quad| quad.predicate == onto_term("version"))
        .and_then(|quad| match &quad.object {
            Term::Literal(literal) => Some(literal.value.clone()),
            _ => None,
        });
    let version_iri = quads
        .iter()
        .find(|quad| quad.predicate == owl_version_iri())
        .and_then(|quad| quad.object.as_iri().cloned());
    let recorded_at = quads
        .iter()
        .find(|quad| quad.predicate == onto_term("recordedAt"))
        .and_then(|quad| match &quad.object {
            Term::Literal(literal) => DateTime::parse_from_rfc3339(&literal.value).ok(),
            _ => None,
        })
        .map_or_else(Utc::now, |value| value.with_timezone(&Utc));
    Ok(Some(OntologyVersion {
        ontology,
        version_info,
        version_iri,
        recorded_at,
    }))
}

/// Returns every class IRI declared in `graph`.
///
/// # Errors
///
/// Propagates storage failures.
pub(crate) fn class_ids(store: &dyn RdfStore, graph: &GraphName) -> Result<Vec<Id>> {
    let quads = store.get(
        QuadPattern::new()
            .with_predicate(rdf_type())
            .with_object(Term::Iri(owl_class()))
            .with_graph(graph.clone()),
    )?;
    Ok(subjects(quads))
}

/// Returns every managed property IRI declared in `graph`.
///
/// # Errors
///
/// Propagates storage failures.
pub(crate) fn property_ids(store: &dyn RdfStore, graph: &GraphName) -> Result<Vec<Id>> {
    let mut ids = BTreeSet::new();
    for kind in [
        PropertyKind::Object,
        PropertyKind::Datatype,
        PropertyKind::Annotation,
    ] {
        let quads = store.get(
            QuadPattern::new()
                .with_predicate(rdf_type())
                .with_object(Term::Iri(property_type_iri(kind)))
                .with_graph(graph.clone()),
        )?;
        ids.extend(subjects(quads));
    }
    Ok(ids.into_iter().collect())
}

/// Returns every ontology IRI declared in `graph`.
///
/// # Errors
///
/// Propagates storage failures.
pub(crate) fn ontology_ids(store: &dyn RdfStore, graph: &GraphName) -> Result<Vec<Id>> {
    let quads = store.get(
        QuadPattern::new()
            .with_predicate(rdf_type())
            .with_object(Term::Iri(owl_ontology()))
            .with_graph(graph.clone()),
    )?;
    Ok(subjects(quads))
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

/// Collects a class's own quads plus its anonymous restriction quads.
///
/// # Errors
///
/// Propagates storage failures.
pub(crate) fn class_removal_quads(
    store: &dyn RdfStore,
    graph: &GraphName,
    class: &Id,
) -> Result<Vec<Quad>> {
    let quads = subject_quads(store, &Term::Iri(class.clone()), graph)?;
    let mut removals = quads.clone();
    for quad in &quads {
        if quad.predicate != rdfs_sub_class_of() {
            continue;
        }
        if let Term::BlankNode(node) = &quad.object {
            removals.extend(subject_quads(store, &Term::BlankNode(node.clone()), graph)?);
        }
    }
    Ok(removals)
}

/// Whether an IRI denotes an XML Schema datatype or `rdfs:Literal`.
pub(crate) fn is_datatype(iri: &Id) -> bool {
    iri.as_str().starts_with(XSD) || *iri == rdfs_literal()
}
