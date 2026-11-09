"""Contract check for a provisioned environment.

Run with::

    python -m onto_mineru.selftest

Exit code 0 means the shim imports, MinerU imports, and the manifest contract
can be described. It does not run a parse; that is covered by the gated
end-to-end test.
"""

from __future__ import annotations

import importlib
import json
import sys

from .bridge import SHIM_VERSION, _schema, extract


def main() -> int:
    report = {
        "shim_version": SHIM_VERSION,
        "python": ".".join(str(part) for part in sys.version_info[:3]),
        "executable": sys.executable,
        "mineru": None,
        "extract_signature": str(importlib.import_module(extract.__module__).extract.__doc__ or "")[:200],
        "schema_probe": _schema(None, "probe"),
    }
    try:
        mineru = importlib.import_module("mineru")
    except Exception as exc:
        print(json.dumps({**report, "ok": False, "error": str(exc)}, indent=2))
        return 1
    report["mineru"] = str(getattr(mineru, "__version__", "unknown"))
    print(json.dumps({**report, "ok": True}, indent=2))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
