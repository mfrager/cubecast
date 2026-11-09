//! RDF persistence for project, work, deliverable, and snapshot records.

use std::collections::BTreeSet;
use std::sync::Arc;

use chrono::{DateTime, Utc};
use ontology_core::error::{Error, Result};
use ontology_core::id::Id;
use ontology_core::model::{GraphName, Literal, Quad, QuadPattern, Term};
use ontology_core::project::{
    Deliverable, DependencyEdge, Goal, Project, ProjectSnapshot, Requirement, TimelineEntry,
    WorkItem, WorkKind,
};
use ontology_core::rdf::RdfStore;
use ontology_core::registry_graph::{
    project_core_graph, project_deliverables_graph, project_events_graph, project_goals_graph,
    project_requirements_graph, project_work_graph,
};

const RDF: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#";
const DCTERMS: &str = "http://purl.org/dc/terms/";
const PROV: &str = "http://www.w3.org/ns/prov#";
const PM: &str = "urn:onto:project:";

fn id(namespace: &str, local: &str) -> Id {
    Id::new(format!("{namespace}{local}")).expect("well-known vocabulary IRIs are valid")
}

fn pm(local: &str) -> Id {
    id(PM, local)
}

fn rdf_type() -> Id {
    id(RDF, "type")
}

fn dcterms(local: &str) -> Id {
    id(DCTERMS, local)
}

fn prov(local: &str) -> Id {
    id(PROV, local)
}

fn literal(quads: &[Quad], predicate: &Id) -> Option<String> {
    quads
        .iter()
        .find(|quad| &quad.predicate == predicate)
        .and_then(|quad| match &quad.object {
            Term::Literal(value) => Some(value.value.clone()),
            _ => None,
        })
}

fn literals(quads: &[Quad], predicate: &Id) -> Vec<String> {
    quads
        .iter()
        .filter(|quad| &quad.predicate == predicate)
        .filter_map(|quad| match &quad.object {
            Term::Literal(value) => Some(value.value.clone()),
            _ => None,
        })
        .collect()
}

fn iri(quads: &[Quad], predicate: &Id) -> Option<Id> {
    quads
        .iter()
        .find(|quad| &quad.predicate == predicate)
        .and_then(|quad| quad.object.as_iri().cloned())
}

fn iris(quads: &[Quad], predicate: &Id) -> Vec<Id> {
    let mut values: Vec<Id> = quads
        .iter()
        .filter(|quad| &quad.predicate == predicate)
        .filter_map(|quad| quad.object.as_iri().cloned())
        .collect();
    values.sort();
    values.dedup();
    values
}

fn integer(quads: &[Quad], predicate: &Id) -> u8 {
    literal(quads, predicate)
        .and_then(|value| value.parse().ok())
        .unwrap_or_default()
}

fn datetime(quads: &[Quad], predicate: &Id) -> Option<DateTime<Utc>> {
    quads
        .iter()
        .find(|quad| &quad.predicate == predicate)
        .and_then(|quad| match &quad.object {
            Term::Literal(value) => DateTime::parse_from_rfc3339(&value.value).ok(),
            _ => None,
        })
        .map(|value| value.with_timezone(&Utc))
}

fn now_quad(subject: &Id, predicate: &Id, value: &DateTime<Utc>, graph: &GraphName) -> Quad {
    Quad::in_graph(
        Term::Iri(subject.clone()),
        predicate.clone(),
        Term::Literal(Literal::datetime(value)),
        graph.clone(),
    )
}

fn typed(subject: &Id, class: &Id, graph: &GraphName) -> Quad {
    Quad::in_graph(
        Term::Iri(subject.clone()),
        rdf_type(),
        Term::Iri(class.clone()),
        graph.clone(),
    )
}

fn string(subject: &Id, predicate: &Id, value: &str, graph: &GraphName) -> Quad {
    Quad::in_graph(
        Term::Iri(subject.clone()),
        predicate.clone(),
        Term::string(value),
        graph.clone(),
    )
}

