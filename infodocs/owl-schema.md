# OWL Schema (`ontology-owl`)

`ontology-owl` stores managed OWL classes, properties, and ontology definitions
as RDF in `system:schema`, and provides hierarchy queries and structural
validation.

Source: `crates/ontology-owl/src/`.

## Responsibilities

- CRUD for `OwlClass` and `OwlProperty`.
- CRUD for `Ontology` and recorded ontology versions.
- Direct and transitive subclass/superclass queries.
- Applicable-property and applicable-class queries.
- Structural validation of a schema graph.
- Encoding/decoding between the domain structs and RDF quads.

It does not implement a general OWL reasoner. Inference is a separate subsystem
([inference.md](inference.md)). It does not validate on write; validation is an
explicit operation.

## Manager setup

```rust
let manager = OwlManager::new(store);                 // system:schema
let manager = OwlManager::with_graph(store, graph);   // custom graph
manager.graph();                                      // active graph
```

`OwlManager` implements `ontology_core::owl::OntologyManager`. Reads and writes
go through `RdfStore`, never Oxigraph directly.

## Domain model

### `OwlClass`

Fields:

- `id: Id`
- `labels: BTreeMap<String, String>` keyed by language tag (`""` untagged)
- `comments: BTreeMap<String, String>`
- `sub_class_of: Vec<Id>` — direct superclasses
- `equivalent_classes: Vec<Id>`
- `disjoint_with: Vec<Id>`
- `restrictions: Vec<Restriction>`
- `deprecated: bool`

`Restriction` supports the common restriction forms represented by the
implementation (property, optional filler class, and restriction kind). Its
`property()` and `class()` accessors are used by validation and applicable-class
queries.

### `OwlProperty`

Fields:

- `id`, `kind` (`Object`, `Datatype`, `Annotation`)
- localized `labels` and `comments`
- `sub_property_of`, `equivalent_properties`
- `domain`, `range`
- characteristics such as functional, inverse-functional, transitive,
  symmetric, asymmetric, reflexive, and irreflexive
- `inverse_of`
- `deprecated`

The exact characteristic set is defined in `ontology_core::owl`; unknown
characteristics cannot be represented.

### `Ontology`

Fields: `id`, localized labels/comments, `version_info`, `version_iri`,
`imports`.

### `OntologyVersion`

Fields: `ontology`, optional `version_info`, optional `version_iri`,
`recorded_at`.

## RDF encoding

All definitions use the manager's graph. The standard encoding is:

- class: `rdf:type owl:Class`, labels (`rdfs:label`), comments
  (`rdfs:comment`), `rdfs:subClassOf`, `owl:equivalentClass`,
  `owl:disjointWith`, and blank-node restrictions;
- property: `rdf:type owl:ObjectProperty` / `owl:DatatypeProperty` /
  `owl:AnnotationProperty`, labels/comments, `rdfs:subPropertyOf`,
  `owl:equivalentProperty`, `rdfs:domain`, `rdfs:range`, `owl:inverseOf`, and
  characteristic type triples;
- ontology: `rdf:type owl:Ontology`, labels/comments, `owl:versionInfo`,
  `owl:versionIRI`, `owl:imports`;
- version record: a separate subject linked to the ontology with
  `dcterms:isVersionOf`, carrying version info and a timestamp.

Encoding is centralized in `encoding.rs`; the manager never constructs ad-hoc
schema triples.

## Operations

### Classes

| Operation | Behavior |
| --- | --- |
| `create_class` | Fails with `Conflict` if the class already exists; inserts all class quads |
| `get_class` | `NotFound` if absent |
| `list_classes(ClassQuery)` | Filters by direct parent and case-insensitive label substring; applies limit/offset |
| `update_class` | Requires existing class; removes the class's encoded quads and inserts the replacement in one transaction |
| `delete_class` | Removes the class's encoded quads; `NotFound` if absent |
| `hierarchy(id)` | Direct superclasses/subclasses plus transitive ancestors/descendants |

