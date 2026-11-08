//! Application-level RDF data model.
//!
//! These types mirror RDF 1.1 without exposing OxRDF: subjects, predicates,
//! objects, graph names, quads, statement records, and instances. The storage
//! adapter translates to and from the engine's types.

use std::collections::BTreeMap;
use std::fmt;

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::error::{Error, Result};
use crate::graph::GraphKind;
use crate::id::Id;
use crate::namespace;

/// An RDF literal.
///
/// `language` and `datatype` are mutually exclusive in valid RDF. Simple
/// literals carry neither, which lets the model round-trip `xsd:string`
/// literals without treating them as a distinct kind.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct Literal {
    /// Lexical form.
    pub value: String,
    /// Language tag, if this is a language-tagged string.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub language: Option<String>,
    /// Datatype IRI, if this literal is typed.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub datatype: Option<Id>,
}

impl Literal {
    /// Creates a simple literal.
    #[must_use]
    pub fn simple(value: impl Into<String>) -> Self {
        Self {
            value: value.into(),
            language: None,
            datatype: None,
        }
    }

    /// Creates a typed literal.
    #[must_use]
    pub fn typed(value: impl Into<String>, datatype: Id) -> Self {
        Self {
            value: value.into(),
            language: None,
            datatype: Some(datatype),
        }
    }

    /// Creates a language-tagged literal.
    ///
    /// # Errors
    ///
    /// Returns [`Error::Validation`] when the language tag is empty.
    pub fn language_tagged(value: impl Into<String>, language: impl Into<String>) -> Result<Self> {
        let language = language.into();
        if language.trim().is_empty() {
            return Err(Error::validation("language tag must not be empty"));
        }
        Ok(Self {
            value: value.into(),
            language: Some(language),
            datatype: None,
        })
    }

    /// Creates an `xsd:integer` literal.
    ///
    /// # Panics
    ///
    /// Panics only if the XSD namespace constant is not a valid IRI; unit tests
    /// cover it.
    #[must_use]
    pub fn integer(value: i64) -> Self {
        Self::typed(
            value.to_string(),
            Id::new(namespace::XSD_INTEGER).expect("xsd:integer is a valid IRI"),
        )
    }

    /// Creates an `xsd:boolean` literal.
    ///
    /// # Panics
    ///
    /// Panics only if the XSD namespace constant is not a valid IRI.
    #[must_use]
    pub fn boolean(value: bool) -> Self {
        Self::typed(
            value.to_string(),
            Id::new(namespace::XSD_BOOLEAN).expect("xsd:boolean is a valid IRI"),
        )
    }

    /// Creates an `xsd:dateTime` literal from a UTC timestamp.
    ///
    /// # Panics
    ///
    /// Panics only if the XSD namespace constant is not a valid IRI.
    #[must_use]
    pub fn datetime(value: &chrono::DateTime<chrono::Utc>) -> Self {
        Self::typed(
            value.to_rfc3339_opts(chrono::SecondsFormat::Secs, true),
            Id::new(namespace::XSD_DATETIME).expect("xsd:dateTime is a valid IRI"),
        )
    }
}

impl fmt::Display for Literal {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let escaped = escape_nt(&self.value);
        if let Some(language) = &self.language {
            write!(formatter, "\"{escaped}\"@{language}")
        } else if let Some(datatype) = &self.datatype {
            write!(formatter, "\"{escaped}\"^^<{datatype}>")
        } else {
            write!(formatter, "\"{escaped}\"")
        }
    }
}

/// An RDF term.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(tag = "term", content = "value", rename_all = "camelCase")]
pub enum Term {
    /// An IRI term.
    Iri(Id),
    /// A blank node identifier (without the `_:` prefix).
    BlankNode(String),
    /// A literal term.
    Literal(Literal),
}

impl Term {
    /// Creates an IRI term.
    #[must_use]
    pub fn iri(id: Id) -> Self {
        Self::Iri(id)
    }

    /// Creates a simple literal term.
    #[must_use]
    pub fn string(value: impl Into<String>) -> Self {
        Self::Literal(Literal::simple(value))
    }