fn iri_quad(subject: &Id, predicate: &Id, object: &Id, graph: &GraphName) -> Quad {
    Quad::in_graph(
        Term::Iri(subject.clone()),
        predicate.clone(),
        Term::Iri(object.clone()),
        graph.clone(),
    )
}

fn int_quad(subject: &Id, predicate: &Id, value: u8, graph: &GraphName) -> Quad {
    Quad::in_graph(
        Term::Iri(subject.clone()),
        predicate.clone(),
        Term::Literal(Literal::integer(i64::from(value))),
        graph.clone(),
    )
}

fn typed_quads(quads: &[Quad], class: &Id) -> bool {
    quads
        .iter()
        .any(|quad| quad.predicate == rdf_type() && quad.object == Term::Iri(class.clone()))
}

/// Project repository over the main-owned project graphs.
pub struct ProjectRepository {
    store: Arc<dyn RdfStore>,
    base_iri: String,
}

impl ProjectRepository {
    /// Creates a repository.
    #[must_use]
    pub fn new(store: Arc<dyn RdfStore>, base_iri: impl Into<String>) -> Self {
        Self {
            store,
            base_iri: base_iri.into(),
        }
    }

    /// Generates an identifier under the configured project prefix.
    pub fn generate(&self, kind: &str) -> Result<Id> {
        Id::new(format!("{}{kind}:{}", self.base_iri, ulid::Ulid::new()))
    }

    fn subject(&self, subject: &Id, graph: &GraphName) -> Result<Vec<Quad>> {
        self.store.get(
            QuadPattern::new()
                .with_subject(Term::Iri(subject.clone()))
                .with_graph(graph.clone()),
        )
    }

    fn replace(&self, removals: Vec<Quad>, additions: Vec<Quad>) -> Result<()> {
        if let Ok(mut transaction) = self.store.transaction() {
            if !removals.is_empty() {
                transaction.delete(&removals)?;
            }
            if !additions.is_empty() {
                transaction.insert(&additions)?;
            }
            return transaction.commit();
        }
        if !removals.is_empty() {
            self.store.delete(&removals)?;
        }
        if !additions.is_empty() {
            self.store.insert(&additions)?;
        }
        Ok(())
    }

    fn replace_subject(&self, subject: &Id, graph: &GraphName, additions: Vec<Quad>) -> Result<()> {
        let removals = self.subject(subject, graph)?;
        self.replace(removals, additions)
    }

    fn ids_of_type(&self, class: &Id, graph: &GraphName) -> Result<Vec<Id>> {
        let quads = self.store.get(
            QuadPattern::new()
                .with_predicate(rdf_type())
                .with_object(Term::Iri(class.clone()))
                .with_graph(graph.clone()),
        )?;
        let mut ids: Vec<Id> = quads
            .into_iter()
            .filter_map(|quad| quad.subject.as_iri().cloned())
            .collect();
        ids.sort();
        ids.dedup();
        Ok(ids)
    }

    fn children(&self, project: &Id, graph: &GraphName) -> Result<Vec<Id>> {
        let quads = self.store.get(
            QuadPattern::new()
                .with_predicate(pm("project"))
                .with_object(Term::Iri(project.clone()))
                .with_graph(graph.clone()),
        )?;
        let mut ids: Vec<Id> = quads
            .into_iter()
            .filter_map(|quad| quad.subject.as_iri().cloned())
            .collect();
        ids.sort();
        ids.dedup();
        Ok(ids)
    }

