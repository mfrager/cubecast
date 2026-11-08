# RDF vocabularies for monadic function flows

This is a more specific problem than general workflow planning. For a flow such as:

```
load_document(path) >> extract_text() >> normalize() >> store_graph()
```

you want RDF to describe each function as a node, the composition as a graph, and the connections as typed dataflow edges.

There is a particularly relevant vocabulary for this: FnO (Function Ontology), including its function-composition extension, FnO Composition.

I would put FnO at the center of the design, supplemented by workflow vocabularies and a small custom ontology for monadic semantics.

## 1. Function Ontology (FnO)

Strongest match

Specification: [Function Ontology](https://fno.io/spec/?utm_source=chatgpt.com)

Namespace:

```
@prefix fno:  <https://w3id.org/function/ontology#> .
@prefix fnoc: <https://w3id.org/function/vocabulary/composition#> .
```

FnO distinguishes several things that are important for a function-flow system:

| FnO concept               | Meaning in a function flow                     |
| ------------------------- | ---------------------------------------------- |
| `fno:Function`            | Abstract function definition                   |
| `fno:Parameter`           | Named input parameter                          |
| `fno:Output`              | Named output                                   |
| `fno:Execution`           | Invocation with actual input and output values |
| `fno:Implementation`      | Concrete implementation                        |
| `fno:Mapping`             | Connects abstract function to implementation   |
| `fnoc:Composition`        | Composition of multiple functions              |
| `fnoc:CompositionMapping` | Connection between parameters and outputs      |

The particularly useful part is `fnoc:Composition`: it describes how the output of one function maps into the input of another. It also supports mapping external inputs and constants into constituent functions.&#x20;

[image](https://www.google.com/s2/favicons?domain=https://fno.io\&sz=32)

FnO+1



This is almost exactly the graph representation you need.

For example:

```
read_file(path) >> parse_document() >> extract_entities()
```

Can be represented as:

```
Composition
  |
  +-- Function: read_file
  |      |
  |      +-- output: document_bytes
  |
  +-- Function: parse_document
  |      |
  |      +-- input: document_bytes
  |      +-- output: parsed_document
  |
  +-- Function: extract_entities
         |
         +-- input: parsed_document
         +-- output: entities
```

However, FnO does not define monadic bind, context propagation, short-circuiting, or error-channel semantics. Those need an extension.

## 2. Wfdesc — representing the actual flow graph

Best match for graph topology

Specification: [Wf4Ever workflow description ontology](https://wf4ever.github.io/ro/?utm_source=chatgpt.com)

Namespace:

```
@prefix wfdesc: <http://purl.org/wf4ever/wfdesc#> .
```

Wfdesc represents a workflow as a directed graph:

| Class              | Meaning                             |
| ------------------ | ----------------------------------- |
| `wfdesc:Workflow`  | The complete flow                   |
| `wfdesc:Process`   | A function or processing node       |
| `wfdesc:Parameter` | A node's input/output parameter     |
| `wfdesc:Input`     | Input port                          |
| `wfdesc:Output`    | Output port                         |
| `wfdesc:Artifact`  | Type or description of data         |
| `wfdesc:DataLink`  | Connection between output and input |

Important relationships:

- `wfdesc:hasSubProcess`
- `wfdesc:hasInput`
- `wfdesc:hasOutput`
- `wfdesc:hasDataLink`
- `wfdesc:hasSource`
- `wfdesc:hasSink`
- `wfdesc:hasArtifact`

This is a very natural representation of a function pipeline. Each process is a node, and each DataLink connects the output port of one node to the input port of another. It also supports nested workflows.&#x20;

[image](https://www.google.com/s2/favicons?domain=https://models.cellml.org\&sz=32)

Physiome Model Repository+1



For your use case, Wfdesc has an advantage over FnO: it explicitly models the graph topology, independently of how the functions are implemented.

## 3. Other useful vocabularies

| Vocabulary | Relevance                                                          |
| ---------- | ------------------------------------------------------------------ |
| FnO        | Function definitions, signatures, implementations and compositions |
| Wfdesc     | Workflow nodes, ports and dataflow edges                           |
| WFPROV     | Actual workflow and process executions                             |
| PROV-O     | Execution provenance, agents, inputs and outputs                   |
| P-Plan     | Planned steps and correspondence to execution                      |
| OWL-S      | Action inputs, outputs, preconditions and effects                  |
| SHACL      | Validate function signatures and flow connections                  |

I would prioritize FnO, Wfdesc and WFPROV. They have complementary responsibilities rather than being competing alternatives.

## 4. The important distinction: ordinary function composition versus monadic composition

Consider your Python-like syntax:

```
load_document(path) >> extract_text() >> normalize() >> store_graph()
```

There are two different things to represent.

Function composition describes the connection between outputs and inputs.

Monadic composition describes how the computational context is carried from one function to the next.

For example, a function might have a signature like:

```
fn load_document(path: Path) -> Result<Document, Error>
```

And the next function:

```
fn extract_text(document: Document) -> Result<String, Error>
```

The `>>` operation is not simply connecting `Document` to `String`. It also propagates the `Result` context, including errors.

Other possible contexts include:

- `Option<T>` — optional results and short-circuiting.
- `Result<T, E>` — errors and failure propagation.
- `Future<T>` — asynchronous execution.
- `State<S, T>` — state threading.
- `Reader<C, T>` — shared execution context.
- `Writer<W, T>` — accumulated output or logs.
- `Stream<T>` — sequences of values.
- `Validation<T, E>` — accumulated validation errors.

FnO can describe the function signature and composition, but does not formally capture these monadic semantics.

I would create a small custom extension for this.

## 5. Proposed monadic flow ontology

I would introduce a namespace:

```
@prefix mflow: <https://example.org/ontology/monadic-flow#> .
```

The key classes would be:

| Class                     | Purpose                            |
| ------------------------- | ---------------------------------- |
| `mflow:Flow`              | Complete composed computation      |
| `mflow:Node`              | Individual invocation in the flow  |
| `mflow:Function`          | Function definition, linked to FnO |
| `mflow:Port`              | Typed input/output port            |
| `mflow:Connection`        | Connection between ports           |
| `mflow:Context`           | Computational context or effect    |
| `mflow:ContextTransition` | How context is propagated          |
| `mflow:Composition`       | Composition operation              |
| `mflow:Branch`            | Conditional execution              |
| `mflow:FailureHandler`    | Error handling                     |
| `mflow:Execution`         | Concrete invocation                |

Suggested relationships:

```
mflow:hasNode
mflow:hasInput
mflow:hasOutput
mflow:connects
mflow:source
mflow:target
mflow:usesFunction
mflow:hasContext
mflow:hasContextTransition
mflow:composesWith
mflow:hasExecution
mflow:next
```

The important thing is that a node should have both a function signature and a context signature.

For example:

```
Node A:
  Function: load_document
  Input: Path
  Output: Document
  Context: Result<Document, Error>

Node B:
  Function: extract_text
  Input: Document
  Output: String
  Context: Result<String, Error>
```

The flow connection would be:

```
A.output.value ──────> B.input.value
A.context.Result ────> B.context.Result
```

The first connection carries the actual value. The second describes the computational context.

These are conceptually different connections, even when the runtime implements them through a single `and_then` operation.

## 6. Example RDF representation

Here's an illustrative representation of a two-node flow.

```
@prefix mflow: <https://example.org/ontology/monadic-flow#> .
@prefix fno:   <https://w3id.org/function/ontology#> .
@prefix wfdesc: <http://purl.org/wf4ever/wfdesc#> .
@prefix xsd:   <http://www.w3.org/2001/XMLSchema#> .

ex:flow
    a mflow:Flow, wfdesc:Workflow ;
    mflow:hasNode ex:loadNode, ex:extractNode ;
    mflow:hasContext ex:resultContext .

ex:loadDocument
    a fno:Function ;
    fno:name "load_document" .

ex:extractText
    a fno:Function ;
    fno:name "extract_text" .

ex:loadNode
    a mflow:Node, wfdesc:Process ;
    mflow:usesFunction ex:loadDocument .

ex:extractNode
    a mflow:Node, wfdesc:Process ;
    mflow:usesFunction ex:extractText .

ex:resultContext
    a mflow:Context ;
    mflow:contextType "Result" .

ex:loadToExtract
    a wfdesc:DataLink, mflow:Connection ;
    wfdesc:hasSource ex:loadOutput ;
    wfdesc:hasSink ex:extractInput .
```

The actual port resources would describe their respective value types and context types. The example deliberately leaves those details abbreviated.

## 7. A further distinction: node definition versus node invocation

I strongly recommend three separate layers:

1\. Function definition

`fno:Function`

Reusable definition, signature, implementation and semantic description.

2\. Flow node

`mflow:Node` / `wfdesc:Process`

A particular occurrence of a function inside a composition, with bound inputs and connections.

3\. Execution instance

`fno:Execution` / `wfprov:ProcessRun`

An actual invocation with runtime values, results, errors and timing.

This means you can reuse the same function in multiple flows, invoke it multiple times in one flow, and track every invocation independently.

## 8. My recommended combination

| Requirement                       | Use                                  |
| --------------------------------- | ------------------------------------ |
| Function definition and signature | FnO                                  |
| Function implementation mapping   | FnO                                  |
| Function composition              | FnO Composition                      |
| DAG and dataflow topology         | Wfdesc                               |
| Runtime execution provenance      | WFPROV + PROV-O                      |
| Monadic context semantics         | Custom MFlow                         |
| Type constraints                  | RDF datatypes + custom type ontology |
| Graph validation                  | SHACL                                |

One caveat: FnO's composition extension is explicitly marked experimental in its draft specification, which also has no formal standards-body status. I would use it as an interoperability model, not make it an unchangeable internal representation.&#x20;

[image](https://www.google.com/s2/favicons?domain=https://fno.io\&sz=32)

FnO+1



## 9. A particularly useful design for your Rust system

I would go one step further and make the RDF ontology describe a typed functional IR, rather than merely a workflow diagram.

For example:

```
load_document(path) >> extract_text() >> normalize() >> store_graph()
```

Could become an internal representation like:

```
Flow
  Node 0: load_document
    Input: Path
    Output: Result<Document, Error>

  Node 1: extract_text
    Input: Document
    Output: Result<Text, Error>

  Node 2: normalize
    Input: Text
    Output: Result<NormalizedText, Error>

  Node 3: store_graph
    Input: NormalizedText
    Output: Result<GraphId, Error>
```

And each connection would have:

```
Connection {
    source_node,
    source_port,
    target_node,
    target_port,
    value_type,
    context_mapping
}
```

I would actually use two graph representations:

- Value graph: which outputs feed which inputs.
- Control/context graph: how execution context, branching, errors, state, and asynchronous behavior propagate.

This distinction makes it possible to inspect, validate, optimize, transform, and execute a flow without confusing its data dependencies with its computational semantics.

My strongest recommendation: use FnO + Wfdesc as the external RDF foundation, and design MFlow as a small ontology specifically for typed monadic composition. The resulting graph can serve as the semantic representation of your DSL, while a compact Rust IR handles actual execution.
