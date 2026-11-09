//! Project-management domain types and the read-only registry bridge.
//!
//! A project task is work, not an ontology release. Deliverables point at
//! registry assets/releases through [`SemanticAssetCatalog`] and never copy
//! release metadata into the project graph.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::error::Result;
use crate::id::Id;
use crate::registry::ValidationReport;

/// Read-only view of the semantic registry used by projects.
#[async_trait::async_trait]
pub trait SemanticAssetCatalog: Send + Sync {
    /// Returns one asset by IRI.
    async fn asset(&self, id: &Id) -> Result<AssetSummary>;
    /// Returns one release by version IRI or release key.
    async fn release(&self, id: &Id) -> Result<ReleaseSummary>;
    /// Resolves a release from a key or IRI.
    async fn release_by_reference(&self, value: &str) -> Result<ReleaseSummary>;
    /// Returns every release of an asset.
    async fn releases_for_asset(&self, id: &Id) -> Result<Vec<ReleaseSummary>>;
    /// Returns validation reports for a release.
    async fn validation_reports(&self, release: &Id) -> Result<Vec<ValidationReport>>;
}

/// Compact asset view used by the project layer.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AssetSummary {
    /// Family IRI.
    pub id: Id,
    /// Path-safe key.
    pub key: String,
    /// Display title.
    pub title: String,
}

/// Compact release view used by the project layer.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReleaseSummary {
    /// Version IRI.
    pub id: Id,
    /// Release key.
    pub key: String,
    /// Asset family IRI.
    pub asset: Id,
    /// Version label.
    pub version: String,
    /// Release channel.
    pub channel: String,
    /// Whether a passing structural validation report exists.
    pub validated: bool,
}

/// Project lifecycle.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProjectStatus {
    /// Work is active.
    Active,
    /// Work is temporarily paused.
    OnHold,
    /// Every deliverable is accepted.
    Completed,
    /// Historical and read-only.
    Archived,
}

impl ProjectStatus {
    /// Stable text form.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Active => "active",
            Self::OnHold => "on_hold",
            Self::Completed => "completed",
            Self::Archived => "archived",
        }
    }

    /// Parses a status.
    #[must_use]
    pub fn parse(value: &str) -> Option<Self> {
        match value.trim().to_ascii_lowercase().as_str() {
            "active" => Some(Self::Active),
            "on_hold" | "on-hold" | "paused" => Some(Self::OnHold),
            "completed" | "complete" => Some(Self::Completed),
            "archived" => Some(Self::Archived),
            _ => None,
        }
    }
}

/// Work-item kind.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WorkKind {
    /// A collection of related work.
    WorkPackage,
    /// A unit of accountable work.
    Task,
    /// A significant completion point.
    Milestone,
}

impl WorkKind {
    /// Stable text form.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::WorkPackage => "work_package",
            Self::Task => "task",
            Self::Milestone => "milestone",
        }
    }

    /// Parses a kind.
    #[must_use]
    pub fn parse(value: &str) -> Option<Self> {
        match value.trim().to_ascii_lowercase().as_str() {
            "work_package" | "work-package" | "package" => Some(Self::WorkPackage),
            "task" => Some(Self::Task),
            "milestone" => Some(Self::Milestone),
            _ => None,
        }
    }
}

/// Work-item lifecycle.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WorkStatus {
    /// Not started.
    Planned,
    /// In progress.
    Active,
    /// Waiting on an external dependency.
    Blocked,
    /// Finished.
    Done,
    /// Deliberately stopped.
    Cancelled,
}

impl WorkStatus {
    /// Stable text form.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Planned => "planned",
            Self::Active => "active",
            Self::Blocked => "blocked",
            Self::Done => "done",
            Self::Cancelled => "cancelled",
        }
    }

    /// Parses a status.
    #[must_use]
    pub fn parse(value: &str) -> Option<Self> {
        match value.trim().to_ascii_lowercase().as_str() {
            "planned" | "todo" => Some(Self::Planned),
            "active" | "in_progress" | "in-progress" => Some(Self::Active),
            "blocked" => Some(Self::Blocked),
            "done" | "completed" | "complete" => Some(Self::Done),
            "cancelled" | "canceled" => Some(Self::Cancelled),
            _ => None,
        }
    }

    /// Returns whether a transition is allowed.
    #[must_use]
    pub fn can_transition_to(self, next: Self) -> bool {
        matches!(
            (self, next),
            (
                Self::Planned,
                Self::Active | Self::Blocked | Self::Cancelled
            ) | (Self::Active, Self::Blocked | Self::Done | Self::Cancelled)
                | (Self::Blocked, Self::Active | Self::Cancelled)
        )
    }
}

