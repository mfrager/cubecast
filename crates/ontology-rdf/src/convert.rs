//! Translation between the application data model and OxRDF.

use ontology_core::error::{Error, Result};
use ontology_core::id::Id;
use ontology_core::model::{GraphName, Literal, Quad, Term};
use ontology_core::namespace;
use oxigraph::model::{
    BlankNode, GraphName as OxGraphName, Literal as OxLiteral, NamedNode, NamedOrBlankNode,
    Quad as OxQuad, Term as OxTerm,
};

/// Converts an application IRI into an OxRDF named node.
///
/// # Errors
///
/// Returns [`Error::InvalidIri`] if the IRI is not accepted by OxRDF.
pub fn iri_to_ox(iri: &Id) -> Result<NamedNode> {
    NamedNode::new(iri.to_string()).map_err(|error| Error::InvalidIri {
        value: iri.to_string(),
        detail: error.to_string(),
    })
}

/// Converts an OxRDF named node into an application IRI.
///
/// # Errors
///
/// Returns [`Error::InvalidIri`] if the node is not a valid IRI.
pub fn iri_from_ox(node: &NamedNode) -> Result<Id> {
    Id::new(node.as_str())
}

/// Converts an application term into an OxRDF term.
///
/// # Errors
///
/// Returns an error when a blank node identifier or literal is not valid.
pub fn term_to_ox(term: &Term) -> Result<OxTerm> {
    match term {
        Term::Iri(iri) => Ok(OxTerm::from(iri_to_ox(iri)?)),
        Term::BlankNode(id) => Ok(OxTerm::from(blank_node_to_ox(id)?)),
        Term::Literal(literal) => Ok(OxTerm::from(literal_to_ox(literal)?)),
    }
}

/// Converts an OxRDF term into an application term.
///
/// # Errors
///
/// Returns [`Error::Unsupported`] for RDF-star quoted triples, which the
/// application model does not represent.
pub fn term_from_ox(term: &OxTerm) -> Result<Term> {
    match term {
        OxTerm::NamedNode(node) => Ok(Term::Iri(iri_from_ox(node)?)),
        OxTerm::BlankNode(node) => Ok(Term::BlankNode(node.as_str().to_owned())),
        OxTerm::Literal(literal) => Ok(Term::Literal(literal_from_ox(literal)?)),
        #[allow(unreachable_patterns)]
        _ => Err(Error::Unsupported {
            operation: "RDF-star quoted triples".to_owned(),
        }),
    }
}

/// Converts an application subject into an OxRDF subject.
///
/// # Errors
///
/// Returns [`Error::Validation`] when the term is a literal.
pub fn subject_to_ox(term: &Term) -> Result<NamedOrBlankNode> {
    match term {
        Term::Iri(iri) => Ok(NamedOrBlankNode::from(iri_to_ox(iri)?)),
        Term::BlankNode(id) => Ok(NamedOrBlankNode::from(blank_node_to_ox(id)?)),
        Term::Literal(_) => Err(Error::validation(format!(
            "literal subject is not allowed: {term}"
        ))),
    }
}

/// Converts an application graph name into an OxRDF graph name.
///
/// # Errors
///
/// Returns an error when the graph IRI or blank node identifier is invalid.
pub fn graph_to_ox(graph: &GraphName) -> Result<OxGraphName> {
    match graph {
        GraphName::Default => Ok(OxGraphName::DefaultGraph),
        GraphName::Iri(iri) => Ok(OxGraphName::from(iri_to_ox(iri)?)),
        GraphName::BlankNode(id) => Ok(OxGraphName::from(blank_node_to_ox(id)?)),
    }
}

/// Converts an OxRDF graph name into an application graph name.
///
/// # Errors
///
/// Returns [`Error::InvalidIri`] if the graph IRI is invalid.
pub fn graph_from_ox(graph: &OxGraphName) -> Result<GraphName> {
    match graph {
        OxGraphName::DefaultGraph => Ok(GraphName::Default),
        OxGraphName::NamedNode(node) => Ok(GraphName::Iri(iri_from_ox(node)?)),
        OxGraphName::BlankNode(node) => Ok(GraphName::BlankNode(node.as_str().to_owned())),
    }
}

/// Converts an OxRDF graph label (as yielded by graph enumeration) into an
/// application graph name.
///
/// # Errors
///
/// Returns [`Error::InvalidIri`] when the label is an invalid IRI.
pub fn graph_label_from_ox(graph: &NamedOrBlankNode) -> Result<GraphName> {
    match graph {
        NamedOrBlankNode::NamedNode(node) => Ok(GraphName::Iri(iri_from_ox(node)?)),
        NamedOrBlankNode::BlankNode(node) => Ok(GraphName::BlankNode(node.as_str().to_owned())),
    }
}

