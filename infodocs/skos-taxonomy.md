# SKOS Taxonomy (`ontology-skos`)

`ontology-skos` stores concept schemes, concepts, hierarchical and associative
relations, and collections as SKOS RDF in `system:taxonomy`. It provides tree
construction, search, merge, versions, and taxonomy validation.

Source: `crates/ontology-skos/src/`.

## Responsibilities

- CRUD for `ConceptScheme`, `Concept`, and `ConceptCollection`.
- Canonical relation storage and inverse reconstruction.
- Cycle prevention for hierarchical moves and creations.
- Scheme trees, ancestors, and descendants.
- Label/notation search.
- Scheme merge and version recording.
- Structural taxonomy validation.

It does not classify instances; classification consumes `TaxonomyManager` from
this crate. See [classification.md](classification.md).

## Manager setup

```rust
let manager = SkosManager::new(store);                 // system:taxonomy
let manager = SkosManager::with_graph(store, graph);   // custom graph
manager.graph();
```

`SkosManager` implements `ontology_core::skos::TaxonomyManager`.

## Domain model

### `ConceptScheme`

- `id`
- `labels: BTreeMap<String, String>` keyed by language tag
- `definitions: BTreeMap<String, String>`
- `top_concepts: Vec<Id>` (`skos:hasTopConcept`)
- optional `version`

### `Concept`

- `id`
- optional `scheme`
- `pref_labels`, `alt_labels`, `hidden_labels` (`Vec<LocalizedText>`)
- `definitions`
- `notations`
- `broader`, `narrower`
- `related`
- mapping relations (`exact_match`, `close_match`, `broad_match`,
  `narrow_match`, `related_match`)
- optional `deprecated`

`narrower` is reconstructed from inverse `skos:broader` edges; the canonical
hierarchy edge is stored once.

### `ConceptCollection`

- `id`
- optional `scheme`
- labels by language
- `members: Vec<Id>` (`skos:member`)

Collections are flat; nested collections are not implemented.

### Relations

`SkosRelation` has eight variants:

`Broader`, `Narrower`, `Related`, `ExactMatch`, `CloseMatch`, `BroadMatch`,
`NarrowMatch`, `RelatedMatch`.

`ConceptRelation { source, relation, target }` is the query result shape.

## Canonical relation storage

`SkosManager::edge` normalizes input before writing:

| Input | Stored edge |
| --- | --- |
| `Broader(source, target)` | `source skos:broader target` |
| `Narrower(source, target)` | `target skos:broader source` |
| `Related` | one edge, endpoints sorted by IRI |
| `ExactMatch`, `CloseMatch`, `RelatedMatch` | one edge, endpoints sorted |
| `BroadMatch`, `NarrowMatch` | directional `source -> target` |

`relations(id)` reconstructs outgoing and incoming relations, including
`Narrower` from incoming `skos:broader`, and returns them sorted by target and
relation.

`add_relation` requires both concepts to exist. For `Broader`/`Narrower` it
checks that the resulting hierarchy would not contain a cycle. `remove_relation`
requires the source concept and deletes the canonical edge; deleting a
non-existent edge is a no-op.

## Schemes

- `create_scheme` rejects duplicates and records an initial version.
- `update_scheme` requires the scheme and replaces its definition, appending a
  version record.
- `delete_scheme` refuses while the scheme contains concepts; otherwise it
  removes the scheme and its version records.
- `list_schemes` is sorted by IRI.
- `scheme_versions` is ordered by `recorded_at`.

## Concepts

### Create

1. Reject an existing concept IRI with `Conflict`.
2. Require `scheme` if one is set.
3. Enforce preferred-label uniqueness within the scheme, case-insensitive and
   trimmed, excluding the concept itself on update.
4. Check `broader` and `narrower` edges for cycles.
5. Insert all concept quads.

A concept may be created without a preferred label; validation reports that as
a warning.

### Update

Requires the concept, applies the same scheme/label/cycle checks, then replaces
all concept-encoded quads in a transaction.

### Delete

Removes the concept's encoded quads, including its relation edges. It does not
rewrite references held by other concepts; validation will report unknown
broader concepts.

