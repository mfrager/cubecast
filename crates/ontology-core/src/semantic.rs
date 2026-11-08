//! Semantic engine domain types: classification strategies and results,
//! embedding providers and vector search, inference and enrichment, and the
//! explanations that every derived result carries.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::error::Result;
use crate::id::Id;
use crate::model::{GraphName, Instance, Quad, Term};

/// A classification strategy.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ClassificationStrategy {
    /// Exact, case-sensitive label match.
    Exact,
    /// Case, whitespace, and punctuation normalized match.
    Normalized,
    /// Token and keyword overlap.
    Token,
    /// Approximate string similarity.
    Fuzzy,
    /// Configurable attribute rules.
    Rules,
    /// Embedding cosine similarity.
    Semantic,
    /// Weighted combination of the available strategies.
    Hybrid,
    /// Manually recorded classification.
    Manual,
}

impl ClassificationStrategy {
    /// Every strategy.
    pub const ALL: [Self; 8] = [
        Self::Exact,
        Self::Normalized,
        Self::Token,
        Self::Fuzzy,
        Self::Rules,
        Self::Semantic,
        Self::Hybrid,
        Self::Manual,
    ];

    /// Stable storage name.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Exact => "exact",
            Self::Normalized => "normalized",
            Self::Token => "token",
            Self::Fuzzy => "fuzzy",
            Self::Rules => "rules",
            Self::Semantic => "semantic",
            Self::Hybrid => "hybrid",
            Self::Manual => "manual",
        }
    }

    /// Parses a storage name.
    #[must_use]
    pub fn parse(value: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|item| item.as_str() == value)
    }

    /// Weight used when combining strategies in hybrid mode.
    #[must_use]
    pub const fn weight(self) -> f64 {
        match self {
            Self::Exact => 1.0,
            Self::Normalized => 0.95,
            Self::Token => 0.8,
            Self::Fuzzy => 0.7,
            Self::Semantic => 0.9,
            Self::Rules => 1.0,
            Self::Hybrid | Self::Manual => 0.0,
        }
    }
}

/// An attribute-to-label match that produced a classification.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AttributeMatch {
    /// Instance attribute whose value matched.
    pub attribute: Id,
    /// Attribute value that matched.
    pub value: String,
    /// Concept label that matched.
    pub label: String,
}

/// Review state of a classification.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ReviewStatus {
    /// Awaiting review.
    Pending,
    /// Accepted.
    Approved,
    /// Rejected.
    Rejected,
}

impl ReviewStatus {
    /// Stable storage name.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Pending => "pending",
            Self::Approved => "approved",
            Self::Rejected => "rejected",
        }
    }

    /// Parses a storage name.
    #[must_use]
    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "pending" => Some(Self::Pending),
            "approved" => Some(Self::Approved),
            "rejected" => Some(Self::Rejected),
            _ => None,
        }
    }
}

/// A rejected or below-threshold candidate.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Rejection {
    /// Candidate concept.
    pub concept: Id,
    /// Score that was computed.
    pub score: f64,
    /// Why the candidate was not accepted.
    pub reason: String,
}

/// Structured explanation attached to derived results.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Explanation {
    /// Short human-readable summary.
    pub summary: String,
    /// Strategies that contributed.
    #[serde(default)]
    pub strategies: Vec<ClassificationStrategy>,
    /// Attribute values that matched.
    #[serde(default)]
    pub matched: Vec<AttributeMatch>,
    /// Rules that fired.
    #[serde(default)]
    pub rules: Vec<String>,
    /// Applicable or inherited properties considered.
    #[serde(default)]
    pub inherited_properties: Vec<Id>,
    /// Mapping records used.
    #[serde(default)]
    pub mappings: Vec<Id>,
    /// Validation notes.
    #[serde(default)]
    pub validation: Vec<String>,
    /// Rejection reasons.
    #[serde(default)]
    pub rejections: Vec<Rejection>,
}

/// Configuration for one classification run.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ClassificationOptions {
    /// Restrict to one scheme; by default every scheme is searched.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub scheme: Option<Id>,
    /// Strategies to run; empty means [`ClassificationStrategy::Hybrid`].
    #[serde(default)]
    pub strategies: Vec<ClassificationStrategy>,
    /// Minimum confidence to report.
    pub threshold: f64,
    /// Maximum classifications per instance.
    pub max_results: usize,
    /// Restrict to these attributes; empty means all.
    #[serde(default)]
    pub attributes: Vec<Id>,
    /// Preferred label language.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub language: Option<String>,
    /// Also report ancestor concepts of accepted matches.
    #[serde(default)]
    pub include_ancestors: bool,
    /// Store the results as RDF.
    #[serde(default = "default_true")]
    pub persist: bool,
    /// Graph holding the instance statements.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub graph: Option<GraphName>,
}

