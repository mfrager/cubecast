# External Ontologies (`ontology-import`)

`ontology-import` manages imported external ontologies and the class/property
mapping registry. Registry metadata and mappings live in `system:external`;
imported statements live in a dedicated graph per ontology.

Source: `crates/ontology-import/src/`.

## Responsibilities

- Import an RDF ontology from a local path or `file://` URI.
- Discover or accept the ontology IRI, format, and target graph.
- Count classes/properties and discover the vocabulary namespace.
- Record source, graph, version, title, timestamp, and counts.
- Re-import/sync from the recorded source.
- Delete an ontology's data graph and registry entry.
- Record and validate class/property mappings.

It does not fetch HTTP(S) documents. It does not perform OWL reasoning on
imported ontologies.

## Domain model

### `ExternalOntology`

| Field | Meaning |
| --- | --- |
| `id` | Ontology IRI as published |
| `source` | Original path or URI |
| `graph` | Graph containing the imported statements |
| `title` | `rdfs:label` or `dcterms:title` from the ontology subject |
| `version` | `owl:versionInfo` |
| `namespace` | Most common vocabulary namespace among declared classes/properties |
| `format` | Syntax used |
| `imported_at` | Import timestamp |
| `quad_count` | Parsed quad count |
| `class_count` | Distinct subjects typed `owl:Class` |
| `property_count` | Distinct subjects typed as object/datatype/annotation property |

### `ImportRequest`

```json
{
  "source": "./ontologies/example.ttl",
  "ontology": "https://example.org/onto",
  "format": "turtle",
  "graph": "urn:onto:external:example"
}
```

Only `source` is required. `ontology` defaults to the document's
`owl:Ontology`; `format` defaults to the source path extension; `graph` defaults
to a graph derived from the ontology IRI.

### `ImportReport`

`ontology`, `graph`, `inserted`, `class_count`, `property_count`,
`already_present`.

### Mappings

`ClassMapping` and `PropertyMapping`:

- `id` — mapping record IRI
- `external` — external class/property IRI
- `internal` — internal class/property IRI
- `kind` — `equivalent_class`, `subclass_of`, `equivalent_property`, or
  `property_transformation`
- optional `confidence`
- `recorded_at`

## Source loading

`read_source` accepts:

- a filesystem path;
- a path with a `file://` prefix (prefix stripped).

`http://` and `https://` return `Unsupported { operation: "remote ontology
resolution" }`. This is a deliberate current limitation, not a silent fallback.

If `format` is omitted, the format is inferred from the source path with
`RdfFormat::from_path`. A missing or unknown extension produces `Validation`
asking for an explicit format.

## Import flow

1. Read the document bytes.
2. Resolve the RDF format.
3. Parse all quads with `ontology_rdf::parse_quads`. A base IRI is supplied when
   the source contains `://`.
4. Use the requested ontology IRI or discover the first subject typed
   `owl:Ontology`. No declaration is a validation error.
5. If the ontology is already registered with a different source, return
   `Conflict`.
6. Choose the target graph: request graph, else existing graph, else
   `urn:onto:external:<first 16 hex of SHA-256(ontology IRI)>`.
7. Count classes/properties and discover the most common namespace.
8. Build the registry record from the ontology subject's title/version.
9. Stage removals:
   - all quads in the previous graph, if this is a re-import;
   - all quads in the target graph;
   - metadata for the ontology subject in `system:external`.
10. Stage additions:
    - every parsed quad forced into the target graph;
    - the new registry metadata.
11. Commit removals and additions in one transaction.

Because step 9 clears the entire target graph, explicitly pointing two
ontologies at the same graph will cause the later import to replace the earlier
data. Use a dedicated graph per ontology unless sharing is intentional.

The import preserves every parsed statement, including statements whose subject
is not the ontology IRI. Statements are **not** rewritten into the internal
schema graph.

## Sync

`sync(id)`:

1. Loads the registry record.
2. Reads the recorded source again.
3. Builds an `ImportRequest` with the recorded source, ontology, format, and
   graph.
4. Runs the same import/replacement path.

Sync is a full replacement, not a diff. If the source is a local file that has
been removed, sync fails before any mutation.

## Delete

`delete(id)` loads the record, collects all quads in its data graph plus its
metadata subject in `system:external`, and deletes them. It does not remove
mappings that reference the ontology.

## Registry queries

- `list()` returns every record, sorted by IRI.
- `get(id)` returns `NotFound` if absent.
- Loading a malformed record (missing graph, unknown mapping kind) skips or
  errors depending on the record; missing required fields mean "not a record".

## Mappings

`map_class` and `map_property` reject a mapping with the same
`(external, internal, kind)` tuple as an existing record, returning `Conflict`.
They do not require either side to be declared at write time.

`list_class_mappings` and `list_property_mappings` accept `MappingQuery`
(`external`, `internal`, `limit`, `offset`) and apply exact filters before
pagination.

### Validation

`validate_mappings()` checks:

- external class/property is declared as the right OWL type by **some** imported
  graph;
- internal class/property is declared in `system:schema`;
- each failure becomes a warning `MappingIssue` with the mapping IRI.

If the external side is missing, the internal check is skipped for that mapping.
The report contains `checked` and `issues`.

## RDF metadata layout

In `system:external`:

- registry record typed `urn:onto:schema:ExternalOntology`
- `source`, `targetGraph`, `quadCount`, `classCount`, `propertyCount`,
  `namespace`, `format`
- `rdfs:label` / `dcterms:title`, `owl:versionInfo`
- `prov:generatedAtTime`
- mappings typed `ClassMapping` or `PropertyMapping`, with external/internal
  IRIs, `mappingKind`, optional `confidence`, and `recordedAt`

## Errors and invariants

| Condition | Error |
| --- | --- |
| Missing/unreadable file | `Validation` with path detail |
| HTTP(S) source | `Unsupported` |
| Unknown format | `Validation` |
| No `owl:Ontology` | `Validation` |
| Same ontology from a different source | `Conflict` |
| Unknown ontology on get/sync/delete | `NotFound` |
| Duplicate mapping tuple | `Conflict` |

- Import replacement is atomic.
- A re-import clears the target graph, not just statements from the previous
  document.
- No remote fetching, redirects, authentication, or caching.
- No signature/integrity verification.
- No mapping deletion API in the current command set.

## Tests

`tests/tests/phase2_external.rs` covers imports, re-import replacement, sync,
delete, metadata/counts, mappings, and mapping validation.

```sh
cargo test -p ontology-import
cargo test -p ontology-integration-tests --test phase2_external
```

## Related

- Core registry types: [core-domain.md](core-domain.md)
- Inference using mappings: [inference.md](inference.md)
- HTTP/CLI/MCP operations: [http-api.md](http-api.md), [cli.md](cli.md),
  [mcp-server.md](mcp-server.md)