    /// Creates a language-tagged literal term.
    ///
    /// # Errors
    ///
    /// Returns [`Error::Validation`] when the language tag is empty.
    pub fn language_tagged(value: impl Into<String>, language: impl Into<String>) -> Result<Self> {
        Ok(Self::Literal(Literal::language_tagged(value, language)?))
    }

    /// Returns the IRI if this term is an IRI.
    #[must_use]
    pub fn as_iri(&self) -> Option<&Id> {
        match self {
            Self::Iri(id) => Some(id),
            _ => None,
        }
    }

    /// Consumes the term and returns the IRI if it is one.
    #[must_use]
    pub fn into_iri(self) -> Option<Id> {
        match self {
            Self::Iri(id) => Some(id),
            _ => None,
        }
    }

    /// Returns whether this term may appear in subject position.
    #[must_use]
    pub fn is_subject(&self) -> bool {
        matches!(self, Self::Iri(_) | Self::BlankNode(_))
    }
}

impl fmt::Display for Term {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Iri(id) => write!(formatter, "<{id}>"),
            Self::BlankNode(id) => write!(formatter, "_:{id}"),
            Self::Literal(literal) => literal.fmt(formatter),
        }
    }
}

/// The graph a quad belongs to.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(tag = "graph", content = "name", rename_all = "camelCase")]
pub enum GraphName {
    /// The unnamed default graph.
    Default,
    /// A named graph.
    Iri(Id),
    /// A blank-node named graph.
    BlankNode(String),
}

impl GraphName {
    /// Returns the graph IRI, if the graph is named by an IRI.
    #[must_use]
    pub fn as_iri(&self) -> Option<&Id> {
        match self {
            Self::Iri(id) => Some(id),
            _ => None,
        }
    }
}

impl fmt::Display for GraphName {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Default => formatter.write_str("default"),
            Self::Iri(id) => formatter.write_str(id.as_str()),
            Self::BlankNode(id) => write!(formatter, "_:{id}"),
        }
    }
}

/// An RDF quad.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct Quad {
    /// Subject (an IRI or blank node).
    pub subject: Term,
    /// Predicate (always an IRI).
    pub predicate: Id,
    /// Object (an IRI, blank node, or literal).
    pub object: Term,
    /// Graph the statement belongs to.
    pub graph: GraphName,
}

impl Quad {
    /// Creates a quad in the default graph.
    #[must_use]
    pub fn new(subject: Term, predicate: Id, object: Term) -> Self {
        Self {
            subject,
            predicate,
            object,
            graph: GraphName::Default,
        }
    }

    /// Creates a quad in a named graph.
    #[must_use]
    pub fn in_graph(subject: Term, predicate: Id, object: Term, graph: GraphName) -> Self {
        Self {
            subject,
            predicate,
            object,
            graph,
        }
    }

    /// Returns the quad placed in `graph`.
    #[must_use]
    pub fn with_graph(mut self, graph: GraphName) -> Self {
        self.graph = graph;
        self
    }

    /// Checks the RDF structural invariant that subjects are never literals.
    ///
    /// # Errors
    ///
    /// Returns [`Error::Validation`] when the subject is a literal.
    pub fn validate(&self) -> Result<()> {
        if self.subject.is_subject() {
            Ok(())
        } else {
            Err(Error::validation(format!(
                "literal subject is not allowed: {}",
                self.subject
            )))
        }
    }
}

/// A filter over quad components; `None` means "any value".
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct QuadPattern {
    /// Subject filter.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub subject: Option<Term>,
    /// Predicate filter.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub predicate: Option<Id>,
    /// Object filter.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub object: Option<Term>,
    /// Graph filter.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub graph: Option<GraphName>,
}

impl QuadPattern {
    /// Creates a pattern matching every quad.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Restricts the subject.
    #[must_use]
    pub fn with_subject(mut self, subject: Term) -> Self {
        self.subject = Some(subject);
        self
    }

    /// Restricts the predicate.
    #[must_use]
    pub fn with_predicate(mut self, predicate: Id) -> Self {
        self.predicate = Some(predicate);
        self
    }

    /// Restricts the object.
    #[must_use]
    pub fn with_object(mut self, object: Term) -> Self {
        self.object = Some(object);
        self
    }

    /// Restricts the graph.
    #[must_use]
    pub fn with_graph(mut self, graph: GraphName) -> Self {
        self.graph = Some(graph);
        self
    }

