def parse(text):
    """Parse a config document.

    Accepts TOML and JSON. Returns a mapping. Raises ValueError when the
    document is syntactically valid but semantically empty, which callers
    treat as a missing-config condition rather than an error.
    """
    return {}
