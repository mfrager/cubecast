"""Run one MinerU parse and report its artefacts.

Contract
========

``extract(source, options) -> dict``

``options`` keys: ``output_dir`` (required), ``backend``, ``tier``,
``model_source``, ``timeout_seconds``.

The returned manifest is versioned by ``SHIM_VERSION`` and lists artifact
paths relative to ``output_dir``. Rust validates that every path stays inside
the run directory before reading it.

MinerU's Python entry point differs between releases. The Phase 0 spike pins
one release and records its signature in ``docs/mineru-contract.md``; this
module adapts to the two entry points used by the 2.x pipeline and reports a
clear, stable error when neither exists.
"""

from __future__ import annotations

import glob
import importlib
import inspect
import json
import os
import time
import traceback

from .errors import ExtractionError

SHIM_VERSION = "1"

_BLOCK_TEXT_TYPES = {"text", "title", "heading", "list", "list_item", "footnote", "code"}


def extract(source: str, options: dict) -> dict:
    """Runs one MinerU parse and returns the manifest dictionary."""
    if not source or not os.path.isfile(source):
        raise ExtractionError("missing_source", f"source {source!r} is not a file")
    output_dir = os.path.abspath(options.get("output_dir") or "")
    if not output_dir:
        raise ExtractionError("missing_output_dir", "options.output_dir is required")
    os.makedirs(output_dir, exist_ok=True)
    _prepare_environment(options)

    started = time.monotonic()
    try:
        mineru = importlib.import_module("mineru")
    except Exception as exc:  # pragma: no cover - environment dependent
        raise ExtractionError(
            "mineru_missing",
            f"could not import mineru from {_interpreter()}: {exc}",
        ) from exc

    version = str(getattr(mineru, "__version__", "") or "unknown")
    warnings: list[str] = []
    _run_parse(source, output_dir, options, warnings)

    middle = _first(output_dir, "middle.json")
    content_list = _first(output_dir, "content_list.json")
    if middle is None and content_list is None:
        raise ExtractionError(
            "missing_artifact",
            "MinerU produced neither middle.json nor content_list.json",
        )

    markdown = _first(output_dir, "*.md")
    if markdown is None and middle is not None:
        markdown = os.path.join(output_dir, "text.md")
        _render_markdown(middle, markdown)
        warnings.append("markdown rendered by the shim from middle.json")

    schema, schema_version = _schema(middle, content_list)
    assets = _assets(output_dir)
    elapsed_ms = int((time.monotonic() - started) * 1000)

    return {
        "shim_version": SHIM_VERSION,
        "mineru_version": version,
        "python_version": _python_version(),
        "backend": str(options.get("backend") or "pipeline"),
        "tier": str(options.get("tier") or "default"),
        "schema": schema,
        "schema_version": schema_version,
        "middle_json": middle,
        "markdown": markdown,
        "content_list": content_list,
        "assets": assets,
        "warnings": warnings,
        "duration_ms": elapsed_ms,
    }


def _prepare_environment(options: dict) -> None:
    """Makes model selection explicit; downloading is never implicit."""
    source = options.get("model_source")
    if source:
        os.environ.setdefault("MINERU_MODEL_SOURCE", os.path.abspath(str(source)))
    timeout = options.get("timeout_seconds")
    if timeout:
        os.environ.setdefault("ONTO_MINERU_TIMEOUT_SECONDS", str(int(timeout)))


def _run_parse(source: str, output_dir: str, options: dict, warnings: list[str]) -> None:
    """Calls the pinned MinerU entry point with arguments it actually accepts."""
    attempts: list[str] = []
    candidates = [
        ("mineru.cli.common", "do_parse"),
        ("mineru.cli.common", "parse_doc"),
        ("mineru.backend.pipeline.pipeline_analyze", "doc_analyze"),
    ]
    for module_name, function_name in candidates:
        try:
            module = importlib.import_module(module_name)
            function = getattr(module, function_name)
        except Exception as exc:
            attempts.append(f"{module_name}.{function_name}: {exc}")
            continue

        try:
            kwargs = _kwargs_for(function, source, output_dir, options)
        except ExtractionError as exc:
            attempts.append(f"{module_name}.{function_name}: {exc}")
            continue

        try:
            result = function(**kwargs)
            _record_result(result, output_dir, warnings)
            return
        except TypeError as exc:
            attempts.append(f"{module_name}.{function_name}: {exc}")
        except Exception as exc:  # pragma: no cover - MinerU dependent
            raise ExtractionError(
                "mineru_failed",
                f"{module_name}.{function_name} failed: {exc}\n{traceback.format_exc(limit=4)}",
            ) from exc

    raise ExtractionError(
        "mineru_api_unavailable",
        "no supported MinerU entry point was found; tried " + "; ".join(attempts),
    )