    /// Inserts or updates a project.
    pub fn save_project(&self, project: &Project) -> Result<()> {
        let graph = project_core_graph();
        let mut additions = vec![
            typed(&project.id, &pm("Project"), &graph),
            string(&project.id, &dcterms("title"), &project.title, &graph),
            string(&project.id, &pm("status"), &project.status, &graph),
            now_quad(
                &project.id,
                &dcterms("created"),
                &project.created_at,
                &graph,
            ),
            now_quad(
                &project.id,
                &dcterms("modified"),
                &project.updated_at,
                &graph,
            ),
        ];
        if let Some(description) = &project.description {
            additions.push(string(
                &project.id,
                &dcterms("description"),
                description,
                &graph,
            ));
        }
        if let Some(owner) = &project.owner {
            additions.push(string(&project.id, &pm("owner"), owner, &graph));
        }
        self.replace_subject(&project.id, &graph, additions)
    }

    /// Loads a project.
    pub fn get_project(&self, id: &Id) -> Result<Project> {
        let graph = project_core_graph();
        let quads = self.subject(id, &graph)?;
        if !typed_quads(&quads, &pm("Project")) {
            return Err(Error::NotFound {
                kind: "project",
                id: id.to_string(),
            });
        }
        Ok(Project {
            id: id.clone(),
            title: literal(&quads, &dcterms("title")).unwrap_or_else(|| id.local_name().to_owned()),
            description: literal(&quads, &dcterms("description")),
            status: literal(&quads, &pm("status")).unwrap_or_else(|| "active".to_owned()),
            owner: literal(&quads, &pm("owner")),
            created_at: datetime(&quads, &dcterms("created")).unwrap_or_else(Utc::now),
            updated_at: datetime(&quads, &dcterms("modified")).unwrap_or_else(Utc::now),
        })
    }

    /// Lists projects.
    pub fn list_projects(&self) -> Result<Vec<Project>> {
        let mut values = Vec::new();
        for id in self.ids_of_type(&pm("Project"), &project_core_graph())? {
            if let Ok(project) = self.get_project(&id) {
                values.push(project);
            }
        }
        values.sort_by(|left, right| left.title.cmp(&right.title).then(left.id.cmp(&right.id)));
        Ok(values)
    }

    /// Saves a goal.
    pub fn save_goal(&self, goal: &Goal) -> Result<()> {
        let graph = project_goals_graph();
        let additions = vec![
            typed(&goal.id, &pm("Goal"), &graph),
            iri_quad(&goal.id, &pm("project"), &goal.project, &graph),
            string(&goal.id, &dcterms("title"), &goal.title, &graph),
            string(&goal.id, &pm("status"), &goal.status, &graph),
            now_quad(&goal.id, &dcterms("created"), &goal.created_at, &graph),
        ]
        .into_iter()
        .chain(
            goal.description
                .as_ref()
                .map(|description| string(&goal.id, &dcterms("description"), description, &graph)),
        )
        .collect();
        self.replace_subject(&goal.id, &graph, additions)
    }

    /// Lists goals.
    pub fn list_goals(&self, project: &Id) -> Result<Vec<Goal>> {
        let graph = project_goals_graph();
        let mut values = Vec::new();
        for id in self.children(project, &graph)? {
            let quads = self.subject(&id, &graph)?;
            if !typed_quads(&quads, &pm("Goal")) {
                continue;
            }
            values.push(Goal {
                id: id.clone(),
                project: project.clone(),
                title: literal(&quads, &dcterms("title"))
                    .unwrap_or_else(|| id.local_name().to_owned()),
                description: literal(&quads, &dcterms("description")),
                status: literal(&quads, &pm("status")).unwrap_or_else(|| "planned".to_owned()),
                created_at: datetime(&quads, &dcterms("created")).unwrap_or_else(Utc::now),
            });
        }
        values.sort_by(|left, right| left.title.cmp(&right.title));
        Ok(values)
    }

