# MinerU contract

Status: Phase 0 scaffolding. The shim contract and Rust-side validation are
implemented; the pinned MinerU release and its Python entry point still need to
be confirmed on a machine where MinerU is installed.

## Versions

| Item | Value | Source |
| --- | --- | --- |
| Shim contract | `1` | `python/onto_mineru/bridge.py` (`SHIM_VERSION`) |
| MinerU | `>=2.1,<3` | `python/constraints-mineru.txt` (placeholder pin) |
| PyO3 | `0.30` | `crates/ontology-mineru/Cargo.toml` |
| CPython (dev host) | 3.14.4 | `python3 --version` |
| Rust | 1.88 | workspace `rust-version` |

MinerU is not installed on the development host, so the version range above is
a placeholder. Phase 0 replaces it with the exact release used for fixtures and
records `mineru --version` here.

## Shim contract

Rust calls exactly one function:

```python
onto_mineru.bridge.extract(source: str, options: dict) -> dict
```

`options`: `output_dir` (required), `backend`, `tier`, `model_source`,
`timeout_seconds`.

The shim runs one MinerU parse, writes artefacts under `output_dir`, and
returns:

```json
{
  "shim_version": "1",
  "mineru_version": "x.y.z",
  "python_version": "3.12.9",
  "backend": "pipeline",
  "tier": "default",
  "schema": "docvortex.middle",
  "schema_version": "2.0",
  "middle_json": "/abs/run/middle.json",
  "markdown": "/abs/run/text.md",
  "content_list": null,
  "assets": [{"path": "images/figure-1.png", "mime": "image/png", "page": 2}],
  "warnings": []
}
```

Rules enforced on the Rust side:

- `schema`/`schema_version`/`mineru_version` must be non-empty and at least one
  of `middle_json`/`content_list` must be present.
- Every reported path is resolved relative to the temporary run directory and
  rejected if it is absolute outside it or contains `..`.
- Artefacts are read from disk and parsed by `ontology-document`; no document
  JSON crosses the FFI boundary.

## Entry-point adaptation

`bridge.py` tries, in order, `mineru.cli.common.do_parse`,
`mineru.cli.common.parse_doc`, and
`mineru.backend.pipeline.pipeline_analyze.doc_analyze`. It filters keyword
arguments through `inspect.signature`, so a release that renames or drops a
parameter is reported as `mineru_api_unavailable` instead of crashing.

Phase 0 must record the exact accepted signature here. Until then, the
adaptation is defensive rather than verified.

## Verification checklist

```bash
python3 -m venv .venv
./.venv/bin/pip install -r python/constraints-mineru.txt
PYO3_PYTHON=./.venv/bin/python cargo check -p ontology-mineru --features python
PYO3_PYTHON=./.venv/bin/python python -m onto_mineru.selftest
```

A successful extraction writes `middle.json`, `text.md`, and the canonical
`structured.json` under `data/extractions/sha256/<document>/<run>/`.

## Open items

1. Exact MinerU release and supported CPython range.
2. Whether the pinned release emits `docvortex.middle` directly or needs the
   content-list/structured-content fallback.
3. Whether NLP Markdown rendering reuses the parse result or requires a second
   pass (affects `both` cost only).
4. Model provisioning layout (`model_source`) for the deployment.