def _kwargs_for(function, source: str, output_dir: str, options: dict) -> dict:
    signature = inspect.signature(function)
    parameters = signature.parameters
    with open(source, "rb") as handle:
        data = handle.read()
    stem = os.path.splitext(os.path.basename(source))[0]
    candidates = {
        "output_dir": output_dir,
        "output": output_dir,
        "pdf_file_names": [stem],
        "pdf_bytes": [data],
        "pdf_file_name": stem,
        "pdf_path": source,
        "pdf_file": source,
        "backend": str(options.get("backend") or "pipeline"),
        "parser": str(options.get("backend") or "pipeline"),
        "tier": str(options.get("tier") or "default"),
        "model_path": options.get("model_source"),
        "f_dump_md": True,
        "f_dump_middle_json": True,
        "f_dump_content_list": True,
        "f_dump_model_output": False,
        "f_dump_orig_pdf": False,
        "f_make_md_mode": "NLP_MD",
    }
    kwargs = {
        name: value
        for name, value in candidates.items()
        if name in parameters and value is not None
    }
    if "output_dir" not in kwargs and "output" not in kwargs:
        raise ExtractionError("unsupported_signature", "entry point has no output directory parameter")
    if not any(key in kwargs for key in ("pdf_bytes", "pdf_file_names", "pdf_path", "pdf_file")):
        raise ExtractionError("unsupported_signature", "entry point has no document parameter")
    return kwargs


def _record_result(result, output_dir: str, warnings: list[str]) -> None:
    """Persists results returned in memory instead of written by MinerU."""
    if not isinstance(result, (list, tuple)):
        return
    pages = []
    for item in result:
        if isinstance(item, dict) and ("pdf_info" in item or "page_idx" in item):
            pages.append(item)
    if not pages:
        return
    if _first(output_dir, "middle.json") is None:
        with open(os.path.join(output_dir, "middle.json"), "w", encoding="utf-8") as handle:
            json.dump({"pdf_info": pages, "schema": "docvortex.middle", "schema_version": "2.0"}, handle)
        warnings.append("middle.json assembled by the shim from the parser result")


def _first(directory: str, pattern: str) -> str | None:
    matches = sorted(glob.glob(os.path.join(directory, "**", pattern), recursive=True))
    return matches[0] if matches else None


def _schema(middle_path: str | None, content_path: str | None) -> tuple[str, str]:
    if middle_path is not None:
        try:
            with open(middle_path, encoding="utf-8") as handle:
                document = json.load(handle)
        except Exception:
            document = {}
        schema = str(document.get("schema") or "docvortex.middle")
        version = str(document.get("schema_version") or document.get("_version_name") or "unknown")
        return schema, version
    return "docvortex.content_list", "2"


def _render_markdown(middle_path: str, target: str) -> None:
    """NLP-style Markdown: headings and paragraphs, no visual blocks."""
    with open(middle_path, encoding="utf-8") as handle:
        document = json.load(handle)
    lines: list[str] = []
    for page in document.get("pdf_info") or []:
        blocks = page.get("para_blocks") or page.get("preproc_blocks") or page.get("blocks") or []
        for block in blocks:
            kind = str(block.get("type") or "text").lower()
            text = _block_text(block)
            if not text:
                continue
            level = int(block.get("level") or block.get("text_level") or 0)
            label = sub = None
            if kind == "title":
                label = "#"
            elif kind in {"heading", "section_header"}:
                label = "#" * max(1, min(level or 1, 6))
            elif kind in {"list", "list_item"}:
                label = "-"
            elif kind.startswith("interline") or kind in {"equation", "formula"}:
                label = "$$"
                sub = "$$"
            elif kind not in _BLOCK_TEXT_TYPES:
                continue
            if label in {"#", "-"} or (label and label.startswith("#")):
                lines.append(f"{label} {text}")
            elif label == "$$":
                lines.append(f"$$\n{text}\n$$")
            else:
                lines.append(text)
    with open(target, "w", encoding="utf-8") as handle:
        handle.write("\n\n".join(lines).strip() + "\n")


def _block_text(block: dict) -> str:
    content = block.get("content") or block.get("text")
    if isinstance(content, str) and content.strip():
        return content.strip()
    parts = []
    for line in block.get("lines") or []:
        for span in line.get("spans") or []:
            value = span.get("content") or span.get("text") or ""
            if value:
                parts.append(str(value))
        parts.append(" ")
    return "".join(parts).strip()


def _assets(directory: str) -> list[dict]:
    assets = []
    for pattern in ("**/*.png", "**/*.jpg", "**/*.jpeg", "**/*.webp", "**/*.svg"):
        for path in sorted(glob.glob(os.path.join(directory, pattern), recursive=True)):
            relative = os.path.relpath(path, directory)
            assets.append(
                {
                    "path": relative,
                    "mime": _mime(relative),
                    "page": None,
                }
            )
    return assets


def _mime(path: str) -> str:
    lower = path.lower()
    if lower.endswith(".png"):
        return "image/png"
    if lower.endswith((".jpg", ".jpeg")):
        return "image/jpeg"
    if lower.endswith(".webp"):
        return "image/webp"
    if lower.endswith(".svg"):
        return "image/svg+xml"
    return "application/octet-stream"


def _interpreter() -> str:
    import sys

    return sys.executable


def _python_version() -> str:
    import sys

    return ".".join(str(part) for part in sys.version_info[:3])