/// Converts an application quad into an OxRDF quad.
///
/// # Errors
///
/// Propagates term conversion and structural validation errors.
pub fn quad_to_ox(quad: &Quad) -> Result<OxQuad> {
    quad.validate()?;
    Ok(OxQuad::new(
        subject_to_ox(&quad.subject)?,
        iri_to_ox(&quad.predicate)?,
        term_to_ox(&quad.object)?,
        graph_to_ox(&quad.graph)?,
    ))
}

/// Converts an OxRDF quad into an application quad.
///
/// # Errors
///
/// Propagates term conversion errors.
pub fn quad_from_ox(quad: &OxQuad) -> Result<Quad> {
    Ok(Quad {
        subject: term_from_ox(&OxTerm::from(quad.subject.clone()))?,
        predicate: iri_from_ox(&quad.predicate)?,
        object: term_from_ox(&OxTerm::from(quad.object.clone()))?,
        graph: graph_from_ox(&quad.graph_name)?,
    })
}

/// Converts an application literal into an OxRDF literal.
///
/// # Errors
///
/// Returns [`Error::Validation`] when both a language tag and a datatype are
/// set, and propagates datatype conversion errors.
pub fn literal_to_ox(literal: &Literal) -> Result<OxLiteral> {
    if literal.language.is_some() && literal.datatype.is_some() {
        return Err(Error::validation(
            "a literal cannot have both a language tag and a datatype",
        ));
    }
    if let Some(language) = &literal.language {
        return OxLiteral::new_language_tagged_literal(literal.value.clone(), language.clone())
            .map_err(|error| Error::validation(error.to_string()));
    }
    match &literal.datatype {
        Some(datatype) => Ok(OxLiteral::new_typed_literal(
            literal.value.clone(),
            iri_to_ox(datatype)?,
        )),
        None => Ok(OxLiteral::new_simple_literal(literal.value.clone())),
    }
}

/// Converts an OxRDF literal into an application literal.
///
/// `xsd:string` is normalized to a simple literal so simple strings round-trip
/// without gaining an explicit datatype.
///
/// # Errors
///
/// Returns [`Error::InvalidIri`] when the datatype IRI is invalid.
pub fn literal_from_ox(literal: &OxLiteral) -> Result<Literal> {
    let value = literal.value().to_owned();
    if let Some(language) = literal.language() {
        return Ok(Literal {
            value,
            language: Some(language.to_owned()),
            datatype: None,
        });
    }
    let datatype = literal.datatype();
    if datatype.as_str() == namespace::XSD_STRING {
        Ok(Literal::simple(value))
    } else {
        Ok(Literal {
            value,
            language: None,
            datatype: Some(iri_from_ox(datatype)?),
        })
    }
}

/// Converts an application quad pattern into OxRDF filter components.
///
/// # Errors
///
/// Returns [`Error::Validation`] when the subject filter is a literal.
pub fn pattern_to_ox(
    pattern: &ontology_core::model::QuadPattern,
) -> Result<(
    Option<NamedOrBlankNode>,
    Option<NamedNode>,
    Option<OxTerm>,
    Option<OxGraphName>,
)> {
    let subject = pattern.subject.as_ref().map(subject_to_ox).transpose()?;
    let predicate = pattern.predicate.as_ref().map(iri_to_ox).transpose()?;
    let object = pattern.object.as_ref().map(term_to_ox).transpose()?;
    let graph = pattern.graph.as_ref().map(graph_to_ox).transpose()?;
    Ok((subject, predicate, object, graph))
}

fn blank_node_to_ox(id: &str) -> Result<BlankNode> {
    BlankNode::new(id.to_owned()).map_err(|error| Error::validation(error.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ex(local: &str) -> Id {
        Id::new(format!("https://example.org/{local}")).unwrap()
    }

    #[test]
    fn simple_and_typed_literals_round_trip() {
        let literal = Literal::simple("hello");
        let ox = literal_to_ox(&literal).unwrap();
        assert_eq!(literal_from_ox(&ox).unwrap(), literal);

        let literal = Literal::integer(42);
        let ox = literal_to_ox(&literal).unwrap();
        assert_eq!(literal_from_ox(&ox).unwrap(), literal);

        let literal = Literal::language_tagged("hello", "en").unwrap();
        let ox = literal_to_ox(&literal).unwrap();
        assert_eq!(literal_from_ox(&ox).unwrap(), literal);
    }

    #[test]
    fn quads_round_trip() {
        let quad = Quad::in_graph(
            Term::Iri(ex("s")),
            ex("p"),
            Term::string("o"),
            GraphName::Iri(ex("g")),
        );
        let ox = quad_to_ox(&quad).unwrap();
        assert_eq!(quad_from_ox(&ox).unwrap(), quad);
    }

    #[test]
    fn literal_subjects_are_rejected() {
        let quad = Quad::new(Term::string("subject"), ex("p"), Term::string("o"));
        assert!(quad_to_ox(&quad).is_err());
    }
}
