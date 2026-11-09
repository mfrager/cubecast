"""Repository-owned MinerU shim.

Rust calls :func:`onto_mineru.bridge.extract` through PyO3. Everything that
depends on MinerU's Python API and output layout lives here, so a MinerU
release change never reaches the Rust crates by surprise.
"""

from .bridge import SHIM_VERSION, extract
from .errors import ExtractionError

__all__ = ["SHIM_VERSION", "extract", "ExtractionError"]