/// A requested work-item transition.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TaskEvent {
    /// Start planned or blocked work.
    Start,
    /// Mark work blocked.
    Block,
    /// Return blocked work to active.
    Unblock,
    /// Complete active work.
    Complete,
    /// Cancel planned, active, or blocked work.
    Cancel,
}

impl TaskEvent {
    /// Returns the status this event requests.
    #[must_use]
    pub const fn target(self) -> WorkStatus {
        match self {
            Self::Start | Self::Unblock => WorkStatus::Active,
            Self::Block => WorkStatus::Blocked,
            Self::Complete => WorkStatus::Done,
            Self::Cancel => WorkStatus::Cancelled,
        }
    }

    /// Parses a stable event name.
    #[must_use]
    pub fn parse(value: &str) -> Option<Self> {
        match value.trim().to_ascii_lowercase().as_str() {
            "start" => Some(Self::Start),
            "block" => Some(Self::Block),
            "unblock" => Some(Self::Unblock),
            "complete" | "done" => Some(Self::Complete),
            "cancel" => Some(Self::Cancel),
            _ => None,
        }
    }
}

/// Deliverable lifecycle.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DeliverableStatus {
    /// Planned but not started.
    Planned,
    /// Being produced.
    InProgress,
    /// Ready for acceptance.
    Ready,
    /// Accepted against evidence.
    Accepted,
    /// Rejected during acceptance.
    Rejected,
}

impl DeliverableStatus {
    /// Stable text form.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Planned => "planned",
            Self::InProgress => "in_progress",
            Self::Ready => "ready",
            Self::Accepted => "accepted",
            Self::Rejected => "rejected",
        }
    }
}

/// A project.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Project {
    /// Project IRI.
    pub id: Id,
    /// Display title.
    pub title: String,
    /// Optional description.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// Lifecycle status.
    pub status: String,
    /// Optional owner.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub owner: Option<String>,
    /// Creation time.
    pub created_at: DateTime<Utc>,
    /// Last update.
    pub updated_at: DateTime<Utc>,
}

/// Input for creating a project.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NewProject {
    /// Display title.
    pub title: String,
    /// Optional description.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// Optional owner.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub owner: Option<String>,
}

/// A goal.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Goal {
    /// Goal IRI.
    pub id: Id,
    /// Owning project.
    pub project: Id,
    /// Title.
    pub title: String,
    /// Optional description.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// Status.
    pub status: String,
    /// Creation time.
    pub created_at: DateTime<Utc>,
}

/// A requirement.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Requirement {
    /// Requirement IRI.
    pub id: Id,
    /// Owning project.
    pub project: Id,
    /// Title.
    pub title: String,
    /// Optional description.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// Requirement kind (`capability`, `constraint`, `profile`, `data`).
    #[serde(rename = "requirement_kind")]
    pub kind: String,
    /// Status.
    pub status: String,
    /// Creation time.
    pub created_at: DateTime<Utc>,
}

/// A work package, task, or milestone.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorkItem {
    /// Work item IRI.
    pub id: Id,
    /// Owning project.
    pub project: Id,
    /// Parent work item, when nested.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parent: Option<Id>,
    /// Kind.
    #[serde(rename = "work_kind")]
    pub kind: String,
    /// Title.
    pub title: String,
    /// Optional description.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// Status.
    pub status: String,
    /// Optional assignee.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub assignee: Option<String>,
    /// Optional due date.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub due: Option<DateTime<Utc>>,
    /// Completion percentage in `0..=100`.
    pub progress: u8,
    /// Dependency IRIs.
    #[serde(default)]
    pub depends_on: Vec<Id>,
    /// Creation time.
    pub created_at: DateTime<Utc>,
}

/// A deliverable bridge to registry content.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Deliverable {
    /// Deliverable IRI.
    pub id: Id,
    /// Owning project.
    pub project: Id,
    /// Title.
    pub title: String,
    /// Deliverable kinds.
    #[serde(default)]
    pub kinds: Vec<String>,
    /// Target asset, when produced for a family.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub asset: Option<Id>,
    /// Target release, when produced against a published version.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub release: Option<Id>,
    /// Status.
    pub status: String,
    /// Evidence labels or report IRIs.
    #[serde(default)]
    pub evidence: Vec<String>,
    /// Acceptance criteria.
    #[serde(default)]
    pub acceptance_criteria: Vec<String>,
    /// Acceptance time.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub accepted_at: Option<DateTime<Utc>>,
    /// Creation time.
    pub created_at: DateTime<Utc>,
}

