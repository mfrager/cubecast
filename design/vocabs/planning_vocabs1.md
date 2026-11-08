# RDF vocabularies for planning

There isn't one universally adopted RDF ontology that covers the full planning problem—from goals and constraints through actions, dependencies, execution, monitoring, and replanning.

The strongest approach is to combine a few established vocabularies, each responsible for a different layer:

1. Goals and desired states — what should be achieved.
2. Plans and tasks — what needs to happen.
3. Actions and operators — what can happen, including preconditions and effects.
4. Dependencies and ordering — what must happen before what.
5. Execution and provenance — what actually happened.
6. Evaluation and replanning — whether the result satisfies the goal and what must change.

A key distinction: a plan is an intended specification; an execution is a particular attempt to carry it out. The RDF model should preserve that distinction.

## 1. The most relevant existing vocabularies

### P-Plan — best starting point for representing plans

Best fit for plan structure

P-Plan (Ontology for Provenance and Plans) extends PROV-O to describe an abstract plan and connect it to the execution that followed it.

Namespace: `http://purl.org/net/p-plan#`

Its core concepts are:

| Term                           | Meaning                                            |
| ------------------------------ | -------------------------------------------------- |
| `p-plan:Plan`                  | A plan composed of steps                           |
| `p-plan:Step`                  | A planned activity                                 |
| `p-plan:Variable`              | An input or output used by steps                   |
| `p-plan:hasInputVar`           | Input variable of a step                           |
| `p-plan:hasOutputVar`          | Output variable of a step                          |
| `p-plan:isStepOfPlan`          | Associates a step with its plan                    |
| `p-plan:isPrecededBy`          | Expresses step dependency                          |
| `p-plan:correspondsToStep`     | Connects an executed activity to its planned step  |
| `p-plan:correspondsToVariable` | Connects an execution entity to a planned variable |

P-Plan makes a particularly useful distinction: a planned step is not the same thing as an execution activity. One abstract step can be implemented differently in different executions. A planned step may also have no corresponding execution activity if execution fails or never reaches it.&#x20;

