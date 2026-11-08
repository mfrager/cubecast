# Classification (`ontology-classify`)

Classification maps instance attributes to SKOS concepts using exact,
normalized, token, fuzzy, rule, and embedding-based strategies. Results can be
persisted, reviewed, overridden, and reclassified.

Source: `crates/ontology-classify/`.

## Components

| Component | Role |
| --- | --- |
| `MatchingClassifier` | Candidate generation and scoring |
| `ClassificationService` | Loads instances, persists results, review/override |
| `scoring` | Normalization, Jaccard, Levenshtein, cosine helpers |

`ClassificationService::new(store, taxonomy)` builds a classifier. Optional
builder methods:

```rust
.with_embedding_provider(provider)
.with_rules(rules)
```

The default `ApplicationService` supplies a `HashingEmbeddingProvider` but no
classification rules. Rules are available to library users and custom
deployments.

## Options

`ClassificationOptions`:

| Field | Meaning |
| --- | --- |
| `scheme` | Restrict candidate concepts to one scheme |
| `strategies` | Strategies to run; empty means `Hybrid` |
| `threshold` | Minimum accepted confidence |
| `max_results` | Maximum accepted classifications per instance |
| `attributes` | Restrict input attributes; empty means all literal attributes |
| `language` | Prefer labels in this language; untagged labels always remain |
| `include_ancestors` | Add ancestor concepts at 0.9× the child confidence |
| `persist` | Write accepted results to the classification graph (default true) |
| `graph` | Graph holding the instance; defaults to `data:asserted` |

`max_results == 0` returns no classifications without loading candidates.

## Candidate generation

The classifier calls `TaxonomyManager::list_concepts` with `limit: 100_000`
(optionally scoped to a scheme). For each concept it collects:

- preferred, alternative, and hidden labels with language tags;
- notations as untagged labels.

If `language` is set, only labels with that language or no language are kept.
Labels are sorted and deduplicated.

If no concepts exist, or the instance has no literal attributes in scope, the
result is empty.

## Strategies

| Strategy | Behavior | Base score |
| --- | --- | --- |
| `Exact` | Case-sensitive equality | `1.0` |
| `Normalized` | Lowercase, punctuation-stripped, whitespace-collapsed equality | `0.95` |
| `Token` | Jaccard similarity of token sets | `0.0..=1.0` |
| `Fuzzy` | Normalized Levenshtein similarity | `0.0..=1.0` |
| `Rules` | Attribute rule fires | rule confidence |
| `Semantic` | Cosine similarity between attribute and label embeddings | `0.0..=1.0` |
| `Hybrid` | Weighted combination of available strategies | weighted average |
| `Manual` | Only produced by overrides | caller-supplied |

Normalization keeps alphanumeric characters, lowercases, and collapses runs of
punctuation/whitespace into one space. Levenshtein is normalized by the longest
input; empty/empty is `1.0`. Cosine returns `0.0` for a zero vector.

`ClassificationStrategy::weight()` controls Hybrid weighting and is defined in
`ontology_core::semantic`.

### Hybrid and explicit selection

- Empty `strategies` is treated as `[Hybrid]`.
- For `Hybrid`, the classifier combines every strategy that produced a score:
  `sum(score × weight) / sum(weight)`. The matched attribute comes from the
  highest-scoring individual strategy.
- For an explicit strategy list, the best matching strategy is selected.
- The best result per concept is retained before thresholding.

## Rules

A `ClassificationRule` contains:

- `name`, `attribute`, `operator` (`equals`, `contains`, `prefix`), `value`;
- target `concept` and `confidence`.

Rule values are normalized before comparison. A rule only applies when its
attribute is among the instance texts and its concept is already a candidate.
A firing rule inserts a `Rules` score; it does not create a concept candidate by
itself.

## Semantic matching

When an embedding provider is configured:

1. Build one input list of all attribute texts plus every candidate label.
2. Embed the entire batch in one provider call.
3. Verify the provider returned exactly one vector per input.
4. For each candidate, compute cosine similarity against each attribute text
   and keep the best pair.

A provider that returns a mismatched vector count fails with `Validation`.

## Ranking, threshold, and explanations

Candidates below `threshold` are rejected. Rejections are sorted by descending
score, capped at five, and attached to the first accepted classification's
`explanation.rejections`.

Accepted results are sorted by score descending, then concept IRI, truncated to
`max_results`, and converted to `Classification` records with:

- a generated `urn:onto:classification:<ULID>` identifier;
- instance, concept, scheme, confidence, strategy;
- matched attribute/value/label;
- taxonomy version from the scheme record, when available;
- `manual: false`, `status: pending`, timestamp;
- explanation summary and strategies.

If `include_ancestors` is true, each accepted concept's ancestors are added at
`confidence * 0.9`, with an explanation noting the ancestor relationship.

## Persistence

Accepted results are written to `urn:onto:data:classification` when
`persist` is true. Stored quads include:

- `rdf:type urn:onto:schema:Classification`
- instance, concept, optional scheme
- confidence (`xsd:decimal`), strategy, status
- `recordedAt`
- `manualOverride true` for manual records

Stored records **do not** persist matched attributes, taxonomy version, or the
full explanation. `list(instance)` reconstructs a record with empty matches and
a generic `"stored classification"` explanation. Consumers that need the full
explanation should use the immediate classify response.

## Review and overrides

### Override

`override_classification(instance, concept, confidence)`:

- validates confidence in `0.0..=1.0`;
- deletes every existing record for that instance/concept (manual or automatic);
- creates a manual record with strategy `Manual`, status `approved`,
  `manual: true`, no scheme or taxonomy version, and a `"manual override"`
  explanation.

The override does not verify that the concept exists.

### Review

`review(classification_id, approved)` loads the record, sets status to
`approved` or `rejected`, and updates the status quad in a transaction. Missing
records return `NotFound`.

Approval matters to inference: only approved classifications are consumed.

### Reclassify

`reclassify(instance, options)` deletes automatic records for the instance
(manual records are preserved), then classifies again and persists according to
the options.

## Batch behavior

`classify_batch` loads all instance IDs first, classifies each in order,
aggregates accepted results, and collects rejections into a
`ClassificationBatch { instance_count, classifications, rejected }`.

The application command returns only `classifications`; the per-instance rejected
list is discarded at the command boundary.

## Errors and invariants

- Empty attribute set → empty result (not an error).
- Missing instance on load → `NotFound`.
- Provider vector-count mismatch → `Validation`.
- Override confidence outside `0.0..=1.0` → `Validation`.
- Review/override transitions are persisted immediately.
- Classification records live only in the classification graph.
- Manual records survive reclassification.

## Tests

`tests/tests/phase3_semantic.rs` covers strategies, thresholding, explanations,
persistence, overrides, review, reclassification, and batch classification.

```sh
cargo test -p ontology-classify
cargo test -p ontology-integration-tests --test phase3_semantic
```

## Limitations

- Candidate loading is capped at 100,000 concepts and is linear.
- Hybrid weights are fixed, not configurable per request.
- Stored records lose match details and taxonomy version.
- `override` does not validate the concept.
- The app service does not inject custom rules; only library callers can.
- No learning, active learning, or feedback loop from reviews.

## Related

- Taxonomy source: [skos-taxonomy.md](skos-taxonomy.md)
- Embeddings: [embeddings.md](embeddings.md)
- Inference consuming approved results: [inference.md](inference.md)
