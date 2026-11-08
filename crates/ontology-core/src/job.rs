//! Background job model shared by the application service and interfaces.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::external::ImportRequest;
use crate::id::Id;
use crate::model::GraphName;
use crate::semantic::{ApplyPolicy, ClassificationOptions, InferenceOptions};

/// Lifecycle state of a background job.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum JobState {
    /// Waiting for a worker.
    Queued,
    /// Executing.
    Running,
    /// Finished successfully.
    Completed,
    /// Finished with an error.
    Failed,
    /// Cancelled before completion.
    Cancelled,
}

impl JobState {
    /// Stable storage name.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Queued => "queued",
            Self::Running => "running",
            Self::Completed => "completed",
            Self::Failed => "failed",
            Self::Cancelled => "cancelled",
        }
    }

    /// Parses a storage name.
    #[must_use]
    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "queued" => Some(Self::Queued),
            "running" => Some(Self::Running),
            "completed" => Some(Self::Completed),
            "failed" => Some(Self::Failed),
            "cancelled" => Some(Self::Cancelled),
            _ => None,
        }
    }

    /// Whether the job will make no further progress.
    #[must_use]
    pub const fn is_terminal(self) -> bool {
        matches!(self, Self::Completed | Self::Failed | Self::Cancelled)
    }
}

/// A background job request. The payload fully describes the work so a failed
/// job can be retried.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "request", rename_all = "camelCase")]
pub enum JobRequest {
    /// Classifies several instances.
    ClassifyBatch {
        /// Instance IRIs.
        instances: Vec<Id>,
        /// Classification options.
        options: ClassificationOptions,
    },
    /// Generates embeddings for several instances.
    EmbedBatch {
        /// Instance IRIs.
        instances: Vec<Id>,
        /// Attributes to embed; empty means all literals.
        #[serde(default)]
        attributes: Vec<Id>,
    },
    /// Previews and applies inference for several instances.
    InferBatch {
        /// Instance IRIs.
        instances: Vec<Id>,
        /// Inference options.
        options: InferenceOptions,
        /// Application policy.
        policy: ApplyPolicy,
    },
    /// Recomputes and replaces automatic classifications for several instances.
    ReclassifyBatch {
        /// Instance IRIs.
        instances: Vec<Id>,
        /// Classification options.
        options: ClassificationOptions,
    },
    /// Parses an RDF document into the store.
    RdfImport {
        /// RDF syntax name.
        format: String,
        /// Document text.
        data: String,
        /// Optional target graph.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        graph: Option<GraphName>,
    },
    /// Serializes the dataset or one graph.
    RdfExport {
        /// RDF syntax name.
        format: String,
        /// Restrict to a graph.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        graph: Option<GraphName>,
    },
    /// Imports an external ontology.
    ExternalImport(ImportRequest),
    /// Re-fetches and synchronizes an external ontology.
    ExternalSync {
        /// External ontology IRI.
        id: Id,
    },
}

impl JobRequest {
    /// Stable job kind name.
    #[must_use]
    pub const fn kind(&self) -> &'static str {
        match self {
            Self::ClassifyBatch { .. } => "classify_batch",
            Self::EmbedBatch { .. } => "embed_batch",
            Self::InferBatch { .. } => "infer_batch",
            Self::ReclassifyBatch { .. } => "reclassify_batch",
            Self::RdfImport { .. } => "rdf_import",
            Self::RdfExport { .. } => "rdf_export",
            Self::ExternalImport(_) => "external_import",
            Self::ExternalSync { .. } => "external_sync",
        }
    }
}

/// Current status and outcome of a job.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct JobStatus {
    /// Job identifier.
    pub id: Id,
    /// Job kind, e.g. `"classify_batch"`.
    pub kind: String,
    /// Lifecycle state.
    pub state: JobState,
    /// Completion fraction in `0.0..=1.0`.
    pub progress: f64,
    /// Units processed.
    pub processed: usize,
    /// Total units, when known.
    pub total: usize,
    /// Retry count.
    #[serde(default)]
    pub attempts: u32,
    /// Human-readable status message.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
    /// Failure detail.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    /// Serialized job result.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub result: Option<serde_json::Value>,
    /// Creation time.
    pub created_at: DateTime<Utc>,
    /// Start time.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub started_at: Option<DateTime<Utc>>,
    /// Finish time.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub finished_at: Option<DateTime<Utc>>,
}

impl JobStatus {
    /// Whether the job has finished.
    #[must_use]
    pub fn is_finished(&self) -> bool {
        self.state.is_terminal()
    }
}