    /// Matches exactly the given quad.
    #[must_use]
    pub fn with_quad(mut self, quad: &Quad) -> Self {
        self.subject = Some(quad.subject.clone());
        self.predicate = Some(quad.predicate.clone());
        self.object = Some(quad.object.clone());
        self.graph = Some(quad.graph.clone());
        self
    }
}

/// The result of a SPARQL query.
///
/// `resultType` distinguishes the shapes; `type` is reserved for the enclosing
/// [`crate::command::CommandResult`] tag.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(into = "QueryResultRepr", from = "QueryResultRepr")]
pub enum QueryResult {
    /// A `SELECT` result.
    Solutions {
        /// Variable names in projection order.
        variables: Vec<String>,
        /// Rows aligned with `variables`; unbound cells are `None`.
        rows: Vec<Vec<Option<Term>>>,
    },
    /// An `ASK` result.
    Boolean(bool),
    /// A `CONSTRUCT` or `DESCRIBE` result.
    Graph(Vec<Quad>),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "resultType", rename_all = "camelCase")]
enum QueryResultRepr {
    Solutions {
        variables: Vec<String>,
        rows: Vec<Vec<Option<Term>>>,
    },
    Boolean {
        value: bool,
    },
    Graph {
        quads: Vec<Quad>,
    },
}

impl From<QueryResult> for QueryResultRepr {
    fn from(result: QueryResult) -> Self {
        match result {
            QueryResult::Solutions { variables, rows } => Self::Solutions { variables, rows },
            QueryResult::Boolean(value) => Self::Boolean { value },
            QueryResult::Graph(quads) => Self::Graph { quads },
        }
    }
}

impl From<QueryResultRepr> for QueryResult {
    fn from(result: QueryResultRepr) -> Self {
        match result {
            QueryResultRepr::Solutions { variables, rows } => Self::Solutions { variables, rows },
            QueryResultRepr::Boolean { value } => Self::Boolean(value),
            QueryResultRepr::Graph { quads } => Self::Graph(quads),
        }
    }
}

impl QueryResult {
    /// Number of rows, triples, or `1`/`0` for a boolean result.
    #[must_use]
    pub fn count(&self) -> usize {
        match self {
            Self::Solutions { rows, .. } => rows.len(),
            Self::Boolean(value) => usize::from(*value),
            Self::Graph(quads) => quads.len(),
        }
    }
}

/// Stable identifier of a statement inside an instance.
///
/// Derived deterministically from the quad's canonical N-Triples form, so the
/// same statement keeps the same identifier across restarts and processes
/// without a separate identifier index.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct StatementId(String);

impl StatementId {
    /// Wraps an existing statement identifier.
    ///
    /// # Errors
    ///
    /// Returns [`Error::Validation`] when the value is empty.
    pub fn new(value: impl Into<String>) -> Result<Self> {
        let value = value.into();
        if value.trim().is_empty() {
            return Err(Error::validation("statement identifier must not be empty"));
        }
        Ok(Self(value))
    }

    /// Derives the statement identifier from a quad.
    #[must_use]
    pub fn of(quad: &Quad) -> Self {
        let canonical = format!(
            "{} {} {} {}",
            quad.subject, quad.predicate, quad.object, quad.graph
        );
        let digest = Sha256::digest(canonical.as_bytes());
        let mut hex = String::with_capacity(digest.len() * 2);
        for byte in digest {
            use std::fmt::Write as _;
            let _ = write!(hex, "{byte:02x}");
        }
        Self(format!("urn:onto:statement:{hex}"))
    }

    /// Returns the identifier text.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for StatementId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

/// Provenance category of a statement, derived from its graph.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum StatementKind {
    /// Explicitly provided or imported.
    Asserted,
    /// Produced by reasoning.
    Inferred,
    /// Produced by classification or another derived computation.
    Derived,
}

impl StatementKind {
    /// Classifies a statement by the graph it lives in.
    #[must_use]
    pub fn from_graph(graph: &GraphName) -> Self {
        match graph.as_iri().and_then(GraphKind::from_iri) {
            Some(GraphKind::Inferred) => Self::Inferred,
            Some(GraphKind::Classification) => Self::Derived,
            _ => Self::Asserted,
        }
    }
}

