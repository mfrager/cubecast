//! OWL schema domain types and the `OntologyManager` capability trait.
//!
//! These types are engine-independent, like the RDF model: the implementation
//! lives in `ontology-owl` and persists the schema as RDF in `system:schema`.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::error::Result;
use crate::id::Id;
use crate::model::{GraphName, Term};

/// The OWL property kinds the schema engine manages.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum PropertyKind {
    /// A property whose values are individuals.
    Object,
    /// A property whose values are literals.
    Datatype,
    /// A property used for metadata only.
    Annotation,
}

impl PropertyKind {
    /// The OWL class local name (`ObjectProperty`, …).
    #[must_use]
    pub const fn local_name(self) -> &'static str {
        match self {
            Self::Object => "ObjectProperty",
            Self::Datatype => "DatatypeProperty",
            Self::Annotation => "AnnotationProperty",
        }
    }
}

/// OWL property characteristics, encoded as additional types.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum PropertyCharacteristic {
    /// At most one value.
    Functional,
    /// A value identifies at most one subject.
    InverseFunctional,
    /// The property composes with itself.
    Transitive,
    /// The property is its own inverse.
    Symmetric,
    /// No pair is both directions.
    Asymmetric,
    /// Every individual relates to itself.
    Reflexive,
    /// No individual relates to itself.
    Irreflexive,
}

impl PropertyCharacteristic {
    /// Every characteristic, in a stable order.
    pub const ALL: [Self; 7] = [
        Self::Functional,
        Self::InverseFunctional,
        Self::Transitive,
        Self::Symmetric,
        Self::Asymmetric,
        Self::Reflexive,
        Self::Irreflexive,
    ];

    /// The OWL class local name (`FunctionalProperty`, …).
    #[must_use]
    pub const fn local_name(self) -> &'static str {
        match self {
            Self::Functional => "FunctionalProperty",
            Self::InverseFunctional => "InverseFunctionalProperty",
            Self::Transitive => "TransitiveProperty",
            Self::Symmetric => "SymmetricProperty",
            Self::Asymmetric => "AsymmetricProperty",
            Self::Reflexive => "ReflexiveProperty",
            Self::Irreflexive => "IrreflexiveProperty",
        }
    }
}

/// A class restriction attached through `rdfs:subClassOf`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "restriction", rename_all = "camelCase")]
pub enum Restriction {
    /// `owl:someValuesFrom`.
    SomeValuesFrom {
        /// Restricted property.
        property: Id,
        /// Filler class.
        class: Id,
    },
    /// `owl:allValuesFrom`.
    AllValuesFrom {
        /// Restricted property.
        property: Id,
        /// Filler class.
        class: Id,
    },
    /// `owl:hasValue`.
    HasValue {
        /// Restricted property.
        property: Id,
        /// Required value.
        value: Term,
    },
    /// `owl:minCardinality` / `owl:minQualifiedCardinality`.
    MinCardinality {
        /// Restricted property.
        property: Id,
        /// Minimum count.
        cardinality: u32,
        /// Filler class for qualified cardinality.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        class: Option<Id>,
    },
    /// `owl:maxCardinality` / `owl:maxQualifiedCardinality`.
    MaxCardinality {
        /// Restricted property.
        property: Id,
        /// Maximum count.
        cardinality: u32,
        /// Filler class for qualified cardinality.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        class: Option<Id>,
    },
    /// `owl:cardinality` / `owl:qualifiedCardinality`.
    ExactCardinality {
        /// Restricted property.
        property: Id,
        /// Exact count.
        cardinality: u32,
        /// Filler class for qualified cardinality.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        class: Option<Id>,
    },
}

impl Restriction {
    /// The restricted property.
    #[must_use]
    pub fn property(&self) -> &Id {
        match self {
            Self::SomeValuesFrom { property, .. }
            | Self::AllValuesFrom { property, .. }
            | Self::HasValue { property, .. }
            | Self::MinCardinality { property, .. }
            | Self::MaxCardinality { property, .. }
            | Self::ExactCardinality { property, .. } => property,
        }
    }

    /// The filler class, when the restriction names one.
    #[must_use]
    pub fn class(&self) -> Option<&Id> {
        match self {
            Self::SomeValuesFrom { class, .. } | Self::AllValuesFrom { class, .. } => Some(class),
            Self::MinCardinality { class, .. }
            | Self::MaxCardinality { class, .. }
            | Self::ExactCardinality { class, .. } => class.as_ref(),
            Self::HasValue { .. } => None,
        }
    }
}

