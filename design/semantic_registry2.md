# Project Management Layer for the DCAT Ontology Registry

I would add a Project Management domain model above the ontology registry, rather than embedding project-management concepts directly into DCAT or the ontology metadata model.

The resulting system has three connected semantic layers:

1. Project layer: projects, goals, requirements, tasks, milestones, assignments, decisions, risks, and deliverables.
2. Semantic asset layer: ontology families, releases, modules, profiles, dependencies, and validation reports.
3. Execution layer: workflows, function flows, state machines, validation runs, imports, and publication activities.

The project layer manages the work required to build and maintain the semantic assets. The registry remains responsible for the assets themselves.

This distinction is important: a project task to update an ontology is not the ontology release, and a deliverable representing that release is not the same thing as the release itself.

## 1. Overall architecture

Project Management

Projects · Goals · Requirements · Tasks · Milestones · Teams · Risks

Work planning, deliverables, traceability

Semantic Asset Registry

DCAT · ADMS · Ontologies · Releases · Modules · Profiles · Dependencies

Workflow execution, validation, publication

Execution and Evidence

P-Plan · PROV-O · FnO · State Machines · SHACL · Reports

Rust Services · Oxigraph · Artifact Storage

One semantic store, separate named graphs and domain services

The project-management vocabulary should be able to manage projects that produce ontologies, but also projects that produce other things: software, documentation, datasets, workflows, APIs, and research artifacts.

That makes the project system reusable beyond the ontology registry.

## 2. Vocabulary foundation

I would use existing vocabularies selectively and introduce a small `pm:` namespace for the integrated project model.

| Vocabulary  | Role                                                              |
| ----------- | ----------------------------------------------------------------- |
| PROMCODE    | Project scope, work items, artifacts, risks, measures, plans      |
| IproK       | Work breakdown, tasks, dependencies, resources, scheduling, costs |
| PRJ         | Goal/task decomposition and goal-oriented project structure       |
| P-Plan      | Planned steps, variables, and dependencies                        |
| PROV-O      | Actual execution and provenance                                   |
| DCAT / ADMS | Project deliverables that are semantic assets                     |
| Dublin Core | General resource metadata                                         |
| ORG / FOAF  | Organizations, people, teams, roles                               |
| OWL-Time    | Dates, intervals, durations                                       |
| QUDT        | Quantities, units, estimates, costs                               |
| SKOS        | Statuses, priorities, classifications                             |
| SHACL       | Project and workflow validation                                   |

PROMCODE is the best established project-management starting point for contracted software delivery, while IproK provides a richer research reference for work breakdown, task dependencies, resource allocation, scheduling, and costs. PRJ is useful for goal/task decomposition, but is an older, less mature vocabulary. I would not adopt any one of these as the complete internal data model.&#x20;