const fn default_true() -> bool {
    true
}

impl Default for ClassificationOptions {
    fn default() -> Self {
        Self {
            scheme: None,
            strategies: Vec::new(),
            threshold: 0.5,
            max_results: 5,
            attributes: Vec::new(),
            language: None,
            include_ancestors: false,
            persist: true,
            graph: None,
        }
    }
}

impl ClassificationOptions {
    /// Creates default options.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Restricts to one scheme.
    #[must_use]
    pub fn with_scheme(mut self, scheme: Id) -> Self {
        self.scheme = Some(scheme);
        self
    }

    /// Selects strategies.
    #[must_use]
    pub fn with_strategies(mut self, strategies: Vec<ClassificationStrategy>) -> Self {
        self.strategies = strategies;
        self
    }

    /// Sets the confidence threshold.
    #[must_use]
    pub fn with_threshold(mut self, threshold: f64) -> Self {
        self.threshold = threshold;
        self
    }

    /// Sets the maximum number of results.
    #[must_use]
    pub fn with_max_results(mut self, max_results: usize) -> Self {
        self.max_results = max_results;
        self
    }

    /// Restricts to these attributes.
    #[must_use]
    pub fn with_attributes(mut self, attributes: Vec<Id>) -> Self {
        self.attributes = attributes;
        self
    }

    /// Sets the preferred label language.
    #[must_use]
    pub fn with_language(mut self, language: impl Into<String>) -> Self {
        self.language = Some(language.into());
        self
    }

    /// Enables ancestor expansion.
    #[must_use]
    pub fn with_ancestors(mut self) -> Self {
        self.include_ancestors = true;
        self
    }

    /// Controls persistence.
    #[must_use]
    pub fn persisting(mut self, persist: bool) -> Self {
        self.persist = persist;
        self
    }

    /// Reads the instance from a specific graph.
    #[must_use]
    pub fn with_graph(mut self, graph: GraphName) -> Self {
        self.graph = Some(graph);
        self
    }
}

/// One concept assigned to an instance.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Classification {
    /// Classification record IRI.
    pub id: Id,
    /// Classified instance.
    pub instance: Id,
    /// Assigned concept.
    pub concept: Id,
    /// Concept scheme of the concept.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub scheme: Option<Id>,
    /// Confidence in `0.0..=1.0`.
    pub confidence: f64,
    /// Strategy that produced the result.
    pub strategy: ClassificationStrategy,
    /// Attribute values that matched.
    #[serde(default)]
    pub matched_attributes: Vec<AttributeMatch>,
    /// Version of the taxonomy used.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub taxonomy_version: Option<String>,
    /// Whether this is a manual override.
    #[serde(default)]
    pub manual: bool,
    /// Review state.
    pub status: ReviewStatus,
    /// When the classification was produced.
    pub classified_at: DateTime<Utc>,
    /// Explanation details.
    #[serde(default)]
    pub explanation: Explanation,
}

/// Batch classification result.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ClassificationBatch {
    /// Number of instances classified.
    pub instance_count: usize,
    /// Accepted classifications.
    pub classifications: Vec<Classification>,
    /// Below-threshold candidates.
    #[serde(default)]
    pub rejected: Vec<Rejection>,
}

/// A configurable attribute rule.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ClassificationRule {
    /// Rule name used in explanations.
    pub name: String,
    /// Attribute the rule inspects.
    pub attribute: Id,
    /// Comparison operator.
    pub operator: RuleOperator,
    /// Value to compare against.
    pub value: String,
    /// Concept assigned when the rule fires.
    pub concept: Id,
    /// Confidence assigned when the rule fires.
    pub confidence: f64,
}

impl ClassificationRule {
    /// Creates a rule.
    #[must_use]
    pub fn new(
        name: impl Into<String>,
        attribute: Id,
        operator: RuleOperator,
        value: impl Into<String>,
        concept: Id,
        confidence: f64,
    ) -> Self {
        Self {
            name: name.into(),
            attribute,
            operator,
            value: value.into(),
            concept,
            confidence,
        }
    }
}

/// Rule comparison operators.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum RuleOperator {
    /// Case-insensitive equality.
    Equals,
    /// Case-insensitive substring.
    Contains,
    /// Case-insensitive prefix.
    Prefix,
}

/// Embedding vector returned by a provider.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Embedding {
    /// Vector values.
    pub vector: Vec<f32>,
}

impl Embedding {
    /// Wraps raw vector values.
    #[must_use]
    pub fn new(vector: Vec<f32>) -> Self {
        Self { vector }
    }
}

