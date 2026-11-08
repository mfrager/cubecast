//! SKOS taxonomy domain types and the `TaxonomyManager` capability trait.
//!
//! Hierarchies are stored canonically: only `skos:broader` edges and a single
//! direction of symmetric relations are persisted, and inverse readings are
//! derived, so the schema cannot hold contradictory broader/narrower pairs.

use std::collections::BTreeMap;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::error::Result;
use crate::id::Id;
use crate::owl::Severity;

/// A text value with an optional language tag.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct LocalizedText {
    /// Lexical value.
    pub value: String,
    /// Language tag, when present.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub language: Option<String>,
}

impl LocalizedText {
    /// Creates a localized text value; an empty language means untagged.
    #[must_use]
    pub fn new(value: impl Into<String>, language: &str) -> Self {
        let language = language.trim();
        Self {
            value: value.into(),
            language: if language.is_empty() {
                None
            } else {
                Some(language.to_owned())
            },
        }
    }
}

/// A SKOS concept scheme.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ConceptScheme {
    /// Scheme IRI.
    pub id: Id,
    /// Preferred labels keyed by language tag (`""` for untagged).
    #[serde(default)]
    pub labels: BTreeMap<String, String>,
    /// Definitions keyed by language tag.
    #[serde(default)]
    pub definitions: BTreeMap<String, String>,
    /// `skos:hasTopConcept` values.
    #[serde(default)]
    pub top_concepts: Vec<Id>,
    /// Recorded version string.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
}

impl ConceptScheme {
    /// Starts a scheme definition.
    #[must_use]
    pub fn new(id: Id) -> Self {
        Self {
            id,
            labels: BTreeMap::new(),
            definitions: BTreeMap::new(),
            top_concepts: Vec::new(),
            version: None,
        }
    }

    /// Adds a preferred label.
    #[must_use]
    pub fn with_label(mut self, language: impl Into<String>, value: impl Into<String>) -> Self {
        self.labels.insert(language.into(), value.into());
        self
    }

    /// Adds a definition.
    #[must_use]
    pub fn with_definition(
        mut self,
        language: impl Into<String>,
        value: impl Into<String>,
    ) -> Self {
        self.definitions.insert(language.into(), value.into());
        self
    }

    /// Declares a top concept.
    #[must_use]
    pub fn with_top_concept(mut self, concept: Id) -> Self {
        self.top_concepts.push(concept);
        self
    }

    /// Sets the version string.
    #[must_use]
    pub fn with_version(mut self, version: impl Into<String>) -> Self {
        self.version = Some(version.into());
        self
    }
}

/// A SKOS concept.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Concept {
    /// Concept IRI.
    pub id: Id,
    /// Scheme membership (`skos:inScheme`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub scheme: Option<Id>,
    /// Preferred labels.
    #[serde(default)]
    pub pref_labels: Vec<LocalizedText>,
    /// Alternative labels.
    #[serde(default)]
    pub alt_labels: Vec<LocalizedText>,
    /// Hidden labels used for matching only.
    #[serde(default)]
    pub hidden_labels: Vec<LocalizedText>,
    /// Definitions.
    #[serde(default)]
    pub definitions: Vec<LocalizedText>,
    /// Notations (`skos:notation`).
    #[serde(default)]
    pub notations: Vec<String>,
    /// Broader concepts.
    #[serde(default)]
    pub broader: Vec<Id>,
    /// Narrower concepts (derived from the inverse edges).
    #[serde(default)]
    pub narrower: Vec<Id>,
    /// Associative relations.
    #[serde(default)]
    pub related: Vec<Id>,
    /// Whether the concept is marked deprecated.
    #[serde(default)]
    pub deprecated: bool,
}

impl Concept {
    /// Starts a concept definition.
    #[must_use]
    pub fn new(id: Id) -> Self {
        Self {
            id,
            scheme: None,
            pref_labels: Vec::new(),
            alt_labels: Vec::new(),
            hidden_labels: Vec::new(),
            definitions: Vec::new(),
            notations: Vec::new(),
            broader: Vec::new(),
            narrower: Vec::new(),
            related: Vec::new(),
            deprecated: false,
        }
    }

