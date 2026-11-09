//! Registry domain types: catalogs, assets, releases, candidates, analysis,
//! validation, and search results.
//!
//! The registry keeps four concerns separate:
//!
//! - catalog metadata (DCAT/ADMS);
//! - immutable release content (one graph per release);
//! - derived analysis (terms, statistics, imports, diffs);
//! - validation and compatibility reports (PROV-O activities and entities).
//!
//! These types are transport-neutral and serialized with the same snake_case
//! convention as the rest of the command boundary.

use std::collections::BTreeMap;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::id::Id;

/// Default page size used by registry queries.
pub const DEFAULT_PAGE_SIZE: usize = 100;

/// A DCAT catalog.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Catalog {
    /// Catalog IRI.
    pub id: Id,
    /// Human-readable title.
    pub title: String,
    /// Optional description.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// Creation time.
    pub created_at: DateTime<Utc>,
}

/// A semantic asset family (an ontology or vocabulary).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Asset {
    /// Stable family IRI.
    pub id: Id,
    /// Path-safe asset key.
    pub key: String,
    /// Owning catalog.
    pub catalog: Id,
    /// Display title.
    pub title: String,
    /// Optional description.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// Declared namespace, when known.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub namespace: Option<String>,
    /// Registration time.
    pub created_at: DateTime<Utc>,
    /// Last metadata update.
    pub updated_at: DateTime<Utc>,
}

/// A content-addressed artifact reference.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ArtifactRef {
    /// SHA-256 hex digest.
    pub sha256: String,
    /// Size in bytes.
    pub byte_size: u64,
    /// Original filename or source name.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub filename: Option<String>,
}

/// A release distribution.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Distribution {
    /// Distribution IRI.
    pub id: Id,
    /// RDF syntax.
    pub format: String,
    /// Media type.
    pub media_type: String,
    /// Artifact hash.
    pub sha256: String,
    /// Byte size.
    pub byte_size: u64,
    /// Human-facing download path or URL.
    pub url: String,
}

/// VoID-style release statistics.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReleaseStatistics {
    /// Total quads in the release graph.
    pub quads: usize,
    /// Declared classes.
    pub classes: usize,
    /// Declared object properties.
    pub object_properties: usize,
    /// Declared datatype properties.
    pub datatype_properties: usize,
    /// Declared annotation properties.
    pub annotation_properties: usize,
    /// Declared named individuals.
    pub individuals: usize,
    /// Indexed terms.
    pub terms: usize,
}

/// One import edge recorded for a release.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ImportDependency {
    /// Imported ontology IRI.
    pub iri: Id,
    /// Declared version, when present.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
    /// Registered release selected for the import, when resolved.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub resolved_release: Option<Id>,
    /// Whether a registered release was selected.
    pub resolved: bool,
}

/// One indexed ontology term.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TermSummary {
    /// Deterministic path-safe term key.
    pub key: String,
    /// Term IRI.
    pub iri: Id,
    /// `class`, `object_property`, `datatype_property`, `annotation_property`,
    /// or `individual`.
    pub kind: String,
    /// Preferred label, when present.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
    /// Definition or comment, when present.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub definition: Option<String>,
    /// Namespace of the term.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub namespace: Option<String>,
    /// Whether the term is deprecated.
    #[serde(default)]
    pub deprecated: bool,
}

/// A validation issue.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ValidationIssue {
    /// `error`, `warning`, or `info`.
    pub severity: String,
    /// Focus resource, when one can be identified.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub focus_node: Option<Id>,
    /// Property path or rule name.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
    /// Human-readable explanation.
    pub message: String,
}

/// A deterministic structural validation report.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ValidationReport {
    /// Report IRI.
    pub id: Id,
    /// Candidate or release validated.
    pub target: Id,
    /// Whether every implemented structural rule passed.
    pub conforms: bool,
    /// Findings.
    #[serde(default)]
    pub issues: Vec<ValidationIssue>,
    /// Whether an external SHACL evaluator was used.
    #[serde(default)]
    pub external_shacl: bool,
    /// When the report was produced.
    pub generated_at: DateTime<Utc>,
}