/// A stored statement with its stable identifier and provenance category.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Statement {
    /// Stable identifier.
    pub id: StatementId,
    /// The underlying quad.
    pub quad: Quad,
    /// Asserted, inferred, or derived.
    pub kind: StatementKind,
}

impl Statement {
    /// Wraps a quad into a statement record.
    #[must_use]
    pub fn new(quad: Quad) -> Self {
        Self {
            id: StatementId::of(&quad),
            kind: StatementKind::from_graph(&quad.graph),
            quad,
        }
    }
}

/// A managed instance: a resource with statements in one graph.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Instance {
    /// Resource IRI.
    pub id: Id,
    /// Graph the statements were read from.
    pub graph: GraphName,
    /// `rdf:type` values.
    pub types: Vec<Id>,
    /// Every statement about the resource.
    pub statements: Vec<Statement>,
}

impl Instance {
    /// Returns the values classifiers and embedding providers should read:
    /// literal attribute values plus the local names of declared types.
    #[must_use]
    pub fn attribute_texts(&self) -> Vec<(Id, String)> {
        let rdf_type =
            Id::new(format!("{}type", namespace::RDF)).expect("rdf:type is a well-known IRI");
        let mut values = Vec::new();
        for statement in &self.statements {
            let quad = &statement.quad;
            if quad.predicate == rdf_type {
                continue;
            }
            if let Term::Literal(literal) = &quad.object {
                values.push((quad.predicate.clone(), literal.value.clone()));
            }
        }
        for class in &self.types {
            let local = class
                .as_str()
                .rsplit(['#', '/'])
                .next()
                .unwrap_or_else(|| class.as_str());
            values.push((rdf_type.clone(), local.to_owned()));
        }
        values
    }
}

/// Compact instance record used by listings.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct InstanceSummary {
    /// Resource IRI.
    pub id: Id,
    /// `rdf:type` values.
    pub types: Vec<Id>,
}

/// Input for creating an instance.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NewInstance {
    /// Resource IRI.
    pub id: Id,
    /// `rdf:type` values.
    #[serde(default)]
    pub types: Vec<Id>,
    /// Predicate/object pairs.
    #[serde(default)]
    pub properties: BTreeMap<Id, Vec<Term>>,
    /// Target graph; defaults to `data:asserted`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub graph: Option<GraphName>,
    /// Optional provenance source.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source: Option<String>,
}

impl NewInstance {
    /// Starts an instance definition.
    #[must_use]
    pub fn new(id: Id) -> Self {
        Self {
            id,
            types: Vec::new(),
            properties: BTreeMap::new(),
            graph: None,
            source: None,
        }
    }

    /// Adds an `rdf:type`.
    #[must_use]
    pub fn with_type(mut self, class: Id) -> Self {
        self.types.push(class);
        self
    }

    /// Adds a property value.
    #[must_use]
    pub fn with_property(mut self, predicate: Id, value: Term) -> Self {
        self.properties.entry(predicate).or_default().push(value);
        self
    }

    /// Sets the target graph.
    #[must_use]
    pub fn with_graph(mut self, graph: GraphName) -> Self {
        self.graph = Some(graph);
        self
    }

    /// Sets the provenance source.
    #[must_use]
    pub fn with_source(mut self, source: impl Into<String>) -> Self {
        self.source = Some(source.into());
        self
    }
}

/// Partial instance update.
///
/// A property mapped to `None` is removed; `Some(values)` replaces the existing
/// values. Omitting `types` leaves the current types untouched.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct InstancePatch {
    /// Replacement types, when present.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub types: Option<Vec<Id>>,
    /// Property replacements/removals.
    #[serde(default)]
    pub properties: BTreeMap<Id, Option<Vec<Term>>>,
}

/// Filter and pagination options for instance listings.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct InstanceQuery {
    /// Restrict to a graph.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub graph: Option<GraphName>,
    /// Restrict to instances with this `rdf:type`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub type_filter: Option<Id>,
    /// Maximum rows to return.
    pub limit: usize,
    /// Rows to skip.
    pub offset: usize,
}

impl Default for InstanceQuery {
    fn default() -> Self {
        Self {
            graph: None,
            type_filter: None,
            limit: 100,
            offset: 0,
        }
    }
}