/// A periodic project snapshot.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProjectSnapshot {
    /// Snapshot IRI.
    pub id: Id,
    /// Project IRI.
    pub project: Id,
    /// Creation time.
    pub created_at: DateTime<Utc>,
    /// Exact release IRIs adopted by the project.
    #[serde(default)]
    pub releases: Vec<Id>,
    /// Work statuses at snapshot time.
    #[serde(default)]
    pub work_statuses: std::collections::BTreeMap<String, String>,
    /// Accepted deliverable IRIs.
    #[serde(default)]
    pub accepted_deliverables: Vec<Id>,
}

/// Project dashboard.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ProjectOverview {
    /// Project.
    pub project: Project,
    /// Goal count.
    pub goals: usize,
    /// Requirement count.
    pub requirements: usize,
    /// Work-item count.
    pub work_items: usize,
    /// Deliverable count.
    pub deliverables: usize,
    /// Accepted deliverable count.
    pub accepted_deliverables: usize,
    /// Weighted completion percentage.
    pub progress: f64,
    /// Work-item counts by status.
    #[serde(default)]
    pub work_by_status: std::collections::BTreeMap<String, usize>,
    /// Releases referenced by deliverables.
    #[serde(default)]
    pub releases: Vec<Id>,
}

/// A timeline entry derived from work items.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TimelineEntry {
    /// Work item.
    pub id: Id,
    /// Title.
    pub title: String,
    /// Kind.
    pub kind: String,
    /// Status.
    pub status: String,
    /// Due date.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub due: Option<DateTime<Utc>>,
}

/// A dependency edge.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DependencyEdge {
    /// Dependent item.
    pub from: Id,
    /// Prerequisite.
    pub to: Id,
}

/// A project command result.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum ProjectResult {
    /// One project.
    Project(Project),
    /// A project page.
    Projects {
        /// Projects.
        items: Vec<Project>,
        /// Total before paging.
        count: usize,
    },
    /// One goal.
    Goal(Goal),
    /// Goals.
    Goals {
        /// Goals.
        items: Vec<Goal>,
        /// Total.
        count: usize,
    },
    /// One requirement.
    Requirement(Requirement),
    /// Requirements.
    Requirements {
        /// Requirements.
        items: Vec<Requirement>,
        /// Total.
        count: usize,
    },
    /// One work item.
    Work(WorkItem),
    /// Work items.
    WorkItems {
        /// Work items.
        items: Vec<WorkItem>,
        /// Total.
        count: usize,
    },
    /// One deliverable.
    Deliverable(Deliverable),
    /// Deliverables.
    Deliverables {
        /// Deliverables.
        items: Vec<Deliverable>,
        /// Total.
        count: usize,
    },
    /// One snapshot.
    Snapshot(ProjectSnapshot),
    /// A project overview.
    Overview(ProjectOverview),
    /// A timeline.
    Timeline {
        /// Entries.
        items: Vec<TimelineEntry>,
    },
    /// Dependency edges.
    Dependencies {
        /// Edges.
        items: Vec<DependencyEdge>,
    },
}

impl ProjectResult {
    /// Number of top-level items.
    #[must_use]
    pub fn count(&self) -> usize {
        match self {
            Self::Projects { count, .. }
            | Self::Goals { count, .. }
            | Self::Requirements { count, .. }
            | Self::WorkItems { count, .. }
            | Self::Deliverables { count, .. } => *count,
            Self::Timeline { items } => items.len(),
            Self::Dependencies { items } => items.len(),
            _ => 1,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn work_status_transitions_are_bounded() {
        assert!(WorkStatus::Planned.can_transition_to(WorkStatus::Active));
        assert!(WorkStatus::Active.can_transition_to(WorkStatus::Done));
        assert!(!WorkStatus::Planned.can_transition_to(WorkStatus::Done));
        assert!(!WorkStatus::Done.can_transition_to(WorkStatus::Active));
    }

    #[test]
    fn statuses_parse_stable_names() {
        assert_eq!(ProjectStatus::parse("on_hold"), Some(ProjectStatus::OnHold));
        assert_eq!(WorkKind::parse("work-package"), Some(WorkKind::WorkPackage));
        assert_eq!(WorkStatus::parse("in_progress"), Some(WorkStatus::Active));
    }
}