/// An OWL class definition.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OwlClass {
    /// Class IRI.
    pub id: Id,
    /// Labels keyed by language tag (`""` for untagged).
    #[serde(default)]
    pub labels: BTreeMap<String, String>,
    /// Comments keyed by language tag (`""` for untagged).
    #[serde(default)]
    pub comments: BTreeMap<String, String>,
    /// Direct superclasses.
    #[serde(default)]
    pub sub_class_of: Vec<Id>,
    /// Equivalent classes.
    #[serde(default)]
    pub equivalent_classes: Vec<Id>,
    /// Disjoint classes.
    #[serde(default)]
    pub disjoint_with: Vec<Id>,
    /// Restrictions applied through `rdfs:subClassOf`.
    #[serde(default)]
    pub restrictions: Vec<Restriction>,
    /// Whether the class is marked `owl:deprecated`.
    #[serde(default)]
    pub deprecated: bool,
}

impl OwlClass {
    /// Starts a class definition.
    #[must_use]
    pub fn new(id: Id) -> Self {
        Self {
            id,
            labels: BTreeMap::new(),
            comments: BTreeMap::new(),
            sub_class_of: Vec::new(),
            equivalent_classes: Vec::new(),
            disjoint_with: Vec::new(),
            restrictions: Vec::new(),
            deprecated: false,
        }
    }

    /// Adds a label; `language` is `""` for an untagged label.
    #[must_use]
    pub fn with_label(mut self, language: impl Into<String>, value: impl Into<String>) -> Self {
        self.labels.insert(language.into(), value.into());
        self
    }

    /// Adds a comment.
    #[must_use]
    pub fn with_comment(mut self, language: impl Into<String>, value: impl Into<String>) -> Self {
        self.comments.insert(language.into(), value.into());
        self
    }

    /// Adds a direct superclass.
    #[must_use]
    pub fn with_superclass(mut self, class: Id) -> Self {
        self.sub_class_of.push(class);
        self
    }

    /// Adds an equivalent class.
    #[must_use]
    pub fn with_equivalent_class(mut self, class: Id) -> Self {
        self.equivalent_classes.push(class);
        self
    }

    /// Adds a disjoint class.
    #[must_use]
    pub fn with_disjoint_class(mut self, class: Id) -> Self {
        self.disjoint_with.push(class);
        self
    }

    /// Adds a restriction.
    #[must_use]
    pub fn with_restriction(mut self, restriction: Restriction) -> Self {
        self.restrictions.push(restriction);
        self
    }
}

/// An OWL property definition.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OwlProperty {
    /// Property IRI.
    pub id: Id,
    /// Object, datatype, or annotation property.
    pub kind: PropertyKind,
    /// Labels keyed by language tag (`""` for untagged).
    #[serde(default)]
    pub labels: BTreeMap<String, String>,
    /// Comments keyed by language tag (`""` for untagged).
    #[serde(default)]
    pub comments: BTreeMap<String, String>,
    /// Direct super-properties.
    #[serde(default)]
    pub sub_property_of: Vec<Id>,
    /// Equivalent properties.
    #[serde(default)]
    pub equivalent_properties: Vec<Id>,
    /// Domains.
    #[serde(default)]
    pub domain: Vec<Id>,
    /// Ranges; classes for object properties, datatypes for datatype properties.
    #[serde(default)]
    pub range: Vec<Id>,
    /// Inverse property (object properties only).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub inverse_of: Option<Id>,
    /// OWL characteristics.
    #[serde(default)]
    pub characteristics: Vec<PropertyCharacteristic>,
    /// Whether the property is marked `owl:deprecated`.
    #[serde(default)]
    pub deprecated: bool,
}

impl OwlProperty {
    /// Starts a property definition.
    #[must_use]
    pub fn new(id: Id, kind: PropertyKind) -> Self {
        Self {
            id,
            kind,
            labels: BTreeMap::new(),
            comments: BTreeMap::new(),
            sub_property_of: Vec::new(),
            equivalent_properties: Vec::new(),
            domain: Vec::new(),
            range: Vec::new(),
            inverse_of: None,
            characteristics: Vec::new(),
            deprecated: false,
        }
    }

    /// Adds a label; `language` is `""` for an untagged label.
    #[must_use]
    pub fn with_label(mut self, language: impl Into<String>, value: impl Into<String>) -> Self {
        self.labels.insert(language.into(), value.into());
        self
    }

    /// Adds a comment.
    #[must_use]
    pub fn with_comment(mut self, language: impl Into<String>, value: impl Into<String>) -> Self {
        self.comments.insert(language.into(), value.into());
        self
    }

    /// Adds a direct super-property.
    #[must_use]
    pub fn with_sub_property_of(mut self, property: Id) -> Self {
        self.sub_property_of.push(property);
        self
    }

    /// Adds an equivalent property.
    #[must_use]
    pub fn with_equivalent_property(mut self, property: Id) -> Self {
        self.equivalent_properties.push(property);
        self
    }