`ClassQuery` has `parent: Option<Id>`, `label_contains: Option<String>`, `limit`,
and `offset`. `limit == 0` returns an empty list.

Hierarchy traversal builds a map of all classes in the graph. It is cycle-safe
(visited set), so a malformed hierarchy cannot loop forever. A class can be
created with a cycle; `validate` reports it rather than blocking the write.

### Properties

`create/get/list/update/delete` mirror classes. `PropertyQuery` filters by
`PropertyKind`, case-insensitive label substring, limit, and offset.

`update_property` removes every quad whose subject is the property IRI in the
schema graph, then inserts the new encoding. `delete_property` does the same
removal. This is broader than the class-specific removal helper, so any
non-property statements attached to that subject in the schema graph are also
removed.

### Ontologies and versions

- `create_ontology` rejects duplicates and records an initial version.
- `update_ontology` replaces the ontology definition and records another
  version with the current timestamp.
- `delete_ontology` removes the ontology, its version records, and version
  subjects linked by `dcterms:isVersionOf`.
- `ontology_versions` returns versions ordered by `recorded_at`.
- `list_ontologies` returns sorted by IRI.

Version records are append-only for normal creates/updates. Replacing a version
is not exposed as a separate operation.

### Applicable properties and classes

`applicable_properties(class)`:

1. Loads all classes and resolves the requested class.
2. Computes its transitive ancestors plus itself.
3. Returns properties whose `domain` is empty (unconstrained) or contains one
   of those classes.

This is a domain-based view, not full OWL reasoning over property restrictions.

`applicable_classes(properties)` returns classes with at least one restriction
whose property is in the supplied set.

## Validation

`validate(scope)` resolves `GraphScope::Schema` to the manager's graph or
`GraphScope::Graph(graph)` to a specific graph, loads all classes/properties,
and returns a `ValidationReport`.

Findings:

| Code | Severity | Trigger |
| --- | --- | --- |
| `unknown-superclass` | warning | `subClassOf` target not declared as `owl:Class` |
| `unknown-equivalent-class` | warning | equivalent class not declared |
| `disjoint-with-self` | error | class disjoint with itself |
| `unknown-disjoint-class` | warning | disjoint target not declared |
| `unknown-restriction-property` | warning | restriction references undeclared property |
| `unknown-restriction-class` | warning | restriction filler not declared |
| `subclass-cycle` | error | class is its own ancestor |
| `unknown-domain-class` | warning | object-property domain not declared |
| `unknown-range-class` | warning | object-property range not declared |
| `unknown-inverse-property` | warning | inverse property not declared |
| `non-datatype-range` | warning | datatype-property range is not an XSD datatype |

`ValidationReport::is_valid()` is false only for error-severity issues.

Validation is local to the selected graph. Cross-graph references are therefore
reported as unknown unless both sides live in the validated graph.

## Errors and invariants

- Duplicate class/property/ontology → `Conflict`.
- Missing class/property/ontology → `NotFound`.
- Updates use a store transaction where replacement must be atomic.
- IRI syntax is enforced by `Id` before storage.
- Class hierarchy traversal tolerates cycles; validation reports them.
- No implicit deletion of subclasses when deleting a parent.
- No `owl:imports` resolution; imports are metadata only.

## Tests

`tests/tests/phase2_owl.rs` covers class/property CRUD, hierarchy, validation,
restrictions, ontology versions, and applicable queries. The full workspace run
includes these tests.

```sh
cargo test -p ontology-owl
cargo test -p ontology-integration-tests --test phase2_owl
```

## Limitations

- No general OWL DL reasoning or consistency checking.
- No automatic validation on create/update.
- No cascade delete for references or subclasses.
- Validation only checks the selected graph.
- Property update/delete remove all statements for the property subject in the
  schema graph.

## Related

- Core types: [core-domain.md](core-domain.md)
- Inference using the schema: [inference.md](inference.md)
- HTTP routes: [http-api.md](http-api.md)
