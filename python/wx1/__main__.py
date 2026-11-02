"""Executable entry point for python -m wx1."""

from __future__ import annotations

import sys
from .cli import main

if __name__ == "__main__":
    sys.exit(main())