    /// Sets the scheme.
    #[must_use]
    pub fn with_scheme(mut self, scheme: Id) -> Self {
        self.scheme = Some(scheme);
        self
    }

    /// Adds a preferred label.
    #[must_use]
    pub fn with_pref_label(mut self, value: impl Into<String>, language: &str) -> Self {
        self.pref_labels.push(LocalizedText::new(value, language));
        self
    }

    /// Adds an alternative label.
    #[must_use]
    pub fn with_alt_label(mut self, value: impl Into<String>, language: &str) -> Self {
        self.alt_labels.push(LocalizedText::new(value, language));
        self
    }

    /// Adds a hidden label.
    #[must_use]
    pub fn with_hidden_label(mut self, value: impl Into<String>, language: &str) -> Self {
        self.hidden_labels.push(LocalizedText::new(value, language));
        self
    }

    /// Adds a definition.
    #[must_use]
    pub fn with_definition(mut self, value: impl Into<String>, language: &str) -> Self {
        self.definitions.push(LocalizedText::new(value, language));
        self
    }

    /// Adds a notation.
    #[must_use]
    pub fn with_notation(mut self, notation: impl Into<String>) -> Self {
        self.notations.push(notation.into());
        self
    }

    /// Adds a broader concept.
    #[must_use]
    pub fn with_broader(mut self, concept: Id) -> Self {
        self.broader.push(concept);
        self
    }

    /// Adds a narrower concept.
    #[must_use]
    pub fn with_narrower(mut self, concept: Id) -> Self {
        self.narrower.push(concept);
        self
    }

    /// Adds a related concept.
    #[must_use]
    pub fn with_related(mut self, concept: Id) -> Self {
        self.related.push(concept);
        self
    }
}

/// A SKOS collection (an unordered group of concepts).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ConceptCollection {
    /// Collection IRI.
    pub id: Id,
    /// Scheme membership.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub scheme: Option<Id>,
    /// Preferred labels keyed by language tag.
    #[serde(default)]
    pub labels: BTreeMap<String, String>,
    /// Member concepts (`skos:member`).
    #[serde(default)]
    pub members: Vec<Id>,
}

impl ConceptCollection {
    /// Starts a collection definition.
    #[must_use]
    pub fn new(id: Id) -> Self {
        Self {
            id,
            scheme: None,
            labels: BTreeMap::new(),
            members: Vec::new(),
        }
    }

    /// Sets the scheme.
    #[must_use]
    pub fn with_scheme(mut self, scheme: Id) -> Self {
        self.scheme = Some(scheme);
        self
    }

    /// Adds a label.
    #[must_use]
    pub fn with_label(mut self, language: impl Into<String>, value: impl Into<String>) -> Self {
        self.labels.insert(language.into(), value.into());
        self
    }

    /// Adds a member concept.
    #[must_use]
    pub fn with_member(mut self, concept: Id) -> Self {
        self.members.push(concept);
        self
    }
}

/// A recorded taxonomy version.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SchemeVersion {
    /// Scheme this version belongs to.
    pub scheme: Id,
    /// Recorded version string.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
    /// When the version was recorded.
    pub recorded_at: DateTime<Utc>,
}

/// A SKOS concept-to-concept or mapping relation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum SkosRelation {
    /// Hierarchical parent.
    Broader,
    /// Hierarchical child.
    Narrower,
    /// Associative relation.
    Related,
    /// Cross-scheme exact match.
    ExactMatch,
    /// Cross-scheme close match.
    CloseMatch,
    /// Cross-scheme broad match.
    BroadMatch,
    /// Cross-scheme narrow match.
    NarrowMatch,
    /// Cross-scheme related match.
    RelatedMatch,
}

impl SkosRelation {
    /// Every relation.
    pub const ALL: [Self; 8] = [
        Self::Broader,
        Self::Narrower,
        Self::Related,
        Self::ExactMatch,
        Self::CloseMatch,
        Self::BroadMatch,
        Self::NarrowMatch,
        Self::RelatedMatch,
    ];