/// Pluggable embedding provider.
#[async_trait::async_trait]
pub trait EmbeddingProvider: Send + Sync {
    /// Embeds a batch of texts.
    async fn embed(&self, texts: &[String]) -> Result<Vec<Embedding>>;
    /// Vector dimensions.
    fn dimensions(&self) -> usize;
    /// Model identifier.
    fn model_id(&self) -> &str;
    /// Model version.
    fn model_version(&self) -> &str {
        "1"
    }
}

/// A stored vector record.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct VectorRecord {
    /// Record IRI.
    pub id: Id,
    /// Source instance.
    pub instance: Id,
    /// Source attribute; `None` for the aggregate instance embedding.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub attribute: Option<Id>,
    /// Model identifier.
    pub model_id: String,
    /// Vector values.
    pub vector: Vec<f32>,
    /// Hash of the source text.
    pub text_hash: String,
    /// When the vector was generated.
    pub generated_at: DateTime<Utc>,
}

/// One vector search hit.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct VectorMatch {
    /// Record IRI.
    pub id: Id,
    /// Source instance.
    pub instance: Id,
    /// Source attribute.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub attribute: Option<Id>,
    /// Model identifier.
    pub model_id: String,
    /// Similarity score, higher is closer.
    pub score: f64,
}

/// Metadata filter for vector search.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct VectorFilter {
    /// Restrict to one instance.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub instance: Option<Id>,
    /// Restrict to one attribute.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub attribute: Option<Id>,
    /// Restrict to one model.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model_id: Option<String>,
}

/// Similarity metric used by a vector index.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum SimilarityMetric {
    /// Cosine similarity.
    Cosine,
    /// Raw dot product.
    Dot,
    /// Inverse Euclidean distance (`1 / (1 + d)`).
    Euclidean,
}

impl Default for SimilarityMetric {
    fn default() -> Self {
        Self::Cosine
    }
}

/// Pluggable vector index.
#[async_trait::async_trait]
pub trait VectorIndex: Send + Sync {
    /// Inserts or replaces records.
    async fn upsert(&self, records: &[VectorRecord]) -> Result<()>;
    /// Searches by vector.
    async fn search(
        &self,
        vector: &[f32],
        limit: usize,
        filter: Option<VectorFilter>,
        metric: SimilarityMetric,
    ) -> Result<Vec<VectorMatch>>;
    /// Deletes records by identifier.
    async fn delete(&self, ids: &[Id]) -> Result<usize>;
}

/// Embedding metadata and, when requested, the vector itself.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EmbeddingRecord {
    /// Record IRI.
    pub id: Id,
    /// Source instance.
    pub instance: Id,
    /// Source attribute; `None` for the aggregate embedding.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub attribute: Option<Id>,
    /// Model identifier.
    pub model_id: String,
    /// Model version.
    pub model_version: String,
    /// Vector dimensions.
    pub dimensions: usize,
    /// Hash of the source text.
    pub text_hash: String,
    /// When the vector was generated.
    pub generated_at: DateTime<Utc>,
    /// Vector values, empty when only metadata was requested.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub vector: Vec<f32>,
}

/// Classifier capability.
#[async_trait::async_trait]
pub trait Classifier: Send + Sync {
    /// Classifies one instance.
    async fn classify(
        &self,
        instance: &Instance,
        options: ClassificationOptions,
    ) -> Result<Vec<Classification>>;
    /// Classifies a batch of instances.
    async fn classify_batch(
        &self,
        instances: &[Instance],
        options: ClassificationOptions,
    ) -> Result<ClassificationBatch>;
}

/// How an inference was derived.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum InferenceMethod {
    /// OWL subclass entailment.
    Subclass,
    /// OWL subproperty entailment.
    Subproperty,
    /// Domain-based type inference.
    Domain,
    /// Range-based type inference.
    Range,
    /// External mapping application.
    Mapping,
    /// Classification-driven enrichment.
    Classification,
    /// Custom attribute rule.
    Rule,
}

impl InferenceMethod {
    /// Stable storage name.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Subclass => "subclass",
            Self::Subproperty => "subproperty",
            Self::Domain => "domain",
            Self::Range => "range",
            Self::Mapping => "mapping",
            Self::Classification => "classification",
            Self::Rule => "rule",
        }
    }

    /// Whether the method is a formal OWL entailment.
    #[must_use]
    pub const fn is_formal(self) -> bool {
        matches!(
            self,
            Self::Subclass | Self::Subproperty | Self::Domain | Self::Range
        )
    }
}

/// One inferred class assignment.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct InferredClass {
    /// Inferred class.
    pub class: Id,
    /// How it was derived.
    pub method: InferenceMethod,
    /// Confidence in `0.0..=1.0`.
    pub confidence: f64,
    /// Human-readable reason.
    pub reason: String,
}

/// One inferred property value.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct InferredProperty {
    /// Inferred property.
    pub property: Id,
    /// Inferred value.
    pub value: Term,
    /// How it was derived.
    pub method: InferenceMethod,
    /// Confidence in `0.0..=1.0`.
    pub confidence: f64,
    /// Human-readable reason.
    pub reason: String,
}

