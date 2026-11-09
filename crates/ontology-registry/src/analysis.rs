//! Deterministic term extraction, statistics, and structural validation.

use std::collections::{BTreeMap, BTreeSet};

use chrono::Utc;
use ontology_core::id::Id;
use ontology_core::model::{Quad, Term};
use ontology_core::registry::{
    ReleaseCandidate, ReleaseStatistics, TermSummary, ValidationIssue, ValidationReport,
};

use crate::identity::{
    content_hash, imports, is_instance_of, rdf_type, term_key, OWL_IMPORTS, OWL_ONTOLOGY,
    OWL_VERSION_INFO, RDFS_LABEL, SKOS_CONCEPT_SCHEME,
};

const RDFS_COMMENT: &str = "http://www.w3.org/2000/01/rdf-schema#comment";
const SKOS_PREF_LABEL: &str = "http://www.w3.org/2004/02/skos/core#prefLabel";
const SKOS_DEFINITION: &str = "http://www.w3.org/2004/02/skos/core#definition";
const DCTERMS_DESCRIPTION: &str = "http://purl.org/dc/terms/description";
const OWL_DEPRECATED: &str = "http://www.w3.org/2002/07/owl#deprecated";

fn literal(quad: &Quad) -> Option<&str> {
    match &quad.object {
        Term::Literal(value) => Some(value.value.as_str()),
        _ => None,
    }
}

fn first_literal(parsed: &[Quad], subject: &Id, predicates: &[&str]) -> Option<String> {
    for predicate in predicates {
        if let Some(value) = parsed
            .iter()
            .filter(|quad| quad.subject.as_iri() == Some(subject))
            .filter(|quad| quad.predicate.as_str() == *predicate)
            .find_map(literal)
        {
            let value = value.trim();
            if !value.is_empty() {
                return Some(value.to_owned());
            }
        }
    }
    None
}

fn kind_for(parsed: &[Quad], subject: &Id) -> Option<&'static str> {
    for quad in parsed {
        if quad.subject.as_iri() != Some(subject) || quad.predicate.as_str() != rdf_type().as_str()
        {
            continue;
        }
        let Some(class) = quad.object.as_iri() else {
            continue;
        };
        let kind = match class.as_str() {
            "http://www.w3.org/2002/07/owl#Class" => Some("class"),
            "http://www.w3.org/2002/07/owl#ObjectProperty" => Some("object_property"),
            "http://www.w3.org/2002/07/owl#DatatypeProperty" => Some("datatype_property"),
            "http://www.w3.org/2002/07/owl#AnnotationProperty" => Some("annotation_property"),
            "http://www.w3.org/2002/07/owl#NamedIndividual" => Some("individual"),
            "http://www.w3.org/2004/02/skos/core#Concept" => Some("individual"),
            _ => None,
        };
        if kind.is_some() {
            return kind;
        }
    }
    None
}

fn deprecated(parsed: &[Quad], subject: &Id) -> bool {
    parsed.iter().any(|quad| {
        quad.subject.as_iri() == Some(subject)
            && quad.predicate.as_str() == OWL_DEPRECATED
            && literal(quad).is_some_and(|value| matches!(value, "true" | "1"))
    })
}

/// Extracts terms, statistics, and imports from parsed quads.
#[derive(Debug, Clone)]
pub struct Analysis {
    /// Statistics.
    pub statistics: ReleaseStatistics,
    /// Deterministic term records.
    pub terms: Vec<TermSummary>,
    /// Declared import IRIs.
    pub imports: Vec<Id>,
    /// Content hash of the parsed document.
    pub content_hash: String,
}

/// Analyzes a parsed document without writing it.
#[must_use]
pub fn analyze(parsed: &[Quad]) -> Analysis {
    let classes = parsed
        .iter()
        .filter(|quad| is_instance_of(quad, "http://www.w3.org/2002/07/owl#Class"))
        .count();
    let object_properties = parsed
        .iter()
        .filter(|quad| is_instance_of(quad, "http://www.w3.org/2002/07/owl#ObjectProperty"))
        .count();
    let datatype_properties = parsed
        .iter()
        .filter(|quad| is_instance_of(quad, "http://www.w3.org/2002/07/owl#DatatypeProperty"))
        .count();
    let annotation_properties = parsed
        .iter()
        .filter(|quad| is_instance_of(quad, "http://www.w3.org/2002/07/owl#AnnotationProperty"))
        .count();
    let individuals = parsed
        .iter()
        .filter(|quad| is_instance_of(quad, "http://www.w3.org/2002/07/owl#NamedIndividual"))
        .count();
    let subjects: BTreeSet<Id> = parsed
        .iter()
        .filter_map(|quad| quad.subject.as_iri().cloned())
        .collect();
    let mut terms = Vec::new();
    for subject in subjects {
        let Some(kind) = kind_for(parsed, &subject) else {
            continue;
        };
        terms.push(TermSummary {
            key: term_key(&subject),
            iri: subject.clone(),
            kind: kind.to_owned(),
            label: first_literal(parsed, &subject, &[SKOS_PREF_LABEL, RDFS_LABEL]),
            definition: first_literal(
                parsed,
                &subject,
                &[SKOS_DEFINITION, RDFS_COMMENT, DCTERMS_DESCRIPTION],
            ),
            namespace: Some(subject.namespace().to_owned()).filter(|value| !value.is_empty()),
            deprecated: deprecated(parsed, &subject),
        });
    }
    terms.sort_by(|left, right| left.iri.cmp(&right.iri));
    Analysis {
        statistics: ReleaseStatistics {
            quads: parsed.len(),
            classes,
            object_properties,
            datatype_properties,
            annotation_properties,
            individuals,
            terms: terms.len(),
        },
        terms,
        imports: imports(parsed),
        content_hash: content_hash(parsed),
    }
}