    /// Parses the storage name of a relation.
    #[must_use]
    pub fn parse(value: &str) -> Option<Self> {
        Self::ALL
            .into_iter()
            .find(|relation| relation.local_name() == value)
    }

    /// The SKOS predicate local name.
    #[must_use]
    pub const fn local_name(self) -> &'static str {
        match self {
            Self::Broader => "broader",
            Self::Narrower => "narrower",
            Self::Related => "related",
            Self::ExactMatch => "exactMatch",
            Self::CloseMatch => "closeMatch",
            Self::BroadMatch => "broadMatch",
            Self::NarrowMatch => "narrowMatch",
            Self::RelatedMatch => "relatedMatch",
        }
    }

    /// Whether the relation is hierarchical.
    #[must_use]
    pub const fn is_hierarchical(self) -> bool {
        matches!(self, Self::Broader | Self::Narrower)
    }

    /// Whether the relation is symmetric in SKOS semantics.
    #[must_use]
    pub const fn is_symmetric(self) -> bool {
        matches!(
            self,
            Self::Related | Self::ExactMatch | Self::CloseMatch | Self::RelatedMatch
        )
    }

    /// The SKOS inverse, when one exists.
    #[must_use]
    pub const fn inverse(self) -> Self {
        match self {
            Self::Broader => Self::Narrower,
            Self::Narrower => Self::Broader,
            Self::BroadMatch => Self::NarrowMatch,
            Self::NarrowMatch => Self::BroadMatch,
            other => other,
        }
    }
}

/// Which label matched during a search.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum LabelKind {
    /// `skos:prefLabel`.
    Preferred,
    /// `skos:altLabel`.
    Alternative,
    /// `skos:hiddenLabel`.
    Hidden,
    /// `skos:notation`.
    Notation,
}

/// One concept search hit.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ConceptMatch {
    /// Matched concept.
    pub concept: Id,
    /// Scheme of the concept, when known.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub scheme: Option<Id>,
    /// Matched label text.
    pub label: String,
    /// Language of the matched label.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub language: Option<String>,
    /// Which label matched.
    pub kind: LabelKind,
    /// Match score in `0.0..=1.0`.
    pub score: f32,
}

/// A concept node with its children.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ConceptTree {
    /// Concept at this node.
    pub concept: Id,
    /// Preferred label used for display.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
    /// Nested narrower concepts.
    #[serde(default)]
    pub children: Vec<ConceptTree>,
}

/// A concept relation with its source.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ConceptRelation {
    /// Relation source.
    pub source: Id,
    /// Relation kind.
    pub relation: SkosRelation,
    /// Relation target.
    pub target: Id,
}

/// Filter and pagination options for concept listings.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct TaxonomyQuery {
    /// Restrict to one scheme.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub scheme: Option<Id>,
    /// Case-insensitive substring match against any label.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub label_contains: Option<String>,
    /// Maximum results.
    pub limit: usize,
    /// Results to skip.
    pub offset: usize,
}

impl TaxonomyQuery {
    /// Creates a query with the default page size.
    #[must_use]
    pub fn new() -> Self {
        Self {
            scheme: None,
            label_contains: None,
            limit: 100,
            offset: 0,
        }
    }

    /// Restricts to a scheme.
    #[must_use]
    pub fn with_scheme(mut self, scheme: Id) -> Self {
        self.scheme = Some(scheme);
        self
    }

    /// Restricts by label substring.
    #[must_use]
    pub fn with_label(mut self, text: impl Into<String>) -> Self {
        self.label_contains = Some(text.into());
        self
    }
}

/// One taxonomy validation finding.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TaxonomyIssue {
    /// Stable machine-readable code.
    pub code: String,
    /// Error or warning.
    pub severity: Severity,
    /// Offending concept.
    pub concept: Id,
    /// Human-readable explanation.
    pub message: String,
}

/// Result of a taxonomy validation or merge pass.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TaxonomyReport {
    /// Scheme the report applies to, when scoped.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub scheme: Option<Id>,
    /// Number of concepts considered.
    pub checked_concepts: usize,
    /// Findings.
    pub issues: Vec<TaxonomyIssue>,
}

