# Embeddings and Similarity (`ontology-embedding`)

`ontology-embedding` generates vectors for instance attributes, stores embedding
metadata as RDF, maintains an in-process vector index, and answers similarity
queries.

Source: `crates/ontology-embedding/src/`.

## Components

| Component | Role |
| --- | --- |
| `HashingEmbeddingProvider` | Deterministic local text-to-vector baseline |
| `InMemoryVectorIndex` | Locked map plus linear similarity scan |
| `EmbeddingService` | Generation, persistence, search, list, delete |

## Provider

`HashingEmbeddingProvider`:

- lowercases and splits text on non-alphanumeric characters;
- hashes each token with a 64-bit FNV-style hash;
- maps the hash to a bucket in `0..dimensions`;
- adds `+1` or `-1` based on the top hash bit (signed hashing trick);
- L2-normalizes the resulting vector;
- reports model id `hashing-v1` and an empty model version.

Construction:

```rust
HashingEmbeddingProvider::new(128)       // panics on 0
HashingEmbeddingProvider::try_new(128)?  // fallible
    .with_model_id("custom-baseline");
```

The default application service uses 128 dimensions. This provider is a
reproducible baseline for tests and offline use, not a trained semantic model.
It has no vocabulary beyond hashed tokens; synonyms are not represented.

## Vector index

`InMemoryVectorIndex` stores `VectorRecord`s in a `BTreeMap<Id, VectorRecord>`
behind `RwLock`.

- `upsert` replaces records by ID.
- `search` scans all records, applies metadata filters, rejects vectors with a
  different dimension, scores, sorts, and truncates.
- `delete` removes by ID and returns the number removed.
- Poisoned locks become `Validation` errors.

Metrics:

| Metric | Score |
| --- | --- |
| Cosine | cosine similarity |
| Dot | raw dot product |
| Euclidean | `1 / (1 + euclidean_distance)` |

Higher is always better. Ties sort by record ID. A zero query vector scores 0
for cosine.

`VectorFilter` supports exact `instance`, `attribute`, and `model_id` filters.

The index is process-local and is not rebuilt from RDF metadata on startup.
After a restart, stored embedding records remain but similarity search has no
vectors until generation runs again.

## Service

```rust
let service = EmbeddingService::new(store, provider, index);
```

`metadata_graph()` is `urn:onto:data:provenance`.

### Generate

`generate(instance, attributes, graph)`:

1. Loads the instance (default graph `data:asserted` when `graph` is `None`).
2. Collects literal attribute texts. `attributes` empty means all.
3. Errors if there is nothing to embed.
4. Builds an aggregate text by joining all selected values with newlines.
5. Sends all attribute texts plus the aggregate in one provider call.
6. For each text:
   - creates `urn:onto:embedding:<ULID>`;
   - computes SHA-256 of the source text;
   - stores metadata in the provenance graph;
   - upserts a vector record;
   - returns an `EmbeddingRecord` including the full vector.

One record per selected attribute plus one aggregate record with
`attribute: None`.

Persisted metadata includes instance, optional attribute, model id, dimensions,
source hash, model version, and `prov:generatedAtTime`. The vector itself is
**not** persisted as RDF.

### Search by instance

`search(instance, limit, filter)`:

1. Loads the instance from the asserted graph.
2. Joins all literal attribute values with newlines.
3. Errors if the instance has no text.
4. Embeds that aggregate text.
5. Searches the index with cosine similarity.

The query text is the concatenation of all attributes, while generation creates
both per-attribute vectors and one aggregate vector. Results therefore usually
include the aggregate record and can include attribute-level records. Callers
can use `VectorFilter::attribute` to restrict the target side, but the query
still uses all attributes.

### Other methods

- `search_vector(vector, limit, filter, metric)` — explicit vector search; not
  exposed by the command API.
- `list(instance)` — metadata records sorted by ID, with `vector: []`.
- `delete(instance)` — deletes metadata quads and index records; returns the
  number of metadata quads removed.
- `embedded_instances()` — distinct instances with metadata.

## RDF metadata

In the provenance graph:

```
<embedding-id> a urn:onto:schema:Embedding ;
  urn:onto:schema:instance <instance> ;
  urn:onto:schema:attribute "..." ;       # omitted for aggregate
  urn:onto:schema:modelId "hashing-v1" ;
  urn:onto:schema:dimensions 128 ;
  urn:onto:schema:sourceHash "<sha256>" ;
  urn:onto:schema:version "" ;
  prov:generatedAtTime "..." .
```

`list` reconstructs metadata but cannot reconstruct vectors from RDF.

## Command surface

`SemanticCommand` exposes:

- `generateEmbeddings` / `generate`
- `searchSimilar` / `similarity`
- `listEmbeddings` / `embeddings`
- `deleteEmbeddings`
- job `embedBatch`

The app returns `CommandResult::Embeddings`, `Similarity`, or `Report`.
`embedBatch` runs `generate` for each instance in a background job.

## Errors and invariants

| Condition | Error |
| --- | --- |
| No literal attributes | `Validation` |
| Provider returns no vector for search | `Validation` |
| Provider/index failure | propagated |
| Poisoned index lock | `Validation` |
| Unknown instance | `NotFound` from instance loading |

- Generation is additive; it does not delete older embeddings first.
- `delete` removes both metadata and index entries.
- Dimension mismatches are silently excluded from index search.
- Metadata and vectors can diverge after restart or partial failure.

## Tests

Phase 3 integration tests cover generation, metadata listing, similarity,
filters, deletion, and deterministic scores. The default provider makes scores
reproducible.

```sh
cargo test -p ontology-embedding
cargo test -p ontology-integration-tests --test phase3_semantic
```

## Limitations

- Hashing provider only; no external model integration is wired in.
- In-memory linear index; no ANN, persistence, or rebuild.
- Vectors are not stored in RDF, only metadata.
- No batch upsert transaction across metadata and index.
- Search query uses concatenated text, not a selected attribute.
- No model migration or stale-vector detection beyond regenerating.

## Related

- Provider/index traits: [core-domain.md](core-domain.md)
- Classification using embeddings: [classification.md](classification.md)
- Jobs: [background-jobs.md](background-jobs.md)