    /// Saves a requirement.
    pub fn save_requirement(&self, requirement: &Requirement) -> Result<()> {
        let graph = project_requirements_graph();
        let mut additions = vec![
            typed(&requirement.id, &pm("Requirement"), &graph),
            iri_quad(
                &requirement.id,
                &pm("project"),
                &requirement.project,
                &graph,
            ),
            string(
                &requirement.id,
                &dcterms("title"),
                &requirement.title,
                &graph,
            ),
            string(&requirement.id, &pm("kind"), &requirement.kind, &graph),
            string(&requirement.id, &pm("status"), &requirement.status, &graph),
            now_quad(
                &requirement.id,
                &dcterms("created"),
                &requirement.created_at,
                &graph,
            ),
        ];
        if let Some(description) = &requirement.description {
            additions.push(string(
                &requirement.id,
                &dcterms("description"),
                description,
                &graph,
            ));
        }
        self.replace_subject(&requirement.id, &graph, additions)
    }

    /// Lists requirements.
    pub fn list_requirements(&self, project: &Id) -> Result<Vec<Requirement>> {
        let graph = project_requirements_graph();
        let mut values = Vec::new();
        for id in self.children(project, &graph)? {
            let quads = self.subject(&id, &graph)?;
            if !typed_quads(&quads, &pm("Requirement")) {
                continue;
            }
            values.push(Requirement {
                id: id.clone(),
                project: project.clone(),
                title: literal(&quads, &dcterms("title"))
                    .unwrap_or_else(|| id.local_name().to_owned()),
                description: literal(&quads, &dcterms("description")),
                kind: literal(&quads, &pm("kind")).unwrap_or_else(|| "capability".to_owned()),
                status: literal(&quads, &pm("status")).unwrap_or_else(|| "planned".to_owned()),
                created_at: datetime(&quads, &dcterms("created")).unwrap_or_else(Utc::now),
            });
        }
        values.sort_by(|left, right| left.title.cmp(&right.title));
        Ok(values)
    }

    /// Saves a work item.
    pub fn save_work(&self, work: &WorkItem) -> Result<()> {
        let graph = project_work_graph();
        let class = match work.kind.as_str() {
            "work_package" => pm("WorkPackage"),
            "milestone" => pm("Milestone"),
            _ => pm("Task"),
        };
        let mut additions = vec![
            typed(&work.id, &class, &graph),
            iri_quad(&work.id, &pm("project"), &work.project, &graph),
            string(&work.id, &dcterms("title"), &work.title, &graph),
            string(&work.id, &pm("status"), &work.status, &graph),
            int_quad(&work.id, &pm("progress"), work.progress, &graph),
            now_quad(&work.id, &dcterms("created"), &work.created_at, &graph),
        ];
        if let Some(parent) = &work.parent {
            additions.push(iri_quad(&work.id, &pm("parentWorkItem"), parent, &graph));
        }
        if let Some(description) = &work.description {
            additions.push(string(
                &work.id,
                &dcterms("description"),
                description,
                &graph,
            ));
        }
        if let Some(assignee) = &work.assignee {
            additions.push(string(&work.id, &pm("assignee"), assignee, &graph));
        }
        if let Some(due) = &work.due {
            additions.push(now_quad(&work.id, &pm("due"), due, &graph));
        }
        for dependency in &work.depends_on {
            additions.push(iri_quad(&work.id, &pm("dependsOn"), dependency, &graph));
        }
        self.replace_subject(&work.id, &graph, additions)
    }