impl TaxonomyReport {
    /// Whether no error-severity issue was found.
    #[must_use]
    pub fn is_valid(&self) -> bool {
        !self
            .issues
            .iter()
            .any(|issue| issue.severity == Severity::Error)
    }

    /// Iterates over error-severity issues.
    pub fn errors(&self) -> impl Iterator<Item = &TaxonomyIssue> {
        self.issues
            .iter()
            .filter(|issue| issue.severity == Severity::Error)
    }
}

/// SKOS taxonomy management capability.
#[async_trait::async_trait]
pub trait TaxonomyManager: Send + Sync {
    /// Creates a scheme and records its initial version.
    async fn create_scheme(&self, scheme: ConceptScheme) -> Result<Id>;
    /// Replaces a scheme definition and records a version.
    async fn update_scheme(&self, scheme: ConceptScheme) -> Result<Id>;
    /// Deletes a scheme definition, returning removed statements.
    async fn delete_scheme(&self, id: &Id) -> Result<usize>;
    /// Loads a scheme.
    async fn get_scheme(&self, id: &Id) -> Result<ConceptScheme>;
    /// Lists schemes.
    async fn list_schemes(&self) -> Result<Vec<ConceptScheme>>;
    /// Returns recorded scheme versions.
    async fn scheme_versions(&self, id: &Id) -> Result<Vec<SchemeVersion>>;
    /// Creates a concept, rejecting duplicate preferred labels in the scheme.
    async fn create_concept(&self, concept: Concept) -> Result<Id>;
    /// Replaces a concept definition.
    async fn update_concept(&self, concept: Concept) -> Result<Id>;
    /// Deletes a concept and its relation edges, returning removed statements.
    async fn delete_concept(&self, id: &Id) -> Result<usize>;
    /// Loads a concept.
    async fn get_concept(&self, id: &Id) -> Result<Concept>;
    /// Lists concepts.
    async fn list_concepts(&self, query: TaxonomyQuery) -> Result<Vec<Concept>>;
    /// Searches preferred, alternative, hidden labels, and notations.
    async fn search(&self, query: &str, scheme: Option<&Id>) -> Result<Vec<ConceptMatch>>;
    /// Adds a relation, rejecting cycles and unknown targets.
    async fn add_relation(&self, source: &Id, relation: SkosRelation, target: &Id) -> Result<()>;
    /// Removes a relation.
    async fn remove_relation(&self, source: &Id, relation: SkosRelation, target: &Id)
        -> Result<()>;
    /// Lists relations involving a concept.
    async fn relations(&self, id: &Id) -> Result<Vec<ConceptRelation>>;
    /// Returns transitive narrower concepts.
    async fn descendants(&self, id: &Id) -> Result<Vec<Concept>>;
    /// Returns transitive broader concepts.
    async fn ancestors(&self, id: &Id) -> Result<Vec<Concept>>;
    /// Builds the tree for a scheme.
    async fn tree(&self, scheme: &Id) -> Result<Vec<ConceptTree>>;
    /// Moves a concept under a new parent and/or into another scheme.
    async fn move_concept(
        &self,
        id: &Id,
        new_broader: Option<&Id>,
        scheme: Option<&Id>,
    ) -> Result<()>;
    /// Merges `source` into `target`.
    async fn merge_schemes(&self, source: &Id, target: &Id) -> Result<TaxonomyReport>;
    /// Validates a scheme, or the whole taxonomy store.
    async fn validate(&self, scheme: Option<&Id>) -> Result<TaxonomyReport>;
    /// Creates a collection.
    async fn create_collection(&self, collection: ConceptCollection) -> Result<Id>;
    /// Loads a collection.
    async fn get_collection(&self, id: &Id) -> Result<ConceptCollection>;
    /// Lists collections, optionally scoped to a scheme.
    async fn list_collections(&self, scheme: Option<&Id>) -> Result<Vec<ConceptCollection>>;
    /// Deletes a collection, returning removed statements.
    async fn delete_collection(&self, id: &Id) -> Result<usize>;
}
