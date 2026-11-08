//! RDF syntax support: the core format registry plus parsing into the
//! application data model.

use ontology_core::error::{Error, Result};
use ontology_core::model::Quad;
use oxigraph::io::{JsonLdProfileSet, RdfFormat as OxRdfFormat, RdfParser};

use crate::convert::quad_from_ox;

pub use ontology_core::format::RdfFormat;

/// Converts a core format into the engine's representation.
pub(crate) fn to_oxigraph(format: RdfFormat) -> OxRdfFormat {
    match format {
        RdfFormat::Turtle => OxRdfFormat::Turtle,
        RdfFormat::NTriples => OxRdfFormat::NTriples,
        RdfFormat::NQuads => OxRdfFormat::NQuads,
        RdfFormat::TriG => OxRdfFormat::TriG,
        RdfFormat::RdfXml => OxRdfFormat::RdfXml,
        RdfFormat::JsonLd => OxRdfFormat::JsonLd {
            profile: JsonLdProfileSet::empty(),
        },
    }
}

/// Parses RDF data into application quads without touching a store.
///
/// Import orchestration uses this so format parsing stays in the adapter crate
/// while the registry only needs the `RdfStore` trait.
///
/// # Errors
///
/// Returns [`Error::Parse`] for invalid input.
pub fn parse_quads(format: RdfFormat, data: &[u8], base: Option<&str>) -> Result<Vec<Quad>> {
    let mut parser = RdfParser::from_format(to_oxigraph(format));
    if let Some(base) = base {
        parser = parser.with_base_iri(base).map_err(Error::parse)?;
    }
    parser
        .for_slice(data)
        .map(|item| {
            let quad = item.map_err(Error::parse)?;
            quad_from_ox(&quad)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use ontology_core::error::Error;
    use ontology_core::model::GraphName;

    #[test]
    fn parses_turtle_into_application_quads() {
        let quads = parse_quads(
            RdfFormat::Turtle,
            br#"@prefix ex: <https://example.org/> .
                ex:item ex:name "Item" ."#,
            None,
        )
        .expect("parse turtle");
        assert_eq!(quads.len(), 1);
        assert_eq!(
            quads[0].subject,
            ontology_core::model::Term::Iri(
                ontology_core::id::Id::new("https://example.org/item").unwrap()
            )
        );
    }

    #[test]
    fn preserves_named_graphs_in_nquads() {
        let quads = parse_quads(
            RdfFormat::NQuads,
            b"<https://example.org/s> <https://example.org/p> <https://example.org/o> <https://example.org/g> .",
            None,
        )
        .expect("parse nquads");
        assert_eq!(quads.len(), 1);
        assert_eq!(
            quads[0].graph,
            GraphName::Iri(ontology_core::id::Id::new("https://example.org/g").unwrap())
        );
    }

    #[test]
    fn resolves_relative_iris_against_the_base() {
        let quads = parse_quads(
            RdfFormat::Turtle,
            b"<item> <https://example.org/p> <https://example.org/o> .",
            Some("https://example.org/base/"),
        )
        .expect("parse relative");
        assert_eq!(
            quads[0].subject,
            ontology_core::model::Term::Iri(
                ontology_core::id::Id::new("https://example.org/base/item").unwrap()
            )
        );
    }

    #[test]
    fn reports_malformed_documents_as_parse_errors() {
        let error = parse_quads(RdfFormat::Turtle, b"<urn:broken", None)
            .expect_err("malformed turtle must fail");
        assert!(matches!(error, Error::Parse { .. }));
    }
}
