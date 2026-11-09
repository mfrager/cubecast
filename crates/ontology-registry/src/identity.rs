//! Registry identity rules.

use std::collections::BTreeSet;

use ontology_core::error::{Error, Result};
use ontology_core::id::Id;
use ontology_core::model::{Quad, Term};
use sha2::{Digest, Sha256};

pub(crate) const RDF_TYPE: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#type";
pub(crate) const OWL_ONTOLOGY: &str = "http://www.w3.org/2002/07/owl#Ontology";
pub(crate) const OWL_VERSION_INFO: &str = "http://www.w3.org/2002/07/owl#versionInfo";
pub(crate) const OWL_VERSION_IRI: &str = "http://www.w3.org/2002/07/owl#versionIRI";
pub(crate) const OWL_IMPORTS: &str = "http://www.w3.org/2002/07/owl#imports";
pub(crate) const SKOS_CONCEPT_SCHEME: &str = "http://www.w3.org/2004/02/skos/core#ConceptScheme";
pub(crate) const RDFS_LABEL: &str = "http://www.w3.org/2000/01/rdf-schema#label";
pub(crate) const DCTERMS_TITLE: &str = "http://purl.org/dc/terms/title";

fn iri(value: &str) -> Id {
    Id::new(value).expect("well-known vocabulary IRIs are valid")
}

/// Returns the first 16 lowercase hex characters of SHA-256 over `value`.
pub(crate) fn hash_key(value: &str) -> String {
    let digest = Sha256::digest(value.as_bytes());
    let mut hex = String::with_capacity(16);
    for byte in &digest[..8] {
        use std::fmt::Write as _;
        let _ = write!(hex, "{byte:02x}");
    }
    hex
}

/// Full SHA-256 hex digest of bytes.
pub fn sha256_hex(bytes: &[u8]) -> String {
    let digest = Sha256::digest(bytes);
    let mut hex = String::with_capacity(digest.len() * 2);
    for byte in digest {
        use std::fmt::Write as _;
        let _ = write!(hex, "{byte:02x}");
    }
    hex
}

/// Stable release key for a family and version.
#[must_use]
pub fn release_key(family: &Id, version: &str) -> String {
    hash_key(&format!("{}|{version}", family.as_str()))
}

/// Stable asset key for a family IRI.
#[must_use]
pub fn asset_key(family: &Id) -> String {
    hash_key(family.as_str())
}

/// Deterministic term key.
#[must_use]
pub fn term_key(term: &Id) -> String {
    hash_key(term.as_str())
}

fn literal(quad: &Quad) -> Option<&str> {
    match &quad.object {
        Term::Literal(value) => Some(value.value.as_str()),
        _ => None,
    }
}

/// Discovers the ontology family IRI.
///
/// `owl:Ontology` is preferred; a SKOS concept scheme is accepted when it is
/// the only identity in the document. Multiple candidates are rejected rather
/// than silently choosing one.
///
/// # Errors
///
/// Returns [`Error::Validation`] when no identity or several identities exist.
pub fn discover_family(parsed: &[Quad]) -> Result<Id> {
    let mut ontologies = BTreeSet::new();
    let mut schemes = BTreeSet::new();
    for quad in parsed {
        if quad.predicate.as_str() != RDF_TYPE {
            continue;
        }
        let Some(class) = quad.object.as_iri() else {
            continue;
        };
        let Some(subject) = quad.subject.as_iri() else {
            continue;
        };
        if class.as_str() == OWL_ONTOLOGY {
            ontologies.insert(subject.clone());
        } else if class.as_str() == SKOS_CONCEPT_SCHEME {
            schemes.insert(subject.clone());
        }
    }
    if ontologies.len() > 1 {
        return Err(Error::validation(format!(
            "document declares multiple owl:Ontology subjects: {ontologies:?}"
        )));
    }
    if let Some(id) = ontologies.into_iter().next() {
        return Ok(id);
    }
    if schemes.len() > 1 {
        return Err(Error::validation(format!(
            "document declares multiple skos:ConceptScheme subjects: {schemes:?}"
        )));
    }
    schemes.into_iter().next().ok_or_else(|| {
        Error::validation("document declares no owl:Ontology or skos:ConceptScheme identity")
    })
}