impl InstanceQuery {
    /// Restricts to a graph.
    #[must_use]
    pub fn with_graph(mut self, graph: GraphName) -> Self {
        self.graph = Some(graph);
        self
    }

    /// Restricts to a type.
    #[must_use]
    pub fn with_type(mut self, class: Id) -> Self {
        self.type_filter = Some(class);
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

/// One statement to add to an instance.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StatementInput {
    /// Predicate IRI.
    pub predicate: Id,
    /// Object term.
    pub object: Term,
}

/// Direction of a resource relationship.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum RelationshipDirection {
    /// This resource is the subject.
    Outgoing,
    /// This resource is the object.
    Incoming,
}

/// A resource relationship derived from the store.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Relationship {
    /// Whether the resource is the subject or the object.
    pub direction: RelationshipDirection,
    /// Predicate IRI.
    pub predicate: Id,
    /// The other resource involved.
    pub target: Term,
    /// Graph the statement lives in.
    pub graph: GraphName,
}

/// Escapes a lexical form for N-Triples output.
fn escape_nt(value: &str) -> String {
    let mut escaped = String::with_capacity(value.len());
    for character in value.chars() {
        match character {
            '"' => escaped.push_str("\\\""),
            '\\' => escaped.push_str("\\\\"),
            '\n' => escaped.push_str("\\n"),
            '\r' => escaped.push_str("\\r"),
            '\t' => escaped.push_str("\\t"),
            other => escaped.push(other),
        }
    }
    escaped
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::namespace::RDF_TYPE;

    fn ex(local: &str) -> Id {
        Id::new(format!("https://example.org/{local}")).unwrap()
    }

    #[test]
    fn statement_ids_are_stable_and_distinct() {
        let quad = Quad::in_graph(
            Term::Iri(ex("a")),
            ex("name"),
            Term::string("A"),
            GraphName::Iri(GraphKind::Asserted.iri()),
        );
        assert_eq!(StatementId::of(&quad), StatementId::of(&quad.clone()));

        let other = Quad::in_graph(
            Term::Iri(ex("a")),
            ex("name"),
            Term::string("B"),
            GraphName::Iri(GraphKind::Asserted.iri()),
        );
        assert_ne!(StatementId::of(&quad), StatementId::of(&other));
    }

    #[test]
    fn graph_kind_determines_statement_kind() {
        let asserted = Statement::new(Quad::in_graph(
            Term::Iri(ex("a")),
            Id::new(RDF_TYPE).unwrap(),
            Term::Iri(ex("Thing")),
            GraphName::Iri(GraphKind::Asserted.iri()),
        ));
        assert_eq!(asserted.kind, StatementKind::Asserted);

        let inferred = Statement::new(Quad::in_graph(
            Term::Iri(ex("a")),
            Id::new(RDF_TYPE).unwrap(),
            Term::Iri(ex("Thing")),
            GraphName::Iri(GraphKind::Inferred.iri()),
        ));
        assert_eq!(inferred.kind, StatementKind::Inferred);

        let derived = Statement::new(Quad::in_graph(
            Term::Iri(ex("a")),
            Id::new(RDF_TYPE).unwrap(),
            Term::Iri(ex("Thing")),
            GraphName::Iri(GraphKind::Classification.iri()),
        ));
        assert_eq!(derived.kind, StatementKind::Derived);
    }

    #[test]
    fn literal_display_is_ntriples_shaped() {
        assert_eq!(
            Literal::simple("a \"quoted\" value").to_string(),
            "\"a \\\"quoted\\\" value\""
        );
        assert_eq!(
            Literal::language_tagged("hello", "en").unwrap().to_string(),
            "\"hello\"@en"
        );
        assert_eq!(
            Literal::integer(7).to_string(),
            "\"7\"^^<http://www.w3.org/2001/XMLSchema#integer>"
        );
    }

    #[test]
    fn quad_pattern_copies_a_quad() {
        let quad = Quad::new(Term::Iri(ex("s")), ex("p"), Term::string("o"));
        let pattern = QuadPattern::new().with_quad(&quad);
        assert_eq!(pattern.subject, Some(quad.subject.clone()));
        assert_eq!(pattern.graph, Some(quad.graph.clone()));
    }
}