/// A proposed statement with its derivation.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct InferredStatement {
    /// Proposed quad in the target graph.
    pub quad: Quad,
    /// How it was derived.
    pub method: InferenceMethod,
    /// Confidence in `0.0..=1.0`.
    pub confidence: f64,
    /// Human-readable reason.
    pub reason: String,
}

impl InferredStatement {
    /// Whether the statement is a formal entailment rather than a heuristic.
    #[must_use]
    pub fn is_formal(&self) -> bool {
        self.method.is_formal()
    }
}

/// Configuration for inference and enrichment.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct InferenceOptions {
    /// Apply external class and property mappings.
    pub include_external: bool,
    /// Apply classification results.
    pub include_classification: bool,
    /// Report applicable and inherited properties.
    pub include_inherited: bool,
    /// Attribute rules to evaluate.
    #[serde(default)]
    pub rules: Vec<ClassificationRule>,
    /// Graph holding the instance.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub graph: Option<GraphName>,
    /// Graph receiving applied statements.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub target_graph: Option<GraphName>,
}

impl Default for InferenceOptions {
    fn default() -> Self {
        Self {
            include_external: true,
            include_classification: true,
            include_inherited: true,
            rules: Vec::new(),
            graph: None,
            target_graph: None,
        }
    }
}

impl InferenceOptions {
    /// Creates default options.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Enables or disables external mappings.
    #[must_use]
    pub fn with_external(mut self, include_external: bool) -> Self {
        self.include_external = include_external;
        self
    }

    /// Enables or disables classification enrichment.
    #[must_use]
    pub fn with_classification(mut self, include_classification: bool) -> Self {
        self.include_classification = include_classification;
        self
    }

    /// Adds an attribute rule.
    #[must_use]
    pub fn with_rule(mut self, rule: ClassificationRule) -> Self {
        self.rules.push(rule);
        self
    }

    /// Sets the target graph.
    #[must_use]
    pub fn with_target_graph(mut self, graph: GraphName) -> Self {
        self.target_graph = Some(graph);
        self
    }
}

/// Inference preview before any statement is written.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct InferencePreview {
    /// Analyzed instance.
    pub instance: Id,
    /// Inferred classes.
    #[serde(default)]
    pub classes: Vec<InferredClass>,
    /// Inferred properties.
    #[serde(default)]
    pub properties: Vec<InferredProperty>,
    /// Proposed statements.
    #[serde(default)]
    pub statements: Vec<InferredStatement>,
    /// Properties applicable to the instance's classes.
    #[serde(default)]
    pub applicable_properties: Vec<Id>,
    /// Classes whose restrictions reference the instance's properties.
    #[serde(default)]
    pub applicable_classes: Vec<Id>,
    /// Explanation details.
    #[serde(default)]
    pub explanation: Explanation,
    /// Statements that were rejected.
    #[serde(default)]
    pub rejections: Vec<Rejection>,
}

/// Application policy for inferred statements.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ApplyPolicy {
    /// Recommendations only.
    Suggest,
    /// Requires approval before application.
    Review,
    /// Applies statements at or above the threshold.
    Automatic {
        /// Minimum confidence.
        threshold: f64,
    },
}

impl ApplyPolicy {
    /// Stable storage name.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Suggest => "suggest",
            Self::Review => "review",
            Self::Automatic { .. } => "automatic",
        }
    }
}

/// Outcome of an inference application.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct InferenceReport {
    /// Analyzed instance.
    pub instance: Id,
    /// Policy that governed application.
    pub policy: ApplyPolicy,
    /// Full preview.
    pub preview: InferencePreview,
    /// Whether statements were written.
    pub applied: bool,
    /// Whether human approval is required.
    pub requires_approval: bool,
    /// Statements written.
    pub applied_count: usize,
    /// Statements skipped.
    pub skipped: usize,
}

/// Inference capability.
#[async_trait::async_trait]
pub trait InferenceEngine: Send + Sync {
    /// Infers classes for an instance.
    async fn infer_classes(
        &self,
        instance: &Id,
        options: InferenceOptions,
    ) -> Result<Vec<InferredClass>>;
    /// Infers property values for an instance.
    async fn infer_properties(
        &self,
        instance: &Id,
        options: InferenceOptions,
    ) -> Result<Vec<InferredProperty>>;
    /// Builds a full preview.
    async fn preview(&self, instance: &Id, options: InferenceOptions) -> Result<InferencePreview>;
    /// Applies results according to a policy.
    async fn apply(
        &self,
        instance: &Id,
        policy: ApplyPolicy,
        options: InferenceOptions,
    ) -> Result<InferenceReport>;
}
