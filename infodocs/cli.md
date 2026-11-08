# Command-Line Interface (`ontology-cli`)

`onto` is a remote administration client for the HTTP API, plus a `server`
subcommand that starts the API in-process. It does not open the RDF database
directly (except through `onto server`).

Source: `crates/ontology-cli/src/`.

## Entry points

```sh
cargo run -p ontology-cli --bin onto -- --help
cargo run -p ontology-cli --bin onto -- instance list
```

The binary builds a `Command`, posts it to `POST /api/v1/commands`, and renders
the returned `CommandResult`. A 300-second HTTP timeout is used.

## Global options

| Option | Environment | Meaning |
| --- | --- | --- |
| `--server` | `ONTO_SERVER` | API base URL |
| `--api-key` | `ONTO_API_KEY` | Bearer token |
| `--profile` | `ONTO_PROFILE` | Named connection profile |
| `--output` | — | `json`, `yaml`, or `table` |
| `--dry-run` | — | Print the translated `Command` and do not send it |
| `--yes` | — | Skip destructive-action confirmation |

Resolution order for server/key/output is CLI/environment, then the selected
profile, then defaults:

- server: `http://127.0.0.1:8080`
- output: `json`
- profile: `default`

Profiles are read from `~/.config/onto/cli.toml` or `ONTO_CLI_CONFIG`:

```toml
[profiles.default]
server = "http://127.0.0.1:8080"
api_key = "secret"
output = "table"

[profiles.prod]
server = "https://onto.example.com"
```

Selecting an unknown profile explicitly is an error. `onto config` prints the
resolved server, whether auth is enabled, output format, and dry-run state
without printing the key.

## Output

- `json` — pretty-printed JSON.
- `yaml` — YAML.
- `table` — a lightweight tab-separated table. It recognizes `items` arrays and
  `resource.items`, otherwise prints key/value lines for one object or rows for
  an array.

For an export command with `--output <path>`, the client writes the `data` field
to the file and prints `wrote <path>`. It fails if the response has no serialized
RDF.

`--dry-run` prints the exact `Command` JSON that would be posted. Policy and
domain validation do not run locally.

## Confirmations

The CLI prompts before:

- delete instance;
- delete statements;
- clear graph;
- delete class/property/ontology;
- delete scheme/concept/collection;
- merge schemes;
- delete external ontology;
- delete embeddings.

`--yes` skips the prompt. If stdin is **not** a terminal, confirmation is also
skipped automatically, which makes non-interactive destructive scripts possible.
The prompt text is written to stderr; declining returns `aborted`.

## Command tree

| Group | Subcommands |
| --- | --- |
| `server` | Start API; `--bind`, `--config` |
| `config` | Show resolved client settings |
| `instance` | create, get, list, update, delete, statements, add-statements, delete-statements, relationships |
| `rdf` | query, update, graphs, import, export, clear-graph |
| `ontology` | list, get, create, update, delete, versions, validate, applicable-properties, applicable-classes |
| `class` | list, get, create, update, delete, hierarchy |
| `property` | list, get, create, update, delete |
| `scheme` | list, get, create, update, delete, versions |
| `concept` | list, get, create, update, delete, search, relations, descendants, ancestors, move |
| `taxonomy` | tree, merge, validate, add-relation, remove-relation, collections, create-collection, get-collection, delete-collection |
| `classify` | run, batch, list, override, review, reclassify |
| `embedding` | generate, list, search, delete |
| `external` | import, list, get, sync, delete, map-class, map-property, class-mappings, property-mappings, validate-mappings |
| `inference` | preview, apply, explain |
| `import` | Bulk RDF import |
| `export` | Bulk RDF export |
| `job` | submit, classify, embed, infer, reclassify, import, export, external-import, external-sync, list, get, results, cancel, retry, watch |
| `shell` | Interactive persistent session |

Create/update commands accept `--json` or `--file` payloads for the corresponding
domain struct. Query commands accept graph, limit, offset, and type filters as
applicable.

## Server subcommand

`onto server`:

1. Loads config from `--config` or `ONTO_CONFIG`.
2. Initializes tracing.
3. Builds `ServeOptions` from the environment.
4. Applies `--bind` if supplied.
5. Uses the resolved client API key as the server's bearer key if one is set.
6. Calls `ontology_api::serve`.

It uses the same local store configuration as the API binary.

## Interactive shell

`onto shell` keeps one `Connection` and parses each line with shell-like quoting
(`shlex`).

- `help` prints the Clap help.
- `history` prints the in-memory history.
- `quit`, `exit`, or `:q` exits.
- `server` and nested `shell` are rejected inside the shell.
- History is appended to `~/.onto_history` when `HOME` is available.
- Errors are printed and the session continues.

The shell does not maintain a server-side transaction or session; every line is
an independent command.

## Job watch

`onto job watch <id> [--interval-secs N]` polls `JobCommand::Get` until the state
is `completed`, `failed`, or `cancelled`, printing state and percentage to
stderr. It then prints the final status. The minimum interval is one second.

## Error behavior

- Non-2xx HTTP responses become `API returned <status>: <body>`.
- Transport, JSON, YAML, TOML, and file errors propagate.
- The process exits non-zero and prints `onto: <error>` to stderr.
- `--dry-run` succeeds without contacting the server.

## Tests

`tests/tests/phase4_cli.rs` exercises command construction, dry-run behavior,
output, confirmations, profiles, and end-to-end calls against a test server.

```sh
cargo test -p ontology-cli
cargo test -p ontology-integration-tests --test phase4_cli
```

## Limitations

- No local/offline mode for data commands.
- Non-TTY stdin auto-approves destructive prompts.
- Table rendering is generic, not domain-aware.
- No pagination helper beyond passing `--limit`/`--offset`.
- No shell completion generation or persistent server session.
- History is a flat file with no concurrency control.

## Related

- API: [http-api.md](http-api.md)
- Commands: [core-domain.md](core-domain.md)
- Jobs: [background-jobs.md](background-jobs.md)
