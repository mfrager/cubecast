//! Project management service over project graphs and the registry bridge.

use std::collections::BTreeMap;
use std::sync::Arc;

use chrono::Utc;
use ontology_core::command::ProjectCommand;
use ontology_core::config::ProjectConfig;
use ontology_core::error::{Error, Result};
use ontology_core::id::Id;
use ontology_core::project::{
    Deliverable, Goal, NewProject, Project, ProjectOverview, ProjectResult, ProjectSnapshot,
    ProjectStatus, Requirement, SemanticAssetCatalog, TaskEvent, WorkItem, WorkKind, WorkStatus,
};
use ontology_core::rdf::RdfStore;

use crate::repository::ProjectRepository;

/// Project service.
pub struct ProjectService {
    repository: ProjectRepository,
    catalog: Arc<dyn SemanticAssetCatalog>,
}

impl ProjectService {
    /// Creates a service.
    #[must_use]
    pub fn new(
        store: Arc<dyn RdfStore>,
        config: ProjectConfig,
        catalog: Arc<dyn SemanticAssetCatalog>,
    ) -> Self {
        Self {
            repository: ProjectRepository::new(store, config.base_iri),
            catalog,
        }
    }

    /// Returns the repository.
    #[must_use]
    pub fn repository(&self) -> &ProjectRepository {
        &self.repository
    }

    /// Executes one project command.
    pub async fn execute(&self, command: &ProjectCommand) -> Result<ProjectResult> {
        match command {
            ProjectCommand::Create(request) => self.create(request).map(ProjectResult::Project),
            ProjectCommand::Get { project } => self
                .repository
                .get_project(project)
                .map(ProjectResult::Project),
            ProjectCommand::List => {
                let items = self.repository.list_projects()?;
                let count = items.len();
                Ok(ProjectResult::Projects { items, count })
            }
            ProjectCommand::Update {
                project,
                title,
                description,
                owner,
            } => self
                .repository
                .update_project(
                    project,
                    title.as_deref(),
                    description.as_deref(),
                    owner.as_deref(),
                )
                .map(ProjectResult::Project),
            ProjectCommand::AddGoal {
                project,
                title,
                description,
            } => self
                .add_goal(project, title, description.as_deref())
                .map(ProjectResult::Goal),
            ProjectCommand::ListGoals { project } => self
                .repository
                .get_project(project)
                .and_then(|_| self.repository.list_goals(project))
                .map(|items| {
                    let count = items.len();
                    ProjectResult::Goals { items, count }
                }),
            ProjectCommand::AddRequirement {
                project,
                title,
                description,
                kind,
            } => self
                .add_requirement(project, title, description.as_deref(), kind.as_deref())
                .map(ProjectResult::Requirement),
            ProjectCommand::ListRequirements { project } => self
                .repository
                .get_project(project)
                .and_then(|_| self.repository.list_requirements(project))
                .map(|items| {
                    let count = items.len();
                    ProjectResult::Requirements { items, count }
                }),
            ProjectCommand::AddWork {
                project,
                parent,
                kind,
                title,
                description,
                assignee,
                due,
            } => self
                .add_work(
                    project,
                    parent.as_ref(),
                    *kind,
                    title,
                    description.as_deref(),
                    assignee.as_deref(),
                    *due,
                )
                .map(ProjectResult::Work),
            ProjectCommand::ListWork { project } => self
                .repository
                .get_project(project)
                .and_then(|_| self.repository.list_work(project))
                .map(|items| {
                    let count = items.len();
                    ProjectResult::WorkItems { items, count }
                }),
            ProjectCommand::Transition { work, event } => {
                self.transition(work, *event).map(ProjectResult::Work)
            }
            ProjectCommand::Assign { work, assignee } => self
                .repository
                .set_assignee(work, assignee)
                .map(ProjectResult::Work),
            ProjectCommand::AddDeliverable {
                project,
                title,
                kinds,
                asset,
                release,
                acceptance_criteria,
            } => self
                .add_deliverable(
                    project,
                    title,
                    kinds,
                    asset.as_ref(),
                    release.as_ref(),
                    acceptance_criteria,
                )
                .map(ProjectResult::Deliverable),
            ProjectCommand::ListDeliverables { project } => self
                .repository
                .get_project(project)
                .and_then(|_| self.repository.list_deliverables(project))
                .map(|items| {
                    let count = items.len();
                    ProjectResult::Deliverables { items, count }
                }),
            ProjectCommand::LinkDeliverable {
                deliverable,
                release,
            } => self.link_deliverable(deliverable, release).await,
            ProjectCommand::Accept {
                deliverable,
                evidence,
                criterion,
            } => self
                .accept(deliverable, evidence.as_deref(), criterion.as_deref())
                .await
                .map(ProjectResult::Deliverable),
            ProjectCommand::Snapshot { project } => {
                self.snapshot(project).map(ProjectResult::Snapshot)
            }
            ProjectCommand::Overview { project } => {
                self.overview(project).map(ProjectResult::Overview)
            }
            ProjectCommand::Timeline { project } => self
                .repository
                .get_project(project)
                .and_then(|_| self.repository.timeline(project))
                .map(|items| ProjectResult::Timeline { items }),
            ProjectCommand::Dependencies { project } => self
                .repository
                .get_project(project)
                .and_then(|_| self.repository.dependencies(project))
                .map(|items| ProjectResult::Dependencies { items }),
        }
    }

