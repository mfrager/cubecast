//! Structural validation of an OWL schema.

use std::collections::{BTreeMap, BTreeSet};

use ontology_core::id::Id;
use ontology_core::owl::{
    GraphScope, OwlClass, OwlProperty, PropertyKind, Severity, ValidationIssue, ValidationReport,
};

use crate::encoding;

fn issue(
    issues: &mut Vec<ValidationIssue>,
    code: &str,
    severity: Severity,
    subject: &Id,
    message: impl Into<String>,
) {
    issues.push(ValidationIssue {
        code: code.to_owned(),
        severity,
        subject: subject.clone(),
        message: message.into(),
    });
}

/// Validates classes and properties in one graph.
pub(crate) fn validate(
    scope: &GraphScope,
    classes: &[OwlClass],
    properties: &[OwlProperty],
) -> ValidationReport {
    let class_map: BTreeMap<Id, &OwlClass> = classes
        .iter()
        .map(|class| (class.id.clone(), class))
        .collect();
    let class_ids: BTreeSet<Id> = class_map.keys().cloned().collect();
    let property_ids: BTreeSet<Id> = properties
        .iter()
        .map(|property| property.id.clone())
        .collect();
    let mut issues = Vec::new();

    for class in classes {
        for superclass in &class.sub_class_of {
            if !class_ids.contains(superclass) {
                issue(
                    &mut issues,
                    "unknown-superclass",
                    Severity::Warning,
                    &class.id,
                    format!("superclass {superclass} is not declared as owl:Class"),
                );
            }
        }
        for equivalent in &class.equivalent_classes {
            if !class_ids.contains(equivalent) {
                issue(
                    &mut issues,
                    "unknown-equivalent-class",
                    Severity::Warning,
                    &class.id,
                    format!("equivalent class {equivalent} is not declared"),
                );
            }
        }
        for disjoint in &class.disjoint_with {
            if disjoint == &class.id {
                issue(
                    &mut issues,
                    "disjoint-with-self",
                    Severity::Error,
                    &class.id,
                    "a class cannot be disjoint with itself",
                );
            } else if !class_ids.contains(disjoint) {
                issue(
                    &mut issues,
                    "unknown-disjoint-class",
                    Severity::Warning,
                    &class.id,
                    format!("disjoint class {disjoint} is not declared"),
                );
            }
        }
        for restriction in &class.restrictions {
            if !property_ids.contains(restriction.property()) {
                issue(
                    &mut issues,
                    "unknown-restriction-property",
                    Severity::Warning,
                    &class.id,
                    format!(
                        "restriction property {} is not declared",
                        restriction.property()
                    ),
                );
            }
            if let Some(filler) = restriction.class() {
                if !class_ids.contains(filler) {
                    issue(
                        &mut issues,
                        "unknown-restriction-class",
                        Severity::Warning,
                        &class.id,
                        format!("restriction filler class {filler} is not declared"),
                    );
                }
            }
        }
        if has_cycle(&class.id, &class_map) {
            issue(
                &mut issues,
                "subclass-cycle",
                Severity::Error,
                &class.id,
                "class is its own ancestor through rdfs:subClassOf",
            );
        }
    }

    for property in properties {
        if property.kind == PropertyKind::Object {
            for domain in &property.domain {
                if !class_ids.contains(domain) {
                    issue(
                        &mut issues,
                        "unknown-domain-class",
                        Severity::Warning,
                        &property.id,
                        format!("domain {domain} is not declared as owl:Class"),
                    );
                }
            }
            for range in &property.range {
                if !class_ids.contains(range) {
                    issue(
                        &mut issues,
                        "unknown-range-class",
                        Severity::Warning,
                        &property.id,
                        format!("range {range} is not declared as owl:Class"),
                    );
                }
            }
            if let Some(inverse) = &property.inverse_of {
                if !property_ids.contains(inverse) {
                    issue(
                        &mut issues,
                        "unknown-inverse-property",
                        Severity::Warning,
                        &property.id,
                        format!("inverse property {inverse} is not declared"),
                    );
                }
            }
        } else if property.kind == PropertyKind::Datatype {
            for range in &property.range {
                if !encoding::is_datatype(range) {
                    issue(
                        &mut issues,
                        "non-datatype-range",
                        Severity::Warning,
                        &property.id,
                        format!("range {range} is not an XML Schema datatype"),
                    );
                }
            }
        }
    }

    ValidationReport {
        scope: scope.clone(),
        checked_classes: classes.len(),
        checked_properties: properties.len(),
        issues,
    }
}