/// A release candidate that has not been published.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReleaseCandidate {
    /// Candidate IRI.
    pub id: Id,
    /// Asset family IRI.
    pub asset: Id,
    /// Release version.
    pub version: String,
    /// Version IRI declared by the source or generated from the release key.
    pub version_iri: Id,
    /// Release key derived from `asset|version`.
    pub release_key: String,
    /// Source format.
    pub format: String,
    /// Hash of the exact source bytes.
    pub source_hash: String,
    /// Informational canonical content hash.
    pub content_hash: String,
    /// `ready`, `needs_review`, or `rejected`.
    pub status: String,
    /// Title declared by the source or request.
    pub title: String,
    /// Optional source description.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// Source label or URL.
    pub source: String,
    /// Parsed statistics.
    pub statistics: ReleaseStatistics,
    /// Declared imports.
    #[serde(default)]
    pub imports: Vec<ImportDependency>,
    /// Creation time.
    pub created_at: DateTime<Utc>,
}

/// A published, immutable ontology release.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Release {
    /// Version IRI.
    pub id: Id,
    /// Release key.
    pub key: String,
    /// Asset family IRI.
    pub asset: Id,
    /// Release version.
    pub version: String,
    /// Display title.
    pub title: String,
    /// Optional description.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// Source format used at ingestion.
    pub format: String,
    /// Exact source hash.
    pub source_hash: String,
    /// Informational canonical content hash.
    pub content_hash: String,
    /// `stable`, `beta`, `experimental`, or `archived`.
    pub channel: String,
    /// Publication time.
    pub published_at: DateTime<Utc>,
    /// Release graph.
    pub graph: String,
    /// Parsed statistics.
    pub statistics: ReleaseStatistics,
    /// Declared imports.
    #[serde(default)]
    pub imports: Vec<ImportDependency>,
    /// Available distributions.
    #[serde(default)]
    pub distributions: Vec<Distribution>,
}

/// Structural comparison of two releases.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct DiffReport {
    /// Left release key.
    pub left: String,
    /// Right release key.
    pub right: String,
    /// Added class IRIs.
    #[serde(default)]
    pub added_classes: Vec<Id>,
    /// Removed class IRIs.
    #[serde(default)]
    pub removed_classes: Vec<Id>,
    /// Added property IRIs.
    #[serde(default)]
    pub added_properties: Vec<Id>,
    /// Removed property IRIs.
    #[serde(default)]
    pub removed_properties: Vec<Id>,
    /// Terms whose label or definition changed.
    #[serde(default)]
    pub changed_terms: Vec<Id>,
    /// Whether the change is structurally breaking.
    pub breaking: bool,
    /// Whether the change only adds terms/axioms.
    pub compatible: bool,
    /// Human-readable summary.
    pub summary: String,
}

/// Registry search categories.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SearchKind {
    /// Catalog, asset, and release metadata.
    Catalog,
    /// Ontology terms.
    Term,
    /// Namespace declarations.
    Namespace,
    /// Import dependencies.
    Dependency,
    /// Every indexed category.
    All,
}

impl SearchKind {
    /// Parses a user-supplied kind.
    #[must_use]
    pub fn parse(value: Option<&str>) -> Option<Self> {
        let value = value.map(str::trim).unwrap_or("all");
        match value.to_ascii_lowercase().as_str() {
            "" | "all" => Some(Self::All),
            "catalog" | "metadata" => Some(Self::Catalog),
            "term" | "terms" => Some(Self::Term),
            "namespace" | "namespaces" => Some(Self::Namespace),
            "dependency" | "dependencies" | "import" | "imports" => Some(Self::Dependency),
            _ => None,
        }
    }
}

/// One search hit.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SearchHit {
    /// Result category.
    pub kind: String,
    /// Stable key or relative identifier.
    pub key: String,
    /// Full IRI.
    pub iri: Id,
    /// Display label.
    pub label: String,
    /// Optional description.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// Release version, for release/term hits.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
}

/// Result of a registry search.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SearchResult {
    /// Hits in deterministic order.
    pub items: Vec<SearchHit>,
    /// Total hits before paging.
    pub count: usize,
    /// Requested limit.
    pub limit: usize,
    /// Requested offset.
    pub offset: usize,
}