/// Resolves the source family, using an explicit override when supplied.
///
/// # Errors
///
/// Returns [`Error::Validation`] when neither source nor override exists.
pub fn family(parsed: &[Quad], explicit: Option<&Id>) -> Result<Id> {
    match explicit {
        Some(id) => Ok(id.clone()),
        None => discover_family(parsed),
    }
}

/// Resolves the release version.
///
/// # Errors
///
/// Returns [`Error::Validation`] when no version is present.
pub fn version(parsed: &[Quad], family: &Id, explicit: Option<&str>) -> Result<String> {
    if let Some(version) = explicit.map(str::trim).filter(|value| !value.is_empty()) {
        return Ok(version.to_owned());
    }
    let version = parsed
        .iter()
        .filter(|quad| quad.subject.as_iri() == Some(family))
        .filter(|quad| quad.predicate.as_str() == OWL_VERSION_INFO)
        .find_map(literal)
        .map(str::trim)
        .filter(|value| !value.is_empty());
    version.map(ToOwned::to_owned).ok_or_else(|| {
        Error::validation("release version is missing; provide `version` or owl:versionInfo")
    })
}

/// Resolves the declared version IRI, or generates one from the release key.
///
/// # Errors
///
/// Returns [`Error::Validation`] for a malformed declared IRI.
pub fn version_iri(parsed: &[Quad], family: &Id, key: &str) -> Result<Id> {
    if let Some(id) = parsed
        .iter()
        .filter(|quad| quad.subject.as_iri() == Some(family))
        .filter(|quad| quad.predicate.as_str() == OWL_VERSION_IRI)
        .find_map(|quad| quad.object.as_iri().cloned())
    {
        return Ok(id);
    }
    Id::new(format!("urn:onto:registry:release:{key}"))
}

/// Returns a source title, falling back to the family local name.
#[must_use]
pub fn title(parsed: &[Quad], family: &Id) -> String {
    let candidates = [DCTERMS_TITLE, RDFS_LABEL];
    for predicate in candidates {
        if let Some(value) = parsed
            .iter()
            .filter(|quad| quad.subject.as_iri() == Some(family))
            .filter(|quad| quad.predicate.as_str() == predicate)
            .find_map(literal)
        {
            let value = value.trim();
            if !value.is_empty() {
                return value.to_owned();
            }
        }
    }
    family.local_name().to_owned()
}

/// Returns declared imports.
#[must_use]
pub fn imports(parsed: &[Quad]) -> Vec<Id> {
    let mut values: Vec<Id> = parsed
        .iter()
        .filter(|quad| quad.predicate.as_str() == OWL_IMPORTS)
        .filter_map(|quad| quad.object.as_iri().cloned())
        .collect();
    values.sort();
    values.dedup();
    values
}

/// Sorted canonical-ish N-Quads content hash.
///
/// Blank-node labels are not stable across parsers; this hash is recorded as
/// informational and never used as the release identity.
#[must_use]
pub fn content_hash(parsed: &[Quad]) -> String {
    let mut lines: Vec<String> = parsed
        .iter()
        .map(|quad| format!("{} {} {}", quad.subject, quad.predicate, quad.object))
        .collect();
    lines.sort();
    lines.dedup();
    sha256_hex(lines.join("\n").as_bytes())
}

/// Counts declared classes and properties.
#[must_use]
pub fn is_instance_of(quad: &Quad, class: &str) -> bool {
    quad.predicate.as_str() == RDF_TYPE
        && quad
            .object
            .as_iri()
            .is_some_and(|value| value.as_str() == class)
}

/// Returns the class IRI for a declared OWL/SKOS class.
pub(crate) fn rdf_type() -> Id {
    iri(RDF_TYPE)
}