fn has_cycle(class: &Id, classes: &BTreeMap<Id, &OwlClass>) -> bool {
    let mut stack: Vec<Id> = classes
        .get(class)
        .map(|definition| definition.sub_class_of.clone())
        .unwrap_or_default();
    let mut visited = BTreeSet::new();
    while let Some(current) = stack.pop() {
        if &current == class {
            return true;
        }
        if !visited.insert(current.clone()) {
            continue;
        }
        if let Some(definition) = classes.get(&current) {
            stack.extend(definition.sub_class_of.iter().cloned());
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;
    use ontology_core::id::Id;
    use ontology_core::model::GraphName;
    use ontology_core::owl::Restriction;

    fn id(value: &str) -> Id {
        Id::new(format!("https://example.org/{value}")).expect("test IRI")
    }

    fn xsd(value: &str) -> Id {
        Id::new(format!("http://www.w3.org/2001/XMLSchema#{value}")).expect("XSD IRI")
    }

    #[test]
    fn accepts_a_consistent_schema() {
        let classes = vec![
            OwlClass::new(id("Thing")),
            OwlClass::new(id("Animal")).with_superclass(id("Thing")),
        ];
        let properties = vec![
            OwlProperty::new(id("eats"), PropertyKind::Object)
                .with_domain(id("Animal"))
                .with_range(id("Thing")),
            OwlProperty::new(id("name"), PropertyKind::Datatype).with_range(xsd("string")),
        ];
        let report = validate(&GraphScope::Schema, &classes, &properties);
        assert_eq!(report.checked_classes, 2);
        assert_eq!(report.checked_properties, 2);
        assert!(report.issues.is_empty(), "{:?}", report.issues);
        assert!(report.is_valid());
    }

    #[test]
    fn detects_cycles_and_invalid_references() {
        let classes = vec![
            OwlClass::new(id("A"))
                .with_superclass(id("B"))
                .with_disjoint_class(id("A")),
            OwlClass::new(id("B")).with_superclass(id("A")),
        ];
        let properties = vec![
            OwlProperty::new(id("p"), PropertyKind::Object)
                .with_domain(id("Missing"))
                .with_range(id("Missing"))
                .with_inverse(id("missingInverse")),
            OwlProperty::new(id("d"), PropertyKind::Datatype).with_range(id("NotADatatype")),
        ];
        let report = validate(&GraphScope::Schema, &classes, &properties);
        let codes: Vec<&str> = report
            .issues
            .iter()
            .map(|issue| issue.code.as_str())
            .collect();
        assert!(codes.contains(&"subclass-cycle"));
        assert!(codes.contains(&"disjoint-with-self"));
        assert!(codes.contains(&"unknown-domain-class"));
        assert!(codes.contains(&"unknown-range-class"));
        assert!(codes.contains(&"unknown-inverse-property"));
        assert!(codes.contains(&"non-datatype-range"));
        assert!(!report.is_valid());
        assert!(report.errors().count() >= 2);
    }

    #[test]
    fn detects_unknown_restriction_targets() {
        let classes =
            vec![
                OwlClass::new(id("Contract")).with_restriction(Restriction::SomeValuesFrom {
                    property: id("party"),
                    class: id("Party"),
                }),
            ];
        let report = validate(&GraphScope::Schema, &classes, &[]);
        assert!(report
            .issues
            .iter()
            .any(|issue| issue.code == "unknown-restriction-property"));
        assert!(report
            .issues
            .iter()
            .any(|issue| issue.code == "unknown-restriction-class"));
        assert!(report.is_valid(), "restriction findings are warnings");
    }

    #[test]
    fn preserves_the_requested_scope() {
        let scope = GraphScope::Graph(GraphName::Default);
        let report = validate(&scope, &[], &[]);
        assert_eq!(report.scope, scope);
        assert_eq!(report.checked_classes, 0);
        assert_eq!(report.checked_properties, 0);
    }
}