    /// Loads a work item.
    pub fn get_work(&self, id: &Id) -> Result<WorkItem> {
        let graph = project_work_graph();
        let quads = self.subject(id, &graph)?;
        let kind = if typed_quads(&quads, &pm("WorkPackage")) {
            WorkKind::WorkPackage
        } else if typed_quads(&quads, &pm("Milestone")) {
            WorkKind::Milestone
        } else if typed_quads(&quads, &pm("Task")) {
            WorkKind::Task
        } else {
            return Err(Error::NotFound {
                kind: "work item",
                id: id.to_string(),
            });
        };
        Ok(WorkItem {
            id: id.clone(),
            project: iri(&quads, &pm("project"))
                .ok_or_else(|| malformed("work item has no project"))?,
            parent: iri(&quads, &pm("parentWorkItem")),
            kind: kind.as_str().to_owned(),
            title: literal(&quads, &dcterms("title")).unwrap_or_else(|| id.local_name().to_owned()),
            description: literal(&quads, &dcterms("description")),
            status: literal(&quads, &pm("status")).unwrap_or_else(|| "planned".to_owned()),
            assignee: literal(&quads, &pm("assignee")),
            due: datetime(&quads, &pm("due")),
            progress: integer(&quads, &pm("progress")).min(100),
            depends_on: iris(&quads, &pm("dependsOn")),
            created_at: datetime(&quads, &dcterms("created")).unwrap_or_else(Utc::now),
        })
    }

    /// Lists work items.
    pub fn list_work(&self, project: &Id) -> Result<Vec<WorkItem>> {
        let graph = project_work_graph();
        let mut values = Vec::new();
        for id in self.children(project, &graph)? {
            if let Ok(work) = self.get_work(&id) {
                values.push(work);
            }
        }
        values.sort_by(|left, right| left.title.cmp(&right.title).then(left.id.cmp(&right.id)));
        Ok(values)
    }

    /// Saves a deliverable.
    pub fn save_deliverable(&self, deliverable: &Deliverable) -> Result<()> {
        let graph = project_deliverables_graph();
        let mut additions = vec![
            typed(&deliverable.id, &pm("Deliverable"), &graph),
            iri_quad(
                &deliverable.id,
                &pm("project"),
                &deliverable.project,
                &graph,
            ),
            string(
                &deliverable.id,
                &dcterms("title"),
                &deliverable.title,
                &graph,
            ),
            string(&deliverable.id, &pm("status"), &deliverable.status, &graph),
            now_quad(
                &deliverable.id,
                &dcterms("created"),
                &deliverable.created_at,
                &graph,
            ),
        ];
        for kind in &deliverable.kinds {
            additions.push(string(
                &deliverable.id,
                &pm("deliverableKind"),
                kind,
                &graph,
            ));
        }
        if let Some(asset) = &deliverable.asset {
            additions.push(iri_quad(
                &deliverable.id,
                &pm("realizesAsset"),
                asset,
                &graph,
            ));
        }
        if let Some(release) = &deliverable.release {
            additions.push(iri_quad(
                &deliverable.id,
                &pm("targetsRelease"),
                release,
                &graph,
            ));
        }
        for evidence in &deliverable.evidence {
            additions.push(string(
                &deliverable.id,
                &pm("hasEvidence"),
                evidence,
                &graph,
            ));
        }
        for criterion in &deliverable.acceptance_criteria {
            additions.push(string(
                &deliverable.id,
                &pm("hasAcceptanceCriterion"),
                criterion,
                &graph,
            ));
        }
        if let Some(accepted) = &deliverable.accepted_at {
            additions.push(now_quad(
                &deliverable.id,
                &pm("acceptedAt"),
                accepted,
                &graph,
            ));
        }
        self.replace_subject(&deliverable.id, &graph, additions)
    }

    /// Loads a deliverable.
    pub fn get_deliverable(&self, id: &Id) -> Result<Deliverable> {
        let graph = project_deliverables_graph();
        let quads = self.subject(id, &graph)?;
        if !typed_quads(&quads, &pm("Deliverable")) {
            return Err(Error::NotFound {
                kind: "deliverable",
                id: id.to_string(),
            });
        }
        Ok(Deliverable {
            id: id.clone(),
            project: iri(&quads, &pm("project"))
                .ok_or_else(|| malformed("deliverable has no project"))?,
            title: literal(&quads, &dcterms("title")).unwrap_or_else(|| id.local_name().to_owned()),
            kinds: literals(&quads, &pm("deliverableKind")),
            asset: iri(&quads, &pm("realizesAsset")),
            release: iri(&quads, &pm("targetsRelease")),
            status: literal(&quads, &pm("status")).unwrap_or_else(|| "planned".to_owned()),
            evidence: literals(&quads, &pm("hasEvidence")),
            acceptance_criteria: literals(&quads, &pm("hasAcceptanceCriterion")),
            accepted_at: datetime(&quads, &pm("acceptedAt")),
            created_at: datetime(&quads, &dcterms("created")).unwrap_or_else(Utc::now),
        })
    }