/// Request to ingest an ontology source.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct IngestRequest {
    /// Catalog receiving the asset; defaults to the default catalog.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub catalog: Option<Id>,
    /// Source label, local path, or HTTP(S) URL.
    pub source: String,
    /// Inline document bytes, base64-free UTF-8 text.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub data: Option<String>,
    /// Explicit family IRI when the document does not declare one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub asset_iri: Option<Id>,
    /// Explicit release version.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
    /// RDF syntax name.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub format: Option<String>,
    /// Optional display title.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
}

impl IngestRequest {
    /// Creates a request for a local path or URL.
    #[must_use]
    pub fn source(source: impl Into<String>) -> Self {
        Self {
            catalog: None,
            source: source.into(),
            data: None,
            asset_iri: None,
            version: None,
            format: None,
            title: None,
        }
    }
}

/// A registry service profile.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Profile {
    /// Profile IRI.
    pub id: Id,
    /// Display title.
    pub title: String,
    /// Optional description.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// Shape artifact hash, when the profile has one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub shapes_hash: Option<String>,
    /// Creation time.
    pub created_at: DateTime<Utc>,
}

/// A registry command result with its own payload kind.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum RegistryResult {
    /// One catalog.
    Catalog(Catalog),
    /// A catalog page.
    Catalogs {
        /// Catalogs.
        items: Vec<Catalog>,
        /// Total before paging.
        count: usize,
    },
    /// One asset.
    Asset(Asset),
    /// An asset page.
    Assets {
        /// Assets.
        items: Vec<Asset>,
        /// Total before paging.
        count: usize,
    },
    /// One candidate.
    Candidate(ReleaseCandidate),
    /// A candidate page.
    Candidates {
        /// Candidates.
        items: Vec<ReleaseCandidate>,
        /// Total before paging.
        count: usize,
    },
    /// One release.
    Release(Release),
    /// A release page.
    Releases {
        /// Releases.
        items: Vec<Release>,
        /// Total before paging.
        count: usize,
    },
    /// A term page.
    Terms {
        /// Release key.
        release: String,
        /// Terms.
        items: Vec<TermSummary>,
        /// Total before paging.
        count: usize,
    },
    /// One term.
    Term(TermSummary),
    /// Import edges.
    Imports {
        /// Release key.
        release: String,
        /// Dependencies.
        items: Vec<ImportDependency>,
    },
    /// Release statistics.
    Statistics {
        /// Release key.
        release: String,
        /// Counts.
        statistics: ReleaseStatistics,
    },
    /// A structural diff.
    Diff(DiffReport),
    /// Search hits.
    Search(SearchResult),
    /// A validation report.
    Validation(ValidationReport),
    /// A profile.
    Profile(Profile),
    /// A profile page.
    Profiles {
        /// Profiles.
        items: Vec<Profile>,
        /// Total before paging.
        count: usize,
    },
    /// An archive operation.
    Archived {
        /// Release key.
        release: String,
        /// New channel.
        channel: String,
    },
    /// A report page.
    Reports {
        /// Release key.
        release: String,
        /// Reports.
        items: Vec<ValidationReport>,
    },
}

impl RegistryResult {
    /// Returns the number of top-level items in a page result.
    #[must_use]
    pub fn count(&self) -> usize {
        match self {
            Self::Catalogs { count, .. }
            | Self::Assets { count, .. }
            | Self::Candidates { count, .. }
            | Self::Releases { count, .. }
            | Self::Terms { count, .. }
            | Self::Profiles { count, .. } => *count,
            Self::Imports { items, .. } => items.len(),
            Self::Reports { items, .. } => items.len(),
            _ => 1,
        }
    }

    /// Returns counts by release channel for dashboards.
    #[must_use]
    pub fn channel_counts(releases: &[Release]) -> BTreeMap<String, usize> {
        let mut counts = BTreeMap::new();
        for release in releases {
            *counts.entry(release.channel.clone()).or_default() += 1;
        }
        counts
    }
}