    /// Creates a project.
    pub fn create(&self, request: &NewProject) -> Result<Project> {
        if request.title.trim().is_empty() {
            return Err(Error::Project {
                stage: "create",
                detail: "project title must not be empty".to_owned(),
            });
        }
        let now = Utc::now();
        let project = Project {
            id: self.repository.generate("project")?,
            title: request.title.trim().to_owned(),
            description: request.description.clone(),
            status: ProjectStatus::Active.as_str().to_owned(),
            owner: request.owner.clone(),
            created_at: now,
            updated_at: now,
        };
        self.repository.save_project(&project)?;
        Ok(project)
    }

    /// Adds a goal.
    pub fn add_goal(&self, project: &Id, title: &str, description: Option<&str>) -> Result<Goal> {
        self.repository.get_project(project)?;
        if title.trim().is_empty() {
            return Err(Error::Project {
                stage: "add goal",
                detail: "goal title must not be empty".to_owned(),
            });
        }
        let goal = Goal {
            id: self.repository.generate("goal")?,
            project: project.clone(),
            title: title.trim().to_owned(),
            description: description.map(ToOwned::to_owned),
            status: "planned".to_owned(),
            created_at: Utc::now(),
        };
        self.repository.save_goal(&goal)?;
        Ok(goal)
    }

    /// Adds a requirement.
    pub fn add_requirement(
        &self,
        project: &Id,
        title: &str,
        description: Option<&str>,
        kind: Option<&str>,
    ) -> Result<Requirement> {
        self.repository.get_project(project)?;
        if title.trim().is_empty() {
            return Err(Error::Project {
                stage: "add requirement",
                detail: "requirement title must not be empty".to_owned(),
            });
        }
        let kind = kind.unwrap_or("capability").trim().to_ascii_lowercase();
        if !matches!(
            kind.as_str(),
            "capability" | "constraint" | "profile" | "data"
        ) {
            return Err(Error::Project {
                stage: "add requirement",
                detail: "requirement kind must be capability, constraint, profile, or data"
                    .to_owned(),
            });
        }
        let requirement = Requirement {
            id: self.repository.generate("requirement")?,
            project: project.clone(),
            title: title.trim().to_owned(),
            description: description.map(ToOwned::to_owned),
            kind,
            status: "planned".to_owned(),
            created_at: Utc::now(),
        };
        self.repository.save_requirement(&requirement)?;
        Ok(requirement)
    }