    /// Lists deliverables.
    pub fn list_deliverables(&self, project: &Id) -> Result<Vec<Deliverable>> {
        let graph = project_deliverables_graph();
        let mut values = Vec::new();
        for id in self.children(project, &graph)? {
            if let Ok(deliverable) = self.get_deliverable(&id) {
                values.push(deliverable);
            }
        }
        values.sort_by(|left, right| left.title.cmp(&right.title));
        Ok(values)
    }

    /// Updates project metadata fields.
    pub fn update_project(
        &self,
        id: &Id,
        title: Option<&str>,
        description: Option<&str>,
        owner: Option<&str>,
    ) -> Result<Project> {
        let mut project = self.get_project(id)?;
        if let Some(title) = title {
            project.title = title.to_owned();
        }
        if let Some(description) = description {
            project.description = Some(description.to_owned());
        }
        if let Some(owner) = owner {
            project.owner = Some(owner.to_owned());
        }
        project.updated_at = Utc::now();
        self.save_project(&project)?;
        Ok(project)
    }

    /// Updates only a work item status and progress.
    pub fn set_work_status(&self, id: &Id, status: &str, progress: Option<u8>) -> Result<WorkItem> {
        let mut work = self.get_work(id)?;
        work.status = status.to_owned();
        if let Some(progress) = progress {
            work.progress = progress.min(100);
        }
        self.save_work(&work)?;
        Ok(work)
    }

    /// Updates only a work item assignee.
    pub fn set_assignee(&self, id: &Id, assignee: &str) -> Result<WorkItem> {
        let mut work = self.get_work(id)?;
        work.assignee = Some(assignee.to_owned());
        self.save_work(&work)?;
        Ok(work)
    }

    /// Links a deliverable to a release.
    pub fn link_release(&self, id: &Id, release: &Id) -> Result<Deliverable> {
        let mut deliverable = self.get_deliverable(id)?;
        deliverable.release = Some(release.clone());
        if deliverable.status == "planned" {
            deliverable.status = "in_progress".to_owned();
        }
        self.save_deliverable(&deliverable)?;
        Ok(deliverable)
    }

    /// Marks a deliverable accepted.
    pub fn accept_deliverable(
        &self,
        id: &Id,
        evidence: Option<&str>,
        release: &Id,
    ) -> Result<Deliverable> {
        let mut deliverable = self.get_deliverable(id)?;
        if deliverable
            .release
            .as_ref()
            .is_some_and(|current| current != release)
        {
            return Err(Error::Project {
                stage: "accept",
                detail: "deliverable is already linked to a different release".to_owned(),
            });
        }
        deliverable.release = Some(release.clone());
        if let Some(evidence) = evidence {
            if !deliverable.evidence.iter().any(|value| value == evidence) {
                deliverable.evidence.push(evidence.to_owned());
            }
        }
        deliverable.status = "accepted".to_owned();
        deliverable.accepted_at = Some(Utc::now());
        self.save_deliverable(&deliverable)?;
        Ok(deliverable)
    }