    /// Adds a domain class.
    #[must_use]
    pub fn with_domain(mut self, class: Id) -> Self {
        self.domain.push(class);
        self
    }

    /// Adds a range (class or datatype).
    #[must_use]
    pub fn with_range(mut self, range: Id) -> Self {
        self.range.push(range);
        self
    }

    /// Sets the inverse property.
    #[must_use]
    pub fn with_inverse(mut self, property: Id) -> Self {
        self.inverse_of = Some(property);
        self
    }

    /// Adds a characteristic.
    #[must_use]
    pub fn with_characteristic(mut self, characteristic: PropertyCharacteristic) -> Self {
        self.characteristics.push(characteristic);
        self
    }
}

/// An ontology resource.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Ontology {
    /// Ontology IRI.
    pub id: Id,
    /// Labels keyed by language tag.
    #[serde(default)]
    pub labels: BTreeMap<String, String>,
    /// Comments keyed by language tag.
    #[serde(default)]
    pub comments: BTreeMap<String, String>,
    /// `owl:versionInfo`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub version_info: Option<String>,
    /// `owl:versionIRI`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub version_iri: Option<Id>,
    /// `owl:imports` targets.
    #[serde(default)]
    pub imports: Vec<Id>,
}

impl Ontology {
    /// Starts an ontology definition.
    #[must_use]
    pub fn new(id: Id) -> Self {
        Self {
            id,
            labels: BTreeMap::new(),
            comments: BTreeMap::new(),
            version_info: None,
            version_iri: None,
            imports: Vec::new(),
        }
    }

    /// Adds a label.
    #[must_use]
    pub fn with_label(mut self, language: impl Into<String>, value: impl Into<String>) -> Self {
        self.labels.insert(language.into(), value.into());
        self
    }

    /// Sets the version information and version IRI.
    #[must_use]
    pub fn with_version(mut self, version_iri: Id, version_info: impl Into<String>) -> Self {
        self.version_iri = Some(version_iri);
        self.version_info = Some(version_info.into());
        self
    }

    /// Adds an import.
    #[must_use]
    pub fn with_import(mut self, ontology: Id) -> Self {
        self.imports.push(ontology);
        self
    }
}

/// A recorded schema version of an ontology.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OntologyVersion {
    /// The ontology this version belongs to.
    pub ontology: Id,
    /// Recorded `owl:versionInfo`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub version_info: Option<String>,
    /// Recorded `owl:versionIRI`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub version_iri: Option<Id>,
    /// When the version was recorded.
    pub recorded_at: chrono::DateTime<chrono::Utc>,
}

/// Which graph schema operations read or validate.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "scope", content = "graph", rename_all = "camelCase")]
pub enum GraphScope {
    /// The manager's schema graph (`system:schema` by default).
    Schema,
    /// A specific named graph.
    Graph(GraphName),
}

/// Filter options for class listings.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ClassQuery {
    /// Only direct subclasses of this class.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parent: Option<Id>,
    /// Case-insensitive substring match against any label.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub label_contains: Option<String>,
    /// Maximum results.
    pub limit: usize,
    /// Results to skip.
    pub offset: usize,
}

impl Default for ClassQuery {
    fn default() -> Self {
        Self {
            parent: None,
            label_contains: None,
            limit: 100,
            offset: 0,
        }
    }
}

impl ClassQuery {
    /// Restricts to direct subclasses.
    #[must_use]
    pub fn with_parent(mut self, parent: Id) -> Self {
        self.parent = Some(parent);
        self
    }

    /// Restricts by label substring.
    #[must_use]
    pub fn with_label(mut self, text: impl Into<String>) -> Self {
        self.label_contains = Some(text.into());
        self
    }

    /// Sets pagination.
    #[must_use]
    pub fn with_pagination(mut self, limit: usize, offset: usize) -> Self {
        self.limit = limit;
        self.offset = offset;
        self
    }
}

/// Filter options for property listings.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct PropertyQuery {
    /// Restrict to a property kind.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub kind: Option<PropertyKind>,
    /// Case-insensitive substring match against any label.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub label_contains: Option<String>,
    /// Maximum results.
    pub limit: usize,
    /// Results to skip.
    pub offset: usize,
}

impl PropertyQuery {
    /// Creates a query with the default page size.
    #[must_use]
    pub fn new() -> Self {
        Self {
            kind: None,
            label_contains: None,
            limit: 100,
            offset: 0,
        }
    }

    /// Restricts to a property kind.
    #[must_use]
    pub fn with_kind(mut self, kind: PropertyKind) -> Self {
        self.kind = Some(kind);
        self
    }

