"""Stable error codes shared with the Rust bridge."""


class ExtractionError(RuntimeError):
    """A failure the Rust side can report without parsing a traceback."""

    def __init__(self, code: str, message: str):
        super().__init__(f"{code}: {message}")
        self.code = code
        self.message = message