### Listing

`TaxonomyQuery` supports `scheme`, `label_contains`, `limit`, and `offset`.
`label_contains` is case-insensitive and checks preferred, alternative, hidden,
and notation values. `limit == 0` returns empty.

## Search

`search(query, scheme)`:

- lowercases and trims the query; empty query returns empty;
- checks preferred, alternative, hidden labels and notations;
- scores exact match `1.0`, prefix `0.9`, substring `0.7`;
- returns at most 50 matches, sorted by score desc, then concept IRI, then label.

This is lexical search, not fuzzy or embedding search. Semantic matching uses
the embedding provider through the classifier.

## Trees, ancestors, descendants

- `tree(scheme)` requires the scheme; builds roots from `top_concepts` plus any
  in-scheme concept with no in-scheme parent. Each node includes its preferred
  label and nested children. A visited set prevents cycles from recursing.
- `ancestors(id)` returns transitive `broader` concepts, excluding the concept.
- `descendants(id)` returns transitive `narrower` concepts, excluding the
  concept.

Both require the starting concept and return sorted concept records.

## Move

`move_concept(id, broader, scheme)`:

- requires the concept and any new parent/scheme;
- checks cycles;
- removes **all** current `skos:broader` edges and, if a scheme is supplied,
  the current `skos:inScheme` edge;
- adds the single new parent and/or scheme.

Calling it with only `scheme` preserves no parent (all broader edges are still
removed). This is a replacement operation, not a partial edge edit.

## Merge

`merge_schemes(source, target)`:

1. Requires both schemes and rejects source == target.
2. Collects source concepts and target preferred labels.
3. Emits `duplicate-label` warnings for collisions.
4. Moves each source concept's `skos:inScheme` edge to the target.
5. Copies source `top_concepts` to the target.
6. Deletes the source scheme and its versions.
7. Returns a `TaxonomyReport` scoped to the target.

Concepts and their broader/related edges are preserved. Merge is a move, not a
copy.

## Validation

`validate(Some(scheme))` scopes to one scheme; `validate(None)` checks all
concepts. Findings:

| Code | Severity | Trigger |
| --- | --- | --- |
| `missing-pref-label` | warning | no preferred label |
| `missing-scheme` | warning | concept has no scheme |
| `unknown-broader` | warning | parent not present in validation scope |
| `related-overlaps-hierarchy` | warning | related concept is an ancestor/descendant |
| `hierarchy-cycle` | error | concept reaches itself through broader |
| `duplicate-label` | warning | preferred label reused within a scheme |
| `top-concept-not-in-scheme` | warning | scheme top concept is missing from its concepts |

`TaxonomyReport::is_valid()` ignores warnings and fails on errors.

## Collections

`create_collection`, `get_collection`, `list_collections(scheme)`, and
`delete_collection` are supported. There is no update operation; update by
deleting and recreating. Creation requires the scheme if present but does not
validate member existence at write time.

## Errors and invariants

- Duplicate scheme/concept/collection → `Conflict`.
- Missing scheme/concept/collection → `NotFound`.
- A scheme with concepts cannot be deleted.
- Hierarchical edges are acyclic by write-time checks.
- Preferred labels are unique per scheme by write-time checks.
- Relation endpoints must exist on add.
- `narrower` is derived, not independently stored.
- No cascade rewrite of references on concept delete.

## Tests

`tests/tests/phase2_skos.rs` covers scheme/concept CRUD, relation
canonicalization, cycle rejection, hierarchy, search, collections, versions,
merge, and validation.

```sh
cargo test -p ontology-skos
cargo test -p ontology-integration-tests --test phase2_skos
```

## Limitations

- Lexical search only; no fuzzy search in the SKOS manager.
- Collections cannot be updated or nested.
- Delete does not clean references from other concepts.
- Merge reports duplicate labels but still merges.
- `move_concept` replaces all broader edges, even when only changing scheme.
- Validation is structural, not a complete SKOS integrity checker.

## Related

- Classification over concepts: [classification.md](classification.md)
- Inference from approved classifications: [inference.md](inference.md)
- HTTP routes: [http-api.md](http-api.md)