    /// Adds a work item.
    #[allow(clippy::too_many_arguments)]
    pub fn add_work(
        &self,
        project: &Id,
        parent: Option<&Id>,
        kind: WorkKind,
        title: &str,
        description: Option<&str>,
        assignee: Option<&str>,
        due: Option<chrono::DateTime<Utc>>,
    ) -> Result<WorkItem> {
        self.repository.get_project(project)?;
        if title.trim().is_empty() {
            return Err(Error::Project {
                stage: "add work",
                detail: "work title must not be empty".to_owned(),
            });
        }
        if let Some(parent) = parent {
            let parent_work = self.repository.get_work(parent)?;
            if &parent_work.project != project {
                return Err(Error::Project {
                    stage: "add work",
                    detail: "parent work item belongs to a different project".to_owned(),
                });
            }
        }
        let work = WorkItem {
            id: self.repository.generate("work")?,
            project: project.clone(),
            parent: parent.cloned(),
            kind: kind.as_str().to_owned(),
            title: title.trim().to_owned(),
            description: description.map(ToOwned::to_owned),
            status: WorkStatus::Planned.as_str().to_owned(),
            assignee: assignee.map(ToOwned::to_owned),
            due,
            progress: 0,
            depends_on: Vec::new(),
            created_at: Utc::now(),
        };
        self.repository.save_work(&work)?;
        Ok(work)
    }

    /// Applies a work-item transition.
    pub fn transition(&self, work: &Id, event: TaskEvent) -> Result<WorkItem> {
        let current = self.repository.get_work(work)?;
        let from = WorkStatus::parse(&current.status).ok_or_else(|| Error::Project {
            stage: "transition",
            detail: format!("unknown current status {:?}", current.status),
        })?;
        let target = event.target();
        if !from.can_transition_to(target) {
            return Err(Error::Project {
                stage: "transition",
                detail: format!(
                    "cannot transition from {} to {}",
                    from.as_str(),
                    target.as_str()
                ),
            });
        }
        let progress = (target == WorkStatus::Done).then_some(100);
        self.repository
            .set_work_status(work, target.as_str(), progress)
    }

    /// Adds a deliverable.
    pub fn add_deliverable(
        &self,
        project: &Id,
        title: &str,
        kinds: &[String],
        asset: Option<&Id>,
        release: Option<&Id>,
        acceptance_criteria: &[String],
    ) -> Result<Deliverable> {
        self.repository.get_project(project)?;
        if title.trim().is_empty() {
            return Err(Error::Project {
                stage: "add deliverable",
                detail: "deliverable title must not be empty".to_owned(),
            });
        }
        let deliverable = Deliverable {
            id: self.repository.generate("deliverable")?,
            project: project.clone(),
            title: title.trim().to_owned(),
            kinds: if kinds.is_empty() {
                vec!["ontology".to_owned()]
            } else {
                kinds.to_vec()
            },
            asset: asset.cloned(),
            release: release.cloned(),
            status: "planned".to_owned(),
            evidence: Vec::new(),
            acceptance_criteria: acceptance_criteria.to_vec(),
            accepted_at: None,
            created_at: Utc::now(),
        };
        self.repository.save_deliverable(&deliverable)?;
        Ok(deliverable)
    }

    /// Links a deliverable to a registered release.
    pub async fn link_deliverable(&self, deliverable: &Id, release: &str) -> Result<ProjectResult> {
        let mut value = self.repository.get_deliverable(deliverable)?;
        let summary = self.catalog.release_by_reference(release).await?;
        if let Some(asset) = &value.asset {
            if asset != &summary.asset {
                return Err(Error::Project {
                    stage: "link deliverable",
                    detail: "release belongs to a different asset family".to_owned(),
                });
            }
        }
        value.asset = Some(summary.asset);
        value.release = Some(summary.id);
        if value.status == "planned" {
            value.status = "in_progress".to_owned();
        }
        self.repository.save_deliverable(&value)?;
        Ok(ProjectResult::Deliverable(value))
    }