    /// Restricts by label substring.
    #[must_use]
    pub fn with_label(mut self, text: impl Into<String>) -> Self {
        self.label_contains = Some(text.into());
        self
    }
}

/// The position of a class in the subclass graph.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ClassHierarchy {
    /// The requested class.
    pub class: Id,
    /// Direct superclasses.
    pub direct_superclasses: Vec<Id>,
    /// Direct subclasses.
    pub direct_subclasses: Vec<Id>,
    /// Transitive superclasses.
    pub ancestors: Vec<Id>,
    /// Transitive subclasses.
    pub descendants: Vec<Id>,
}

/// Severity of a schema validation finding.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Severity {
    /// The schema is inconsistent.
    Error,
    /// Suspicious but not necessarily wrong.
    Warning,
}

/// One schema validation finding.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ValidationIssue {
    /// Stable machine-readable code.
    pub code: String,
    /// Error or warning.
    pub severity: Severity,
    /// Offending resource.
    pub subject: Id,
    /// Human-readable explanation.
    pub message: String,
}

/// Result of a schema validation pass.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ValidationReport {
    /// Validated scope.
    pub scope: GraphScope,
    /// Number of classes checked.
    pub checked_classes: usize,
    /// Number of properties checked.
    pub checked_properties: usize,
    /// Findings.
    pub issues: Vec<ValidationIssue>,
}

impl ValidationReport {
    /// Whether no error-severity issue was found.
    #[must_use]
    pub fn is_valid(&self) -> bool {
        !self
            .issues
            .iter()
            .any(|issue| issue.severity == Severity::Error)
    }

    /// Iterates over error-severity issues.
    pub fn errors(&self) -> impl Iterator<Item = &ValidationIssue> {
        self.issues
            .iter()
            .filter(|issue| issue.severity == Severity::Error)
    }
}

/// OWL schema management capability.
///
/// The methods are asynchronous so every interface (HTTP, CLI, MCP) shares one
/// signature; the current implementation performs synchronous engine work
/// inside them and will move to blocking workers with the HTTP layer.
#[async_trait::async_trait]
pub trait OntologyManager: Send + Sync {
    /// Creates a class; fails if the IRI already denotes a class.
    async fn create_class(&self, class: OwlClass) -> Result<Id>;
    /// Replaces a class definition.
    async fn update_class(&self, class: OwlClass) -> Result<Id>;
    /// Deletes a class definition and its anonymous restrictions, returning the
    /// number of removed statements.
    async fn delete_class(&self, id: &Id) -> Result<usize>;
    /// Creates a property; fails if the IRI already denotes a property.
    async fn create_property(&self, property: OwlProperty) -> Result<Id>;
    /// Replaces a property definition.
    async fn update_property(&self, property: OwlProperty) -> Result<Id>;
    /// Deletes a property definition, returning the number of removed statements.
    async fn delete_property(&self, id: &Id) -> Result<usize>;
    /// Loads a class definition.
    async fn get_class(&self, id: &Id) -> Result<OwlClass>;
    /// Loads a property definition.
    async fn get_property(&self, id: &Id) -> Result<OwlProperty>;
    /// Lists classes with optional filtering.
    async fn list_classes(&self, query: ClassQuery) -> Result<Vec<OwlClass>>;
    /// Lists properties with optional filtering.
    async fn list_properties(&self, query: PropertyQuery) -> Result<Vec<OwlProperty>>;
    /// Returns the subclass hierarchy around a class.
    async fn hierarchy(&self, class: &Id) -> Result<ClassHierarchy>;
    /// Validates a schema scope.
    async fn validate(&self, scope: GraphScope) -> Result<ValidationReport>;
    /// Registers an ontology.
    async fn create_ontology(&self, ontology: Ontology) -> Result<Id>;
    /// Loads an ontology definition.
    async fn get_ontology(&self, id: &Id) -> Result<Ontology>;
    /// Lists registered ontologies.
    async fn list_ontologies(&self) -> Result<Vec<Ontology>>;
    /// Replaces an ontology definition and records a new version.
    async fn update_ontology(&self, ontology: Ontology) -> Result<Id>;
    /// Deletes an ontology definition and its recorded versions.
    async fn delete_ontology(&self, id: &Id) -> Result<usize>;
    /// Returns the recorded versions of an ontology.
    async fn ontology_versions(&self, id: &Id) -> Result<Vec<OntologyVersion>>;
    /// Returns properties applicable to a class through domains and inheritance.
    async fn applicable_properties(&self, class: &Id) -> Result<Vec<OwlProperty>>;
    /// Returns classes whose restrictions reference any of the given properties.
    async fn applicable_classes(&self, properties: &[Id]) -> Result<Vec<OwlClass>>;
}