    /// Inserts an immutable snapshot.
    pub fn save_snapshot(&self, snapshot: &ProjectSnapshot) -> Result<()> {
        let graph = project_events_graph();
        let mut additions = vec![
            typed(&snapshot.id, &pm("ProjectSnapshot"), &graph),
            typed(&snapshot.id, &prov("Entity"), &graph),
            iri_quad(&snapshot.id, &pm("project"), &snapshot.project, &graph),
            now_quad(
                &snapshot.id,
                &prov("generatedAtTime"),
                &snapshot.created_at,
                &graph,
            ),
        ];
        for release in &snapshot.releases {
            additions.push(iri_quad(
                &snapshot.id,
                &pm("snapshotRelease"),
                release,
                &graph,
            ));
        }
        for (work, status) in &snapshot.work_statuses {
            additions.push(string(
                &snapshot.id,
                &pm("workState"),
                &format!("{work}|{status}"),
                &graph,
            ));
        }
        for deliverable in &snapshot.accepted_deliverables {
            additions.push(iri_quad(
                &snapshot.id,
                &pm("acceptedDeliverable"),
                deliverable,
                &graph,
            ));
        }
        self.replace_subject(&snapshot.id, &graph, additions)
    }

    /// Lists snapshots.
    pub fn list_snapshots(&self, project: &Id) -> Result<Vec<ProjectSnapshot>> {
        let graph = project_events_graph();
        let mut values = Vec::new();
        for id in self.children(project, &graph)? {
            let quads = self.subject(&id, &graph)?;
            if !typed_quads(&quads, &pm("ProjectSnapshot")) {
                continue;
            }
            let mut work_statuses = std::collections::BTreeMap::new();
            for value in literals(&quads, &pm("workState")) {
                if let Some((work, status)) = value.split_once('|') {
                    work_statuses.insert(work.to_owned(), status.to_owned());
                }
            }
            values.push(ProjectSnapshot {
                id: id.clone(),
                project: project.clone(),
                created_at: datetime(&quads, &prov("generatedAtTime")).unwrap_or_else(Utc::now),
                releases: iris(&quads, &pm("snapshotRelease")),
                work_statuses,
                accepted_deliverables: iris(&quads, &pm("acceptedDeliverable")),
            });
        }
        values.sort_by_key(|right| std::cmp::Reverse(right.created_at));
        Ok(values)
    }

    /// Builds dependency edges for a project.
    pub fn dependencies(&self, project: &Id) -> Result<Vec<DependencyEdge>> {
        let mut edges = Vec::new();
        for work in self.list_work(project)? {
            for dependency in work.depends_on {
                edges.push(DependencyEdge {
                    from: work.id.clone(),
                    to: dependency,
                });
            }
        }
        edges.sort_by(|left, right| left.from.cmp(&right.from).then(left.to.cmp(&right.to)));
        Ok(edges)
    }

    /// Builds timeline entries for a project.
    pub fn timeline(&self, project: &Id) -> Result<Vec<TimelineEntry>> {
        let mut entries: Vec<TimelineEntry> = self
            .list_work(project)?
            .into_iter()
            .map(|work| TimelineEntry {
                id: work.id,
                title: work.title,
                kind: work.kind,
                status: work.status,
                due: work.due,
            })
            .collect();
        entries.sort_by(|left, right| left.due.cmp(&right.due).then(left.title.cmp(&right.title)));
        Ok(entries)
    }

    /// Returns all projects that reference a release.
    pub fn projects_for_release(&self, release: &Id) -> Result<BTreeSet<Id>> {
        let quads = self.store.get(
            QuadPattern::new()
                .with_predicate(pm("targetsRelease"))
                .with_object(Term::Iri(release.clone()))
                .with_graph(project_deliverables_graph()),
        )?;
        Ok(quads
            .into_iter()
            .filter_map(|quad| quad.subject.as_iri().cloned())
            .filter_map(|deliverable| self.get_deliverable(&deliverable).ok())
            .map(|deliverable| deliverable.project)
            .collect())
    }
}

fn malformed(detail: &str) -> Error {
    Error::Project {
        stage: "repository",
        detail: detail.to_owned(),
    }
}

/// Returns the graph containing project metadata.
#[must_use]
pub fn project_graph() -> GraphName {
    project_core_graph()
}
