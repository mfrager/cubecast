# RDF vocabularies for state machines

State machines are a very good complement to the function-flow and planning ontologies. They represent something different: the control state of a computation, and the conditions under which execution moves between states.

For your monadic DSL, I would consider state machines a first-class part of the semantic model, not merely another type of workflow node.

There is one particularly important standard:

SCXML — W3C State Chart XML.

It provides a mature formal model for hierarchical, event-driven state machines, including compound states, parallel states, history states, guarded transitions, and executable actions.&#x20;

[image](https://www.google.com/s2/favicons?domain=https://www.w3.org\&sz=32)

W3C



## 1. Relevant vocabularies and standards

| Technology         | What it provides                                                  | RDF-native?                                        |
| ------------------ | ----------------------------------------------------------------- | -------------------------------------------------- |
| SCXML              | Hierarchical state machines, events, transitions, guards, actions | No, XML                                            |
| UML State Machines | State, transition, region, pseudostate, trigger, guard, effect    | No, UML model                                      |
| OWL / RDF          | Semantic definitions of states, events, transitions               | Yes, but no standard state-machine execution model |
| PROV-O             | Execution history and state-changing activities                   | Yes                                                |
| P-Plan             | Planned state transitions and execution correspondence            | Yes                                                |
| SHACL              | State and transition graph validation                             | Yes                                                |
| OWL-S              | Preconditions, effects, and process control constructs            | Yes                                                |

An important distinction: RDF is an atemporal graph model. It can describe state-machine definitions and snapshots of state, but does not itself provide transition execution semantics.&#x20;

[image](https://www.google.com/s2/favicons?domain=https://www.w3.org\&sz=32)

W3C+1



## 2. SCXML is the strongest semantic reference

Recommended state-machine model

SCXML has a comprehensive vocabulary of concepts:

| SCXML concept  | Meaning                           |
| -------------- | --------------------------------- |
| State          | A condition or mode of operation  |
| Initial state  | Starting state                    |
| Final state    | Completed state                   |
| Transition     | Movement between states           |
| Event          | Trigger for a transition          |
| Condition      | Guard evaluated before transition |
| Action         | Executable behavior               |
| Compound state | State containing substates        |
| Parallel state | Concurrent state regions          |
| History state  | Resume a previous configuration   |
| Entry action   | Executed upon entering a state    |
| Exit action    | Executed upon leaving a state     |

It also defines execution semantics, including event queues, transition selection, and run-to-completion processing.&#x20;

[image](https://www.google.com/s2/favicons?domain=https://www.w3.org\&sz=32)

W3C+1



[UML Statechart Diagrams](https://images.openai.com/static-rsc-4/Z1VIYl1PP-zlry70WXUrM62skla3n36TIAqPDzhP-qo1199NlvF0SIfY3PhRF5wCNsIguzD3zpoh7hgQc_c-B0HE28P5saoZahZ0Uhrlenhe8N4sKkMVzM_bdV7ajrYOz4IszDb5WMYEyrljE74d1sk1CvuAycQ0KaE8NTuFrc4?purpose=inline)

[currentaffairs2you.blogspot.com](https://currentaffairs2you.blogspot.com/2019/04/uml-statechart-diagrams.html?utm_source=chatgpt.com)

I would use SCXML as a reference for semantics, even if your canonical representation is RDF and your runtime is Rust.

## 3. Existing RDF state-machine ontologies

There are some RDF-native efforts worth knowing about, although none is as broadly established as SCXML.

### Protocol State Machine Ontology (PSM)

[Protocol State Machine Ontology — OWL specification](https://paul.ti.rw.fau.de/~jo00defe/ont/psm?utm_source=chatgpt.com)

This is a relatively recent OWL ontology, revision 0.0.1 from 2024, intended to model UML Protocol State Machines.

It is relevant for:

- State-machine structure.
- States and transitions.
- Protocol conformance.
- Modeling permitted behavior.

However, it is an early draft with limited demonstrated adoption. I would evaluate it as a reference rather than make it a core dependency.&#x20;

[image](https://www.google.com/s2/favicons?domain=https://paul.ti.rw.fau.de\&sz=32)

Paul+1



### Finite State Machine Ontology (FSM)

An older ontology describing:

- `StateMachine`
- `State`
- `Initial`
- `Final`
- `Transition`
- `Guard`
- `Event`
- `Action`
- `Composite`
- `Region`
- `History`
- `Pseudostate`

There is also research on semantic state machines for control systems and PLCs, including explicit active-state and region modeling. These are useful conceptual references, but the implementations are older and less suitable as a modern interoperability foundation.&#x20;

[image](https://www.google.com/s2/favicons?domain=https://journals.bg.agh.edu.pl\&sz=32)

journals.bg.agh.edu.pl+1



My assessment: There isn't a single RDF state-machine vocabulary I would confidently recommend as the universal foundation. SCXML is the better behavioral reference; a small RDF ontology is the better fit for your graph architecture.

## 4. Proposed RDF state-machine model

I'd create a custom `sm:` vocabulary, borrowing concepts and semantics from SCXML and UML.

```
@prefix sm: <https://example.org/ontology/state-machine#> .

# Core
sm:StateMachine
sm:State
sm:InitialState
sm:FinalState
sm:CompoundState
sm:ParallelState
sm:Region
sm:Transition

# Transition semantics
sm:Event
sm:Guard
sm:Effect
sm:Action
sm:EntryAction
sm:ExitAction

# Runtime
sm:MachineInstance
sm:StateConfiguration
sm:ActiveState
sm:TransitionExecution
sm:EventOccurrence
sm:StateSnapshot
```

Suggested relationships:

```
sm:hasState
sm:hasInitialState
sm:hasFinalState
sm:hasRegion
sm:hasTransition
sm:sourceState
sm:targetState
sm:triggeredBy
sm:hasGuard
sm:hasEffect
sm:hasEntryAction
sm:hasExitAction
sm:hasActiveState
sm:hasConfiguration
sm:hasHistory
sm:transitionPriority
```

The model should distinguish three things:

1. State-machine definition: the reusable graph of states and transitions.
2. Machine instance: a particular running state machine.
3. State configuration: the currently active state or combination of active states.

The third is especially important for hierarchical and parallel state machines. A machine can have multiple simultaneously active states, so a single `currentState` property is insufficient.

## 5. Example: state machine controlling a monadic flow

Imagine a document-processing agent:

## Document processing state machine

READY

start event

EXTRACTING

MinerU extraction function flow

Success

INDEXING

COMPLETE

Failure

RECOVERY

FAILED

Here, `EXTRACTING` can execute an entire monadic function flow:

```
parse_pdf(path) >> mineru_extract() >> normalize() >> validate()
```

The state machine controls when that flow starts, what happens when it succeeds, and what happens when it fails.

This gives you two complementary structures:

- A dataflow graph describing how functions connect.
- A state-transition graph describing how the overall process progresses.

They should be connected, but not collapsed into one graph.

## 6. How the ontologies fit together

For the larger system, I would now use four complementary models:

Planning ontology

P-Plan + custom planning vocabulary

What should happen, and what outcomes are desired?

Function ontology

FnO + Wfdesc + custom monadic-flow vocabulary

Which functions are composed, and how does data flow between them?

State-machine ontology

Custom SM ontology based on SCXML/UML

What state is the process in, and which transitions are enabled?

Execution ontology

PROV-O + WFPROV

What actually executed, and what were the results?

## 7. A useful extension: Petri nets

I would also consider Petri nets.

They are particularly useful when your agent system needs:

- Multiple concurrent tasks.
- Resource allocation.
- Synchronization barriers.
- Waiting for multiple results.
- Mutual exclusion.
- Deadlock detection.

Unlike ordinary state machines, Petri nets use places, transitions, arcs, and tokens. Tokens represent available resources or current execution markings.

For example, a task requiring two independent extraction jobs can wait until both corresponding places contain tokens before continuing.

This is worth keeping in mind if your monadic flows become concurrent or distributed. RDF can describe a Petri net, but the execution semantics belong in the runtime.

## 8. Final recommendation for the Rust/Oxigraph implementation

I would build these as separate, interoperable graph models:

| Model                | Core abstraction                 |
| -------------------- | -------------------------------- |
| Planning             | Goal → Plan → Step               |
| Function flow        | Function → Node → Connection     |
| State machine        | State → Transition → Event       |
| Execution            | Activity → Input/Output → Result |
| Petri net (optional) | Place → Transition → Token       |

One additional recommendation: make state-machine transitions capable of invoking function flows.

For example:

```
Transition:
  source: READY
  event: START
  target: EXTRACTING
  effect:
    invoke: document_extraction_flow
```

Likewise, a function-flow completion can emit an event:

```
Flow completion:
  success -> EXTRACTION_SUCCEEDED
  failure -> EXTRACTION_FAILED
```

That provides a clean event-driven boundary between the two execution models.

For the Rust implementation, I'd keep the ontology as the descriptive layer and implement a typed state-machine IR with explicit transition selection, guards, event queues, state configurations, and effects. Use SCXML as the behavioral compatibility target, without requiring the runtime itself to execute XML.

The overall architecture becomes a planner that constructs function flows, state machines that control those flows, and an execution engine that records everything through PROV-O.