    /// Accepts a deliverable against a validated registry release.
    pub async fn accept(
        &self,
        deliverable: &Id,
        evidence: Option<&str>,
        criterion: Option<&str>,
    ) -> Result<Deliverable> {
        let mut value = self.repository.get_deliverable(deliverable)?;
        let Some(release) = value.release.clone() else {
            return Err(Error::Project {
                stage: "accept",
                detail: "deliverable has no linked release".to_owned(),
            });
        };
        if let Some(criterion) = criterion {
            if !value
                .acceptance_criteria
                .iter()
                .any(|value| value == criterion)
            {
                return Err(Error::Project {
                    stage: "accept",
                    detail: format!("unknown acceptance criterion {criterion:?}"),
                });
            }
        }
        let summary = self.catalog.release(&release).await?;
        if !summary.validated {
            return Err(Error::Project {
                stage: "accept",
                detail: "linked release has no passing validation report".to_owned(),
            });
        }
        if summary.channel == "archived" {
            return Err(Error::Project {
                stage: "accept",
                detail: "archived releases cannot be accepted as evidence".to_owned(),
            });
        }
        if let Some(evidence) = evidence {
            if !value.evidence.iter().any(|value| value == evidence) {
                value.evidence.push(evidence.to_owned());
            }
        }
        value.status = "accepted".to_owned();
        value.accepted_at = Some(Utc::now());
        self.repository.save_deliverable(&value)?;
        Ok(value)
    }

    /// Records a snapshot.
    pub fn snapshot(&self, project: &Id) -> Result<ProjectSnapshot> {
        self.repository.get_project(project)?;
        let work = self.repository.list_work(project)?;
        let deliverables = self.repository.list_deliverables(project)?;
        let mut work_statuses = BTreeMap::new();
        for item in &work {
            work_statuses.insert(item.id.to_string(), item.status.clone());
        }
        let releases = deliverables
            .iter()
            .filter_map(|deliverable| deliverable.release.clone())
            .collect();
        let accepted_deliverables = deliverables
            .iter()
            .filter(|deliverable| deliverable.status == "accepted")
            .map(|deliverable| deliverable.id.clone())
            .collect();
        let snapshot = ProjectSnapshot {
            id: self.repository.generate("snapshot")?,
            project: project.clone(),
            created_at: Utc::now(),
            releases,
            work_statuses,
            accepted_deliverables,
        };
        self.repository.save_snapshot(&snapshot)?;
        Ok(snapshot)
    }

    /// Builds a project dashboard.
    pub fn overview(&self, project: &Id) -> Result<ProjectOverview> {
        let project_value = self.repository.get_project(project)?;
        let goals = self.repository.list_goals(project)?;
        let requirements = self.repository.list_requirements(project)?;
        let work = self.repository.list_work(project)?;
        let deliverables = self.repository.list_deliverables(project)?;
        let mut work_by_status = BTreeMap::new();
        let mut progress = 0.0;
        for item in &work {
            *work_by_status.entry(item.status.clone()).or_insert(0) += 1;
            progress += f64::from(item.progress);
        }
        if !work.is_empty() {
            progress /= work.len() as f64;
        }
        let accepted_deliverables = deliverables
            .iter()
            .filter(|deliverable| deliverable.status == "accepted")
            .count();
        let mut releases: Vec<Id> = deliverables
            .iter()
            .filter_map(|deliverable| deliverable.release.clone())
            .collect();
        releases.sort();
        releases.dedup();
        Ok(ProjectOverview {
            project: project_value,
            goals: goals.len(),
            requirements: requirements.len(),
            work_items: work.len(),
            deliverables: deliverables.len(),
            accepted_deliverables,
            progress,
            work_by_status,
            releases,
        })
    }
}

/// Converts a project result into the shared result family.
#[must_use]
pub fn result(value: ProjectResult) -> ontology_core::command::CommandResult {
    ontology_core::command::CommandResult::Project(value)
}