/// Runs deterministic structural validation.
///
/// No `sh:conforms true` is emitted for a profile: the workspace has no SHACL
/// evaluator, so a profile request produces a warning and a non-conforming
/// report rather than a false positive.
#[must_use]
pub fn validate(
    candidate: &ReleaseCandidate,
    parsed: &[Quad],
    require_resolved_imports: bool,
    profile_requested: bool,
) -> ValidationReport {
    let mut issues = Vec::new();
    if parsed.is_empty() {
        issues.push(issue(
            "error",
            Some(candidate.id.clone()),
            None,
            "release contains no RDF statements",
        ));
    }
    if candidate.version.trim().is_empty() {
        issues.push(issue(
            "error",
            Some(candidate.id.clone()),
            Some("dcat:version"),
            "release version must not be empty",
        ));
    }
    for import in &candidate.imports {
        if import.iri == candidate.asset {
            issues.push(issue(
                "error",
                Some(candidate.id.clone()),
                Some("owl:imports"),
                "an ontology must not import itself",
            ));
        }
        if require_resolved_imports && !import.resolved {
            issues.push(issue(
                "error",
                Some(import.iri.clone()),
                Some("owl:imports"),
                format!(
                    "import {} is unresolved and the publication gate is enabled",
                    import.iri
                ),
            ));
        } else if !import.resolved {
            issues.push(issue(
                "warning",
                Some(import.iri.clone()),
                Some("owl:imports"),
                "import is not resolved against a registered release",
            ));
        }
    }
    let analysis = analyze(parsed);
    let mut labels: BTreeMap<(&str, String), Vec<Id>> = BTreeMap::new();
    for term in &analysis.terms {
        if let Some(label) = &term.label {
            labels
                .entry((&term.kind, label.to_lowercase()))
                .or_default()
                .push(term.iri.clone());
        }
    }
    for ((kind, label), iris) in labels {
        if iris.len() > 1 {
            issues.push(issue(
                "warning",
                None,
                Some("skos:prefLabel"),
                format!("duplicate {kind} label {label:?} on {iris:?}"),
            ));
        }
    }
    if profile_requested {
        issues.push(issue(
            "error",
            Some(candidate.id.clone()),
            Some("prof:Profile"),
            "profile validation requires an external SHACL evaluator; structural conformance is reported separately",
        ));
    }
    let conforms = !issues.iter().any(|issue| issue.severity == "error");
    ValidationReport {
        id: Id::new(format!(
            "urn:onto:registry:validation-report:{}",
            ulid::Ulid::new()
        ))
        .expect("generated validation report IRI"),
        target: candidate.id.clone(),
        conforms,
        issues,
        external_shacl: false,
        generated_at: Utc::now(),
    }
}

fn issue(
    severity: &str,
    focus_node: Option<Id>,
    path: Option<&str>,
    message: impl Into<String>,
) -> ValidationIssue {
    ValidationIssue {
        severity: severity.to_owned(),
        focus_node,
        path: path.map(ToOwned::to_owned),
        message: message.into(),
    }
}

/// Returns the RDF type IRI used by tests and callers.
#[must_use]
pub fn ontology_type() -> Id {
    Id::new(OWL_ONTOLOGY).expect("owl:Ontology is valid")
}

/// Returns the version-info predicate IRI.
#[must_use]
pub fn version_info() -> Id {
    Id::new(OWL_VERSION_INFO).expect("owl:versionInfo is valid")
}

/// Returns the import predicate IRI.
#[must_use]
pub fn imports_predicate() -> Id {
    Id::new(OWL_IMPORTS).expect("owl:imports is valid")
}

/// Returns the SKOS scheme type IRI.
#[must_use]
pub fn scheme_type() -> Id {
    Id::new(SKOS_CONCEPT_SCHEME).expect("skos:ConceptScheme is valid")
}

#[cfg(test)]
mod tests {
    use super::*;
    use ontology_core::model::Quad;

    fn ex(value: &str) -> Id {
        Id::new(format!("https://example.org/{value}")).unwrap()
    }

    #[test]
    fn extracts_terms_and_counts() {
        let parsed = vec![
            Quad::new(
                Term::Iri(ex("Thing")),
                rdf_type(),
                Term::Iri(Id::new("http://www.w3.org/2002/07/owl#Class").unwrap()),
            ),
            Quad::new(
                Term::Iri(ex("Thing")),
                Id::new(RDFS_LABEL).unwrap(),
                Term::string("Thing"),
            ),
        ];
        let analysis = analyze(&parsed);
        assert_eq!(analysis.statistics.classes, 1);
        assert_eq!(analysis.statistics.terms, 1);
        assert_eq!(analysis.terms[0].label.as_deref(), Some("Thing"));
    }
}
