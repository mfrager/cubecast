# Inference and Enrichment (`ontology-inference`)

`ontology-inference` derives candidate classes and properties from OWL schema,
external mappings, approved classifications, and attribute rules. It supports
preview, policy-controlled application, and explanation output.

Source: `crates/ontology-inference/src/`.

## Engine

```rust
let engine = RuleInferenceEngine::new(store, owl, taxonomy)
    .with_external(external_registry);
```

`RuleInferenceEngine` implements `ontology_core::semantic::InferenceEngine`.
It reads the instance from the asserted graph by default, reads schema through
`OntologyManager`, and optionally reads mappings through
`ExternalOntologyRegistry`.

The taxonomy handle is currently unused by inference; approved classifications
are read directly from the classification graph.

## Methods

`InferenceMethod` identifies why a statement was proposed:

| Method | Source | Confidence |
| --- | --- | --- |
| `Subclass` | asserted type's ancestors | 1.0 |
| `Subproperty` | superproperties of an asserted predicate | 1.0 |
| `Domain` | domain of an asserted predicate | 0.9 |
| `Range` | object range of an asserted predicate | 0.9 |
| `Mapping` | external class/property mapping | mapping confidence or 1.0 |
| `Classification` | approved classification | classification confidence |
| `Rule` | attribute rule | rule confidence |

## Options

`InferenceOptions`:

| Field | Default | Effect |
| --- | --- | --- |
| `include_external` | true | Apply class/property mappings |
| `include_classification` | true | Apply approved classifications |
| `include_inherited` | true | Report applicable/inherited properties and classes |
| `rules` | empty | Classification rules to evaluate |
| `graph` | `data:asserted` | Graph holding the instance |
| `target_graph` | `data:inferred` | Graph for proposed/applied statements |

## Preview flow

`preview(instance, options)`:

1. Reads all quads for the instance in the source graph.
2. Loads the instance for attribute texts.
3. Collects asserted `(subject, predicate, object)` keys and asserted types.
4. Loads all schema properties into a map.
5. Proposes ancestor classes for each asserted type.
6. For each asserted quad with a known property:
   - adds each property domain as a class (± confidence 0.9);
   - for non-datatype ranges, adds a type statement for an object target and, if
     the object is the instance, adds the range class to the instance;
   - adds each superproperty with the same object.
7. Optionally applies external class/property mappings.
8. Optionally adds approved classifications as classes and as
   `urn:onto:schema:classifiedAs` properties.
9. Optionally evaluates rules against attribute texts.
10. Converts classes/properties into quads in the target graph.
11. Drops proposals already asserted with the same subject/predicate/object and
    records them as rejections.
12. Optionally reports applicable properties for the first asserted type and
    classes whose restrictions reference asserted predicates.
13. Builds the summary, mappings/rules/validation explanation, and rejections.

The preview is single-pass. It does not iterate to a fixpoint, so a class added
by domain/range inference is not expanded through its own superclass hierarchy
in the same preview.

### Deduplication

Classes are keyed by IRI; properties by `(predicate, object)`. If two methods
propose the same item, the higher confidence wins. Statements are keyed by the
full quad. A proposal equal to an asserted triple is rejected rather than
applied.

### Explanations

`InferencePreview.explanation` contains:

- a summary count of proposed classes/properties;
- strategies used;
- matched attributes for rules;
- mapped mapping IRIs;
- applicable/inherited properties;
- text validation notes;
- rejections.

`explain` in the command surface returns the same preview; there is no separate
explanation graph traversal.

## Application policies

`ApplyPolicy`:

| Policy | Writes | Result |
| --- | --- | --- |
| `Suggest` | none | `applied: false`, all statements skipped |
| `Review` | none | `applied: false`, `requires_approval: true` |
| `Automatic { threshold }` | statements with confidence ≥ threshold | `applied: true` |

`apply` always computes the preview first. For `Automatic`:

- selects statements by confidence;
- inserts their quads in one store insert;
- records a provenance record in `data:provenance`;
- reports applied and skipped counts.

`InferenceReport` includes the full preview, policy, `applied`,
`requires_approval`, `applied_count`, and `skipped`.

## Provenance

Successful automatic application writes:

```
<inference-id> a urn:onto:schema:Inference ;
  urn:onto:schema:instance <instance> ;
  urn:onto:schema:strategy "inference" ;
  urn:onto:schema:status "applied" ;
  urn:onto:schema:quadCount <n> ;
  urn:onto:schema:recordedAt "..." .
```

The record is written only when at least one statement is inserted.

## Rules

Rules reuse `ClassificationRule`:

- operators: equals, contains, prefix;
- comparison is case-insensitive;
- a matching rule adds its concept as an inferred class and its name/matched
  attribute to the explanation.

Rules do not create property statements; they only add classes.

## External mappings

When enabled and an external registry is present:

- for each asserted type, class mappings whose `external` matches produce the
  internal class;
- for each asserted predicate, property mappings whose `external` matches
  produce the internal property with the same object.

Mapping records are added to the explanation. Confidence defaults to 1.0.

## Approved classifications

Only records with status `approved` are consumed. Each contributes:

- the concept as an inferred class;
- `classifiedAs` pointing at the concept.

Pending and rejected records are ignored.

## Errors and invariants

- Missing instance in the source graph → `NotFound` from loading.
- Schema/taxonomy/registry failures propagate.
- `target_graph` defaults to `data:inferred`; asserted statements are never
  modified.
- Preview does not write.
- Automatic application is additive; it does not remove stale inferred
  statements.
- Duplicate proposals keep the highest confidence.
- Already-asserted statements are skipped.

## Tests

Phase 3 integration tests cover subclass/domain/range/subproperty inference,
mappings, approved classifications, rules, preview, policies, target graphs, and
provenance.

```sh
cargo test -p ontology-inference
cargo test -p ontology-integration-tests --test phase3_semantic
```

## Limitations

- Single-pass inference; no recursive closure or fixpoint.
- No OWL class-expression reasoning, disjointness, cardinality, or consistency.
- No removal/invalidation of previously inferred statements.
- Approved classifications are the only review state consumed.
- External mappings are one-hop.
- Rule language is limited to equals/contains/prefix.
- `explain` is a preview alias, not a proof-tree implementation.
- The taxonomy dependency is retained but unused.

## Related

- Schema: [owl-schema.md](owl-schema.md)
- Classification: [classification.md](classification.md)
- External mappings: [external-ontologies.md](external-ontologies.md)
- MCP safety policy for `apply`: [mcp-server.md](mcp-server.md)