[image](https://www.google.com/s2/favicons?domain=https://www.oasis-open.org\&sz=32)

OASIS Open+2



Recommended approach: reuse standard terms where their semantics fit, then add `pm:` terms for the integrated concepts and relationships that are missing.

## 3. Integrated project-management data model

### 3.1 Core entity hierarchy

The project model should support multiple projects, each containing a structured hierarchy of goals, work packages, tasks, milestones, and deliverables.

```
Project
 ├── Goals
 │    ├── Objectives
 │    └── Success Criteria
 │
 ├── Requirements
 │    ├── Functional Requirements
 │    ├── Technical Requirements
 │    └── Acceptance Criteria
 │
 ├── Work Breakdown Structure
 │    ├── Phase
 │    ├── Work Package
 │    │    ├── Task
 │    │    └── Subtask
 │    └── Milestone
 │
 ├── Deliverables
 │    ├── Ontology
 │    ├── Ontology Release
 │    ├── Documentation
 │    ├── Software
 │    └── Report
 │
 ├── Resources
 │    ├── People
 │    ├── Teams
 │    ├── Roles
 │    └── Allocations
 │
 ├── Risks and Issues
 ├── Decisions
 ├── Constraints
 └── Project Events
```

### 3.2 Proposed `pm:` vocabulary

Namespace:

```
@prefix pm: <https://example.org/ontology/project-management#> .
```

These are custom extension terms, not claims about existing standard vocabulary terms.

Project and decomposition

| Class                 | Meaning                                   |
| --------------------- | ----------------------------------------- |
| `pm:Project`          | A managed project                         |
| `pm:ProjectPhase`     | A major project stage                     |
| `pm:WorkPackage`      | A group of related work                   |
| `pm:WorkItem`         | Abstract unit of managed work             |
| `pm:Task`             | Actionable work item                      |
| `pm:Subtask`          | Decomposed task                           |
| `pm:Milestone`        | Significant completion point              |
| `pm:Goal`             | Desired outcome                           |
| `pm:Objective`        | More specific measurable outcome          |
| `pm:SuccessCriterion` | Condition for considering a goal achieved |

Requirements and design

| Class                      | Meaning                                    |
| -------------------------- | ------------------------------------------ |
| `pm:Requirement`           | A required capability or condition         |
| `pm:FunctionalRequirement` | Required behavior                          |
| `pm:TechnicalRequirement`  | Technical constraint or requirement        |
| `pm:AcceptanceCriterion`   | Testable completion condition              |
| `pm:Decision`              | An approved project decision               |
| `pm:Constraint`            | A restriction on the solution or work      |
| `pm:Assumption`            | An accepted planning assumption            |
| `pm:ChangeRequest`         | A proposed change to scope or requirements |

Planning and scheduling

| Class            | Meaning                                  |
| ---------------- | ---------------------------------------- |
| `pm:ProjectPlan` | A project-level plan                     |
| `pm:WorkPlan`    | Plan for a work package or task group    |
| `pm:Schedule`    | Planned temporal arrangement             |
| `pm:Baseline`    | Approved reference plan or schedule      |
| `pm:Dependency`  | Explicit relationship between work items |
| `pm:Estimate`    | Effort, duration, or cost estimate       |
| `pm:Iteration`   | A bounded work cycle                     |

Resources and governance

| Class           | Meaning                               |
| --------------- | ------------------------------------- |
| `pm:Team`       | A project team                        |
| `pm:Assignment` | Assignment of a resource to work      |
| `pm:Resource`   | A person, service, or other resource  |
| `pm:Role`       | Responsibility within the project     |
| `pm:Risk`       | A possible adverse or uncertain event |
| `pm:Issue`      | An existing problem requiring action  |
| `pm:Review`     | A review of work or a deliverable     |
| `pm:Approval`   | An approval decision                  |
| `pm:Status`     | A project lifecycle status            |

Deliverables and evidence

| Class                | Meaning                                   |
| -------------------- | ----------------------------------------- |
| `pm:Deliverable`     | An expected project output                |
| `pm:Artifact`        | A produced or referenced artifact         |
| `pm:Evidence`        | Evidence supporting a claim or acceptance |
| `pm:Acceptance`      | Acceptance of a deliverable               |
| `pm:ProjectEvent`    | A significant event in project history    |
| `pm:ProjectSnapshot` | A recorded state of the project           |

### 3.3 Core relationships

| Property                    | Domain → Range              | Meaning                             |
| --------------------------- | --------------------------- | ----------------------------------- |
| `pm:hasPhase`               | Project → Phase             | Project decomposition               |
| `pm:hasWorkPackage`         | Project/Phase → WorkPackage | Work breakdown                      |
| `pm:hasTask`                | WorkPackage → Task          | Task decomposition                  |
| `pm:parentWorkItem`         | WorkItem → WorkItem         | Parent relationship                 |
| `pm:hasGoal`                | Project → Goal              | Project goals                       |
| `pm:hasObjective`           | Goal → Objective            | Goal decomposition                  |
| `pm:contributesTo`          | WorkItem → Goal             | Work supports an outcome            |
| `pm:fulfills`               | Deliverable → Requirement   | Deliverable satisfies a requirement |
| `pm:implements`             | Artifact → Requirement      | Artifact implements a requirement   |
| `pm:dependsOn`              | WorkItem → WorkItem         | Work dependency                     |
| `pm:blocks`                 | WorkItem → WorkItem         | Blocking relationship               |
| `pm:assignedTo`             | WorkItem → Agent/Team       | Responsibility                      |
| `pm:hasAssignment`          | WorkItem → Assignment       | Assignment record                   |
| `pm:produces`               | WorkItem → Deliverable      | Work output                         |
| `pm:hasMilestone`           | Project → Milestone         | Project milestone                   |
| `pm:hasRisk`                | Project/WorkItem → Risk     | Risk association                    |
| `pm:hasIssue`               | Project/WorkItem → Issue    | Issue association                   |
| `pm:hasDecision`            | Project → Decision          | Decision history                    |
| `pm:hasAcceptanceCriterion` | Deliverable → Criterion     | Completion conditions               |
| `pm:acceptedBy`             | Deliverable → Agent         | Acceptance authority                |
| `pm:hasEvidence`            | Deliverable → Evidence      | Supporting evidence                 |

I would keep the `pm:WorkItem` hierarchy separate from `p-plan:Step`. A task is a unit of project accountability; a plan step is a unit of planned execution. A task can contain multiple plan steps, and a single reusable workflow can be invoked by many tasks.

### 3.4 Example project graph

This example shows a project to create a planning ontology and publish it through the registry.

```
@prefix pm:    <https://example.org/ontology/project-management#> .
@prefix dcat:  <http://www.w3.org/ns/dcat#> .
@prefix dct:   <http://purl.org/dc/terms/> .
@prefix ex:    <https://example.org/project/ontology-registry/> .

ex:project
    a pm:Project ;
    dct:title "Ontology Registry Planning Model" ;
    pm:hasGoal ex:goal-planning ;
    pm:hasWorkPackage ex:work-ontology ;
    pm:hasMilestone ex:milestone-release .

ex:goal-planning
    a pm:Goal ;
    dct:title "Provide reusable project planning semantics" ;
    pm:hasObjective ex:objective-publish .

ex:objective-publish
    a pm:Objective ;
    dct:title "Publish a versioned planning ontology" ;
    pm:hasSuccessCriterion ex:criterion-validation .

ex:work-ontology
    a pm:WorkPackage ;
    dct:title "Planning ontology development" ;
    pm:hasTask ex:task-model ;
    pm:hasTask ex:task-validate .

ex:task-model
    a pm:Task ;
    dct:title "Define planning ontology" ;
    pm:contributesTo ex:goal-planning ;
    pm:produces ex:deliverable-ontology .

ex:task-validate
    a pm:Task ;
    dct:title "Validate ontology against registry profile" ;
    pm:dependsOn ex:task-model ;
    pm:produces ex:deliverable-validation .

ex:deliverable-ontology
    a pm:Deliverable ;
    dct:title "Planning Ontology Release" ;
    pm:fulfills ex:objective-publish ;
    pm:realizesAsset ex:planning-ontology .

ex:planning-ontology
    a dcat:Dataset ;
    dct:title "Planning Ontology" .

ex:criterion-validation
    a pm:AcceptanceCriterion ;
    dct:title "Ontology passes the required SHACL profile" .
```

`pm:realizesAsset` is another proposed extension property. The ontology release would be described using the registry's DCAT/ADMS model; the deliverable points to that release rather than duplicating its metadata.

## 4. Connecting projects to the ontology registry

This is the central integration point.

A project should be able to manage the complete lifecycle of an ontology as a deliverable.

### 4.1 Deliverable-to-registry mapping

| Project concept      | Registry concept                                                  | Relationship                      |
| -------------------- | ----------------------------------------------------------------- | --------------------------------- |
| Project              | Catalog or collection of assets                                   | Project manages catalog           |
| Requirement          | Ontology capability or profile requirement                        | Requirement constrains asset      |
| Task                 | Ingestion, modeling, analysis, validation, or publishing activity | Task is executed by workflow      |
| Deliverable          | Ontology asset or release                                         | Deliverable realizes asset        |
| Milestone            | Release or approval milestone                                     | Milestone is satisfied by event   |
| Acceptance criterion | SHACL shape or validation rule                                    | Criterion evaluated by validation |
| Decision             | Ontology design decision                                          | Decision informs model            |
| Risk                 | Dependency or compatibility risk                                  | Risk concerns release/dependency  |
| Evidence             | Validation report, diff, or test report                           | Evidence supports acceptance      |
| Baseline             | Selected asset release set                                        | Baseline pins versions            |

I would add a few more custom relationships:

```
pm:managesAsset
pm:realizesAsset
pm:targetsRelease
pm:requiresProfile
pm:validatedBy
pm:acceptedThrough
pm:hasEvidence
pm:dependsOnAsset
pm:approvedRelease
```

These make project and registry queries straightforward.

### 4.2 Project deliverable types

Use a `pm:DeliverableType` classification, preferably backed by SKOS.

Initial concept scheme:

```
Deliverable Types
 ├── Semantic Asset
 │    ├── Ontology
 │    ├── Vocabulary
 │    ├── SHACL Profile
 │    └── Mapping Set
 ├── Documentation
 ├── Software
 ├── Dataset
 ├── Workflow
 ├── Validation Report
 └── Research Output
```

A deliverable can have multiple classifications. For example, an ontology release can also be a research output.

### 4.3 Project-to-asset relationships

The model should support multiple projects working on the same ontology, and a project working on multiple ontologies.

```
Project A ── manages ── Ontology X
Project B ── manages ── Ontology X
Project B ── manages ── Ontology Y
Project C ── consumes ── Ontology X
```

Distinguish:

- Manages: responsible for developing or maintaining an asset.
- Produces: work produces a new asset or release.
- Consumes: project uses an asset as an input.
- Depends on: project cannot complete without an asset.
- Adopts: project formally selects an asset or profile.
- Contributes to: project contributes changes to an externally maintained asset.

This prevents a registry from assuming that the project that uses an ontology is necessarily the project that owns or maintains it.

## 5. Planning and execution model

The project-management layer should use P-Plan and PROV-O as complementary models.

- Project plan: what the project intends to accomplish.
- Work items: who is responsible for accomplishing it.
- P-Plan: how a particular plan decomposes into steps and data dependencies.
- Execution: what actually happened.
- PROV-O: which agents, activities, and entities were involved.
- State machine: what lifecycle state each work item or release is in.

### 5.1 Connecting the planning vocabulary

A project task can have a P-Plan describing its execution.

```
Project
  └── Work Package
       └── Task: Publish ontology release
            └── P-Plan
                 ├── Step: Import RDF
                 ├── Step: Resolve imports
                 ├── Step: Analyze ontology
                 ├── Step: Validate profile
                 └── Step: Publish release
```

Example mapping:

| Project model    | P-Plan                                  |
| ---------------- | --------------------------------------- |
| `pm:Task`        | `p-plan:Plan` or associated with a plan |
| `pm:WorkPlan`    | `p-plan:Plan`                           |
| `pm:PlanStep`    | `p-plan:Step`                           |
| Task input       | `p-plan:Variable`                       |
| Task output      | `p-plan:Variable`                       |
| Step dependency  | `p-plan:isPrecededBy`                   |
| Actual execution | `prov:Activity`                         |
| Actual output    | `prov:Entity`                           |

I would not assert that a `pm:Task` is automatically a `p-plan:Step`. Instead, relate the task to its execution plan:

```
ex:task-publish
    a pm:Task ;
    pm:hasExecutionPlan ex:publish-plan .

ex:publish-plan
    a p-plan:Plan .
```

This permits a project task to have multiple plans, alternate approaches, or a revised plan without changing the task's identity.

### 5.2 Function flows as executable plans

The function-flow subsystem previously discussed can implement individual plan steps or entire task workflows.

For example:

```
load_ontology(source) \    >> normalize_rdf() \    >> resolve_imports() \    >> analyze_ontology() \    >> validate_profile(profile) \    >> create_release_candidate() \    >> publish_release()
```

The RDF model should distinguish:

| Concept                 | Vocabulary                      |
| ----------------------- | ------------------------------- |
| Function definition     | FnO                             |
| Workflow node           | Wfdesc / custom flow ontology   |
| Plan step               | P-Plan                          |
| Project task            | `pm:Task`                       |
| Function execution      | FnO execution / PROV-O          |
| Actual execution output | PROV-O Entity                   |
| Flow state              | Custom state-machine vocabulary |

A project task invokes a flow, but the flow is not the task. The same flow can be reused by many tasks, projects, and asset types.

### 5.3 State machines for project lifecycle

Use a small project state model for work items, deliverables, and releases.

Task lifecycle

Backlog

Ready

In Progress

Blocked

Review

Done

Ontology release lifecycle

Draft

Candidate

Validation

Approved

Published

Archived

These are illustrative lifecycle states. Define them as SKOS concepts and model actual transitions using the custom state-machine vocabulary.

An important distinction: `pm:status` should identify the current state of a task, while PROV-O records the activities that caused the state to change. Do not overwrite history when updating status.

### 5.4 Project events and provenance

Every significant project action should generate a provenance record.

Examples:

- Task assigned.
- Requirement approved.
- Ontology imported.
- Ontology release candidate created.
- SHACL validation passed.
- Release published.
- Deliverable accepted.
- Dependency upgraded.
- Risk mitigated.

Represent actual operations as `prov:Activity`, with agents, inputs, outputs, and timestamps. Link those activities back to their corresponding project tasks and plan steps.

This gives the system an auditable history without making the project model itself an event log.

## 6. Project graph and storage architecture

Keep the project model and registry model in separate named graphs, but allow resources to link across graph boundaries.

### 6.1 Suggested graph partitioning

| Named graph             | Contents                                       |
| ----------------------- | ---------------------------------------------- |
| `pm:projects`           | Project definitions and metadata               |
| `pm:work`               | Tasks, phases, work packages, milestones       |
| `pm:goals`              | Goals, objectives, success criteria            |
| `pm:requirements`       | Requirements, acceptance criteria, constraints |
| `pm:resources`          | Teams, roles, assignments, allocations         |
| `pm:governance`         | Risks, issues, decisions, approvals            |
| `pm:plans`              | Project plans and P-Plan structures            |
| `pm:deliverables`       | Deliverable descriptions and asset links       |
| `pm:events`             | Project events and snapshots                   |
| `registry:catalog`      | DCAT catalog metadata                          |
| `registry:ontology:*`   | Immutable ontology releases                    |
| `registry:analysis:*`   | Derived term and dependency indexes            |
| `registry:validation:*` | SHACL reports                                  |
| `registry:provenance:*` | Asset-related provenance                       |

The project graphs can be updated as work progresses. Published ontology graphs remain immutable.

### 6.2 Project snapshots and baselines

A project should have immutable snapshots at meaningful points:

- Initial approved scope.
- Schedule baseline.
- Release milestone.
- Accepted deliverable.
- Project completion.

A `pm:ProjectSnapshot` should identify the project state and the exact asset releases and plans it references.

For example:

```
Project Snapshot 3
 ├── Project Plan 2
 ├── Schedule Baseline 1
 ├── Planning Ontology 1.2
 ├── Workflow Vocabulary 1.0
 ├── SHACL Profile 2.1
 └── Acceptance Report 3
```

This is especially important for reproducibility. A completed project should be reconstructable using the exact ontology and profile versions it used at the time.

### 6.3 Cross-project dependencies

The registry should support project dependencies without forcing all projects into one graph.

```
Project A: Core Ontology
      │
      ▼
Ontology Release 2.0
      │
      ├───────────────┐
      ▼               ▼
Project B          Project C
Planning Profile   Workflow Profile
      │               │
      ▼               ▼
Application X      Application Y
```

The dependency graph should distinguish project-to-project dependencies, project-to-asset dependencies, and asset-to-asset dependencies. They have different lifecycle and ownership semantics.

## 7. Project management services in Rust

I would extend the previous registry crate architecture with a separate project-management domain.

```
ontology-platform/
├── crates/
│   ├── registry-core/
│   ├── registry-rdf/
│   ├── registry-store/
│   ├── registry-catalog/
│   ├── registry-ontology/
│   ├── registry-import/
│   ├── registry-analysis/
│   ├── registry-validation/
│   ├── registry-versioning/
│   ├── registry-publish/
│   │
│   ├── project-core/
│   ├── project-model/
│   ├── project-repository/
│   ├── project-planning/
│   ├── project-scheduling/
│   ├── project-resources/
│   ├── project-governance/
│   ├── project-deliverables/
│   ├── project-baselines/
│   │
│   ├── workflow-core/
│   ├── workflow-engine/
│   ├── state-machine/
│   ├── provenance/
│   │
│   ├── platform-api/
│   ├── platform-cli/
│   └── platform-mcp/
```

### 7.1 Project domain types

A possible initial type model:

```
pub struct Project {
    pub id: ProjectId,
    pub title: String,
    pub goals: Vec<GoalId>,
    pub work_packages: Vec<WorkPackageId>,
    pub status: ProjectStatus,
}

pub struct WorkPackage {
    pub id: WorkPackageId,
    pub project: ProjectId,
    pub parent: Option<WorkPackageId>,
    pub tasks: Vec<TaskId>,
}

pub struct Task {
    pub id: TaskId,
    pub title: String,
    pub status: TaskStatus,
    pub dependencies: Vec<TaskId>,
    pub assignments: Vec<AssignmentId>,
    pub deliverables: Vec<DeliverableId>,
    pub plan: Option<PlanId>,
}

pub struct Deliverable {
    pub id: DeliverableId,
    pub title: String,
    pub kind: DeliverableKind,
    pub asset: Option<AssetId>,
    pub target_release: Option<ReleaseId>,
    pub acceptance_criteria: Vec<CriterionId>,
}

pub struct Goal {
    pub id: GoalId,
    pub title: String,
    pub parent: Option<GoalId>,
    pub success_criteria: Vec<CriterionId>,
}

pub struct Assignment {
    pub id: AssignmentId,
    pub work_item: WorkItemId,
    pub assignee: AgentId,
    pub role: Option<RoleId>,
}
```

These are application-level representations, not substitutes for the RDF model. The repository layer should translate between the Rust domain types and RDF resources.

### 7.2 Important service interfaces

```
pub trait ProjectService {
    async fn create(&self, project: NewProject)
        -> Result<ProjectId, ProjectError>;

    async fn get(&self, id: ProjectId)
        -> Result<Project, ProjectError>;

    async fn snapshot(&self, id: ProjectId)
        -> Result<SnapshotId, ProjectError>;
}

pub trait WorkManagement {
    async fn create_task(&self, task: NewTask)
        -> Result<TaskId, ProjectError>;

    async fn transition_task(
        &self,
        id: TaskId,
        event: TaskEvent,
    ) -> Result<TaskStatus, ProjectError>;

    async fn assign(
        &self,
        task: TaskId,
        assignment: Assignment,
    ) -> Result<(), ProjectError>;
}

pub trait DeliverableManagement {
    async fn link_asset(
        &self,
        deliverable: DeliverableId,
        asset: AssetId,
    ) -> Result<(), ProjectError>;

    async fn evaluate_acceptance(
        &self,
        deliverable: DeliverableId,
    ) -> Result<AcceptanceReport, ProjectError>;
}

pub trait ProjectPlanner {
    async fn generate_plan(
        &self,
        task: TaskId,
    ) -> Result<PlanId, ProjectError>;

    async fn evaluate_dependencies(
        &self,
        project: ProjectId,
    ) -> Result<DependencyReport, ProjectError>;
}
```

I would keep the project planner separate from the workflow executor. The planner creates or revises plans; the workflow engine executes approved plans. This also makes it possible to use a human-approved plan or an external planning engine.

## 8. HTTP API and CLI extensions

The project API should live alongside the ontology registry API, under a shared service boundary.

### Project API

| Method  | Endpoint                      | Purpose                       |
| ------- | ----------------------------- | ----------------------------- |
| `POST`  | `/projects`                   | Create project                |
| `GET`   | `/projects`                   | Search projects               |
| `GET`   | `/projects/{id}`              | Get project                   |
| `GET`   | `/projects/{id}/overview`     | Project dashboard data        |
| `GET`   | `/projects/{id}/goals`        | List goals                    |
| `POST`  | `/projects/{id}/goals`        | Create goal                   |
| `GET`   | `/projects/{id}/work`         | List work items               |
| `POST`  | `/projects/{id}/work`         | Create task or work package   |
| `PATCH` | `/work/{id}`                  | Update work item              |
| `POST`  | `/work/{id}/transition`       | Transition lifecycle state    |
| `POST`  | `/work/{id}/assignments`      | Assign resource               |
| `GET`   | `/projects/{id}/dependencies` | Work dependency graph         |
| `GET`   | `/projects/{id}/deliverables` | Project deliverables          |
| `POST`  | `/deliverables/{id}/accept`   | Accept deliverable            |
| `POST`  | `/projects/{id}/snapshots`    | Create snapshot               |
| `GET`   | `/projects/{id}/timeline`     | Project events and milestones |
| `GET`   | `/projects/{id}/risks`        | Project risks                 |
| `GET`   | `/projects/{id}/decisions`    | Project decisions             |

### Registry integration API

```
GET  /projects/{id}/assets
POST /projects/{id}/assets
GET  /projects/{id}/asset-dependencies
GET  /projects/{id}/deliverables/{deliverable}/release
POST /projects/{id}/deliverables/{deliverable}/validate
POST /projects/{id}/deliverables/{deliverable}/publish
GET  /projects/{id}/compatibility
```

These endpoints should use the existing registry services, not duplicate their operations.

### CLI

I would extend `oreg` into a unified platform CLI, or introduce `sem` as the top-level command if you want the project system to become a broader platform.

```
# Projects
sem project create "Ontology Registry"
sem project list
sem project show ontology-registry
sem project overview ontology-registry

# Goals and requirements
sem goal add ontology-registry "Support reusable planning"
sem requirement add ontology-registry "Versioned ontology releases"

# Work management
sem work add ontology-registry "Build planning vocabulary"
sem work list ontology-registry
sem work assign task-123 user-456
sem work transition task-123 review
sem work dependencies ontology-registry

# Planning
sem plan generate task-123
sem plan show plan-123
sem plan approve plan-123
sem plan execute plan-123

# Deliverables
sem deliverable list ontology-registry
sem deliverable link task-123 planning-ontology
sem deliverable validate deliverable-123
sem deliverable accept deliverable-123

# Project and asset lifecycle
sem project snapshot ontology-registry
sem project timeline ontology-registry
sem project compatibility ontology-registry
sem project report ontology-registry
```

The CLI should use the same HTTP API and permission model as the user interface and MCP interface.

## 9. Queries the integrated model should support

The real value of connecting project management and the registry is the ability to ask questions that cross both domains.

### What work remains before an ontology can be published?

```
SELECT ?task ?title ?status
WHERE {
    ?task a pm:Task ;
          pm:produces ex:planning-deliverable ;
          dct:title ?title ;
          pm:status ?status .

    FILTER (?status != pm:Done)
}
```

### Which projects depend on a particular ontology?

```
SELECT ?project ?title
WHERE {
    ?project a pm:Project ;
             dct:title ?title ;
             pm:adopts ex:planning-ontology .
}
```

### Which deliverables have not passed validation?

```
SELECT ?deliverable ?report
WHERE {
    ?deliverable a pm:Deliverable ;
                 pm:hasEvidence ?report .

    ?report a sh:ValidationReport ;
            sh:conforms false .
}
```

### Which tasks are blocked by upstream dependencies?

```
SELECT ?task ?blocker
WHERE {
    ?task a pm:Task ;
          pm:dependsOn+ ?blocker .

    ?blocker pm:status pm:Blocked .
}
```

### Which projects would be affected by an ontology upgrade?

This requires traversing the asset dependency graph and the project adoption relationships:

```
Ontology release
    → Ontology family
    → Dependent ontology releases
    → Profiles
    → Project asset adoptions
    → Project deliverables
    → Affected work items
```

This should be implemented as a dedicated dependency-impact analysis service, rather than requiring an LLM to compose increasingly complicated SPARQL queries.

## 10. Project dashboards and reports

The model should support a project overview generated from RDF rather than maintained as a separate collection of unrelated dashboard fields.

A project overview should include:

- Goals and measurable progress.
- Requirements and coverage.
- Work breakdown and task status.
- Critical dependencies and blocked work.
- Milestones and schedule.
- Assigned resources and workload.
- Risks and open issues.
- Deliverables and acceptance.
- Ontology releases and compatibility.
- Recent activity and decisions.

For ontology-specific projects, add:

- Number of ontology assets under management.
- Number of published releases.
- Number of unresolved imports.
- Validation conformance.
- Breaking changes awaiting review.
- Dependent projects affected by pending upgrades.
- Unaccepted deliverables.

For project progress, do not simply average task completion percentages. Calculate progress from weighted work packages, accepted deliverables, and goal-specific success criteria. Keep the calculation method explicit and versioned.

## 11. SHACL validation and project integrity

The project model should have its own SHACL shapes, separate from ontology validation shapes.

Examples of rules:

| Rule             | Constraint                                                     |
| ---------------- | -------------------------------------------------------------- |
| Project identity | Every project has a title and identifier                       |
| Task ownership   | Every active task has an accountable owner                     |
| Task dependency  | No task depends on itself                                      |
| Dependency graph | Cycles are detected and reported                               |
| Milestone        | Every milestone belongs to a project                           |
| Deliverable      | Every deliverable has an expected output                       |
| Acceptance       | Accepted deliverables have acceptance evidence                 |
| Publication      | Published ontology deliverables reference an immutable release |
| Baseline         | Baselines reference exact release versions                     |
| Completion       | A completed project satisfies its required completion criteria |

Some rules, particularly dependency cycles and schedule feasibility, require application-level algorithms rather than SHACL alone.

I would also introduce a project profile:

```
Project Management Profile
 ├── Project Shapes
 ├── Goal Shapes
 ├── Requirement Shapes
 ├── Work Item Shapes
 ├── Dependency Shapes
 ├── Deliverable Shapes
 ├── Assignment Shapes
 └── Governance Shapes
```

This profile can itself be registered as a semantic asset in DCAT. That means the project-management vocabulary can evolve through the same versioning and validation infrastructure as the other ontologies.

## 12. Recommended implementation phases

I would implement this as an extension to the ontology registry, not as a separate project-management application.

1. Phase 1 — Project vocabulary

   Implement `pm:` core classes and relationships, PROMCODE mappings, project CRUD, work breakdown, goals, and requirements.

   Foundation
2. Phase 2 — Registry integration

   Add project-to-asset relationships, deliverables, release targets, acceptance criteria, and project asset dependencies.
3. Phase 3 — Planning and execution

   Integrate P-Plan, FnO, Wfdesc, PROV-O, and the state-machine engine. Implement task plans, reusable workflows, and execution history.
4. Phase 4 — Scheduling and governance

   Add milestones, assignments, resource allocations, estimates, risks, issues, decisions, and baselines.
5. Phase 5 — Project API and agents

   Implement HTTP endpoints, CLI commands, MCP tools, project reports, and cross-ontology queries.
6. Phase 6 — Advanced planning

   Add dependency-impact analysis, automated planning, compatibility-driven work generation, project snapshots, and portfolio-level management.

## 13. Final vocabulary and architecture recommendation

I would organize the complete platform around these semantic domains:

| Domain             | Vocabulary foundation | Responsibility                         |
| ------------------ | --------------------- | -------------------------------------- |
| Catalog            | DCAT + ADMS           | Semantic asset discovery               |
| Ontology           | OWL + RDFS            | Ontology definitions                   |
| Taxonomy           | SKOS                  | Classification and mappings            |
| Validation         | SHACL                 | Structural and semantic constraints    |
| Project management | PROMCODE + custom PM  | Goals, tasks, deliverables, resources  |
| Planning           | P-Plan                | Plans, steps, variables                |
| Function flows     | FnO + Wfdesc          | Functions, ports, dataflow             |
| Execution          | PROV-O                | Actual activity and provenance         |
| State machines     | Custom SM             | Lifecycle and transitions              |
| Organization       | ORG + FOAF            | People, teams, organizational roles    |
| Time               | OWL-Time              | Temporal modeling                      |
| Quantities         | QUDT                  | Estimates, effort, costs, measurements |

The most important design decision: make `pm:Deliverable` the bridge between project management and the semantic registry.

A task produces a deliverable. A deliverable realizes a specific semantic asset or release. A release has dependencies, distributions, validation reports, and provenance. Acceptance criteria determine whether the deliverable is complete.

That produces a traceable chain:

```
Project Goal
    ↓
Requirement
    ↓
Work Package
    ↓
Task
    ↓
Execution Plan
    ↓
Workflow
    ↓
Ontology Release
    ↓
Validation Evidence
    ↓
Deliverable Acceptance
    ↓
Goal Achievement
```

It also gives you a natural foundation for an agent-driven project system: an agent can inspect a goal, identify missing requirements, create tasks, generate plans, invoke reusable workflows, publish ontology releases, collect evidence, and update project progress—all while maintaining a consistent RDF knowledge graph.