[image](https://www.google.com/s2/favicons?domain=https://www.opmw.org\&sz=32)

opmw\.org+1



Example:

```
@prefix p-plan: <http://purl.org/net/p-plan#> .
@prefix prov:   <http://www.w3.org/ns/prov#> .
@prefix ex:     <https://example.org/> .

ex:plan
    a p-plan:Plan, prov:Plan .

ex:research
    a p-plan:Step ;
    p-plan:isStepOfPlan ex:plan .

ex:analyze
    a p-plan:Step ;
    p-plan:isStepOfPlan ex:plan ;
    p-plan:isPrecededBy ex:research .

ex:sourceData
    a p-plan:Variable ;
    p-plan:isVariableOfPlan ex:plan .

ex:research
    p-plan:hasInputVar ex:sourceData .
```

Assessment: Excellent for workflow decomposition, data dependencies, and plan-to-execution correspondence. It does not provide a rich model of goals, action semantics, resource allocation, decision-making, or general-purpose planning algorithms.

One implementation detail: older P-Plan documentation uses the misspelling `isPreceededBy`; the current vocabulary spelling is `isPrecededBy`. Check the ontology version you import.&#x20;

[image](https://www.google.com/s2/favicons?domain=https://www.opmw.org\&sz=32)

opmw\.org+1



### PROV-O — plan identity, agents, and execution history

Core interoperability vocabulary

Namespace: `http://www.w3.org/ns/prov#`

PROV-O defines:

- `prov:Plan` — an entity representing intended actions or steps to achieve goals.
- `prov:Activity` — something that actually occurred.
- `prov:Entity` — something used or generated.
- `prov:Agent` — a person, organization, or software agent.
- `prov:wasAssociatedWith` — associates an activity with an agent.
- `prov:used` — identifies entities used by an activity.
- `prov:wasGeneratedBy` — identifies the activity that generated an entity.
- `prov:hadPlan` — associates an activity with the plan followed.

The important limitation is deliberate: PROV-O defines `prov:Plan` but does not prescribe how its steps, goals, or internal structure are represented. P-Plan fills much of that gap.&#x20;

[image](https://www.google.com/s2/favicons?domain=https://www.w3.org\&sz=32)

W3C+1



### OPMW — workflow templates and concrete workflow executions

Namespace: `http://www.opmw.org/ontology/`

OPMW extends P-Plan and PROV-O with a more specific model for scientific workflows.

Notable concepts:

- `opmw:WorkflowTemplate`
- `opmw:WorkflowTemplateProcess`
- `opmw:WorkflowTemplateArtifact`
- `opmw:WorkflowExecutionProcess`
- `opmw:WorkflowExecutionArtifact`
- `opmw:correspondsToTemplateProcess`

It describes reusable workflow templates, their inputs and outputs, and their concrete executions. It is useful when workflows are primarily data-processing pipelines, especially scientific or reproducible workflows.&#x20;

[image](https://www.google.com/s2/favicons?domain=https://www.opmw.org\&sz=32)

opmw\.org+1



Assessment: More specialized than P-Plan. I would not make it the general planning foundation, but it is worth supporting as an interoperability mapping for workflow-oriented plans.

### OWL-S — actions, preconditions, effects, and composite processes

Best reference for action semantics

OWL-S is a semantic description framework for services and processes.

Namespace: `http://www.daml.org/services/owl-s/1.2/`

Its process model provides:

- Atomic processes
- Simple processes
- Composite processes
- Inputs and outputs
- Preconditions
- Results
- Effects
- Control constructs such as sequence, choice, split, and iteration

The particularly useful idea is its IOPE model:

| Element       | Planning meaning                           |
| ------------- | ------------------------------------------ |
| Inputs        | Information required to perform an action  |
| Outputs       | Information produced                       |
| Preconditions | Conditions that must hold before execution |
| Effects       | Changes caused by execution                |

OWL-S explicitly distinguishes an action's information outputs from its effects on the world. For example, a purchase action might return an order confirmation while also changing ownership or inventory state.&#x20;

[image](https://www.google.com/s2/favicons?domain=https://www.w3.org\&sz=32)

W3C+1



Assessment: A useful design reference for an action/operator ontology. It is service-oriented, relatively heavyweight, and not a complete general-purpose goal-based planning model.

### Other relevant vocabularies

| Vocabulary                | What it contributes                                                             | Suitability                          |
| ------------------------- | ------------------------------------------------------------------------------- | ------------------------------------ |
| DUL — Dolce+DnS Ultralite | Foundational concepts for situations, tasks, plans, agents, objects, and events | Semantic foundation                  |
| Schema.org                | `schema:Action` and related action types, agents, objects, targets, results     | Broad web interoperability           |
| BPMN ontology             | Tasks, events, gateways, sequence flows, and business-process structure         | Business-process modeling            |
| ODRL                      | Permissions, prohibitions, duties, and policy constraints                       | Policy and authorization constraints |
| SHACL                     | Graph validation, cardinalities, value constraints, and structural rules        | Validation, not planning semantics   |

BPMN is particularly useful if you need to exchange plans with business-process tools, but it models process flow rather than general goal-directed planning. ODRL is useful for constraints that govern what an agent may or must do.&#x20;

[image](https://www.google.com/s2/favicons?domain=https://onlinelibrary.wiley.com\&sz=32)

Wiley Online Library+1



## 2. What is missing from these vocabularies?

For a general-purpose planner, particularly one that can plan for software agents, business processes, research tasks, or autonomous systems, there are several important concepts that the existing vocabularies do not adequately cover.

| Planning concept  | Recommended representation                     |
| ----------------- | ---------------------------------------------- |
| Goal              | Custom `plan:Goal`                             |
| Subgoal           | `plan:Goal` with decomposition relationships   |
| Desired state     | `plan:StateSpecification`                      |
| Current state     | State facts in the knowledge graph             |
| Action definition | Custom `plan:Action`                           |
| Action instance   | `p-plan:Step`                                  |
| Preconditions     | `plan:Precondition`                            |
| Effects           | `plan:Effect`                                  |
| Action parameters | `p-plan:Variable` or typed parameter resources |
| Constraints       | Custom `plan:Constraint`, optionally ODRL      |
| Dependencies      | P-Plan dependency relationships                |
| Alternative plans | Custom `plan:PlanAlternative`                  |
| Plan evaluation   | Custom `plan:Evaluation`                       |
| Execution         | `prov:Activity`                                |
| Execution outcome | `prov:Entity`                                  |
| Agent             | `prov:Agent`                                   |

I would not try to force all of these into P-Plan or OWL-S. They solve different parts of the problem.

In particular, a goal is not a task, a task is not an action, and an action definition is not an action execution.

These distinctions are important for planning and reasoning.

## 3. Recommended ontology architecture

For a general-purpose planning engine, I would use a small application ontology on top of P-Plan and PROV-O, borrowing action semantics from OWL-S.

## Planning ontology stack

Goal and intention layer

`plan:Goal`, `plan:Objective`, `plan:DesiredState`, `plan:Constraint`

Why the planner is acting and what success means.

Plan layer — P-Plan

`p-plan:Plan`, `p-plan:Step`, `p-plan:Variable`

Decomposition into steps, dependencies, and data flow.

Action semantics — OWL-S-inspired

`plan:Action`, `plan:Precondition`, `plan:Effect`, `plan:Parameter`

What actions do, what they require, and how they change state.

Execution and provenance — PROV-O

`prov:Activity`, `prov:Entity`, `prov:Agent`

What actually happened, who did it, and what was produced.

Validation and reasoning

SHACL, SPARQL, application rules, and a planning engine

Check feasibility, evaluate outcomes, and generate revised plans.

The key is to make the custom ontology relatively small. Reuse existing terms whenever their semantics actually match.

## 4. Proposed custom planning vocabulary

I would define a namespace such as:

```
@prefix plan: <https://example.org/ontology/planning#> .
```

The following is a proposed application ontology, not an existing standard.

### Goals

```
plan:Goal
plan:Objective
plan:DesiredState
plan:GoalDecomposition
plan:GoalEvaluation
```

Relationships:

```
plan:hasGoal
plan:hasSubgoal
plan:contributesTo
plan:requiresGoal
plan:achievesGoal
plan:hasDesiredState
plan:hasEvaluation
```

Goals should describe desired conditions, not prescribe the steps required to achieve them.

A goal might be:

```
ex:goal
    a plan:Goal ;
    plan:hasDesiredState ex:desiredState .
```

The desired state should be machine-evaluable where possible.

### Actions

```
plan:Action
plan:ActionParameter
plan:Precondition
plan:Effect
plan:Condition
plan:StateTransition
```

Relationships:

```
plan:hasPrecondition
plan:hasEffect
plan:hasParameter
plan:hasInput
plan:hasOutput
plan:requiresCapability
plan:transformsState
```

An action is a reusable definition. A plan step references an action definition and binds its parameters.

For example:

```
ex:downloadFile
    a plan:Action ;
    plan:hasPrecondition ex:networkAvailable ;
    plan:hasEffect ex:fileAvailable ;
    plan:requiresCapability ex:httpClient .
```

The actual action invocation would be represented separately:

```
ex:downloadStep
    a p-plan:Step ;
    plan:instantiatesAction ex:downloadFile ;
    p-plan:isStepOfPlan ex:plan .
```

This lets the same action definition be reused in many plans.

### Constraints

```
plan:Constraint
plan:ResourceConstraint
plan:TemporalConstraint
plan:CapabilityConstraint
plan:PolicyConstraint
plan:RiskConstraint
```

Relationships:

```
plan:hasConstraint
plan:constrainsAction
plan:constrainsPlan
plan:hasSeverity
plan:hasConstraintExpression
```

Keep constraints separate from preconditions:

- A precondition determines whether an action is currently applicable.
- A constraint determines whether a plan or action is acceptable.
- An effect describes what changes after an action.

For example, an action may be executable but violate a budget constraint.

### Plan alternatives and evaluation

```
plan:PlanAlternative
plan:PlanEvaluation
plan:PlanObjective
plan:EvaluationCriterion
plan:PlanScore
plan:PlanRevision
```

Relationships:

```
plan:alternativeTo
plan:hasAlternative
plan:hasScore
plan:hasCriterion
plan:hasRevision
plan:supersedesPlan
```

This provides a place for a planner to retain competing plans, compare them, and explain why one was selected.

## 5. Important modeling decision: represent state separately

I would avoid representing the entire world state as a collection of plan-specific properties.

Instead, model state as knowledge graph facts and let planning actions describe state transitions.

For example:

```
ex:server
    ex:hasStatus ex:Offline .

ex:startServer
    a plan:Action ;
    plan:hasPrecondition ex:serverOffline ;
    plan:hasEffect ex:serverOnline .

ex:serverOffline
    a plan:Condition ;
    plan:conditionExpression
        "server.status == Offline" .

ex:serverOnline
    a plan:Condition ;
    plan:conditionExpression
        "server.status == Online" .
```

The expression syntax above is illustrative; it is not a standardized RDF expression language.

For an actual planning engine, I would define a typed expression representation rather than rely on arbitrary strings. This could support:

- Equality and inequality.
- Boolean conjunction and disjunction.
- Numeric comparisons.
- Resource availability.
- Entity relationships.
- Quantified conditions.
- State transition expressions.

The ontology stores the representation. The planner evaluates it.

This separation also allows the planner to use symbolic reasoning, constraint solving, or search algorithms without requiring RDF itself to perform those operations.

## 6. Relationship to established planning systems

There is another important distinction: RDF vocabularies describe knowledge, but they do not automatically provide a planning algorithm.

| Planning technology    | Main contribution                                                |
| ---------------------- | ---------------------------------------------------------------- |
| PDDL                   | Formal planning problems, actions, preconditions, effects, goals |
| HTN planning           | Hierarchical decomposition of tasks into subtasks                |
| STRIPS                 | State-transition planning with preconditions and effects         |
| Temporal planning      | Durations, scheduling, concurrency, temporal constraints         |
| Constraint programming | Resource, scheduling, and feasibility constraints                |
| Graph planning         | Dependency and reachability reasoning                            |

I would particularly look at PDDL and HTN planning when designing the action and goal model.

PDDL offers a well-established formal representation for symbolic planning. HTN planning is especially relevant when tasks are naturally decomposed into increasingly specific subtasks.

RDF should be the semantic knowledge representation and interchange layer; the planning engine can translate appropriate portions of the graph into its internal planning representation.

## 7. My recommendation

For the Rust/Oxigraph knowledge system, I would adopt this combination:

| Layer                     | Vocabulary / technology          | Role                                    |
| ------------------------- | -------------------------------- | --------------------------------------- |
| Goals                     | Custom `plan:`                   | Desired states and objectives           |
| Plan decomposition        | P-Plan                           | Plans, steps, variables, dependencies   |
| Action semantics          | Custom `plan:` informed by OWL-S | Preconditions, effects, parameters      |
| Execution                 | PROV-O                           | Actual activities, agents, and outcomes |
| Workflow interoperability | OPMW                             | Import/export of workflow templates     |
| Policies                  | ODRL                             | Permissions, prohibitions, obligations  |
| Validation                | SHACL                            | Structural and semantic constraints     |
| Planning algorithms       | PDDL / HTN / custom Rust         | Search, decomposition, scheduling       |

I would not adopt OWL-S wholesale, nor build the entire system around OPMW. P-Plan and PROV-O give you a useful foundation, while a small custom ontology supplies the missing concepts needed for general-purpose planning.

One additional design decision: model a plan as an immutable or versioned entity. Replanning should generate a new plan revision rather than silently mutate the original. This makes it possible to compare intended plans, actual executions, deviations, and subsequent decisions.

### Primary references

- [W3C PROV-O specification](https://www.w3.org/TR/prov-o/?utm_source=chatgpt.com)
- [P-Plan ontology specification](https://www.opmw.org/model/p-plan/?utm_source=chatgpt.com)
- [P-Plan classes and properties](https://www.opmw.org/model/p-plan/?utm_source=chatgpt.com)
- [OWL-S specification](https://www.w3.org/submissions/OWL-S/?utm_source=chatgpt.com)
- [OPMW ontology specification](https://www.opmw.org/model/OPMW/?utm_source=chatgpt.com)
- [BPMN semantic ontology research](https://onlinelibrary.wiley.com/doi/10.1002/spe.3184?utm_source=chatgpt.com)

The architecture I would pursue is RDF as the semantic planning knowledge base, a typed symbolic IR as the planner's working representation, and Rust as the execution and reasoning engine. This keeps the ontology interoperable without forcing every planning operation into SPARQL or OWL reasoning.
