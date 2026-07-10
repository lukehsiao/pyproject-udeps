---
"pyproject-udeps": minor
---

**feat**: parse Python with ruff's parser instead of scanning text for import statements.

Imports are now collected from a real AST, which fixes a family of accuracy bugs: `from x import a, b` counted only `a`, `from x import *` counted nothing, `'''`-quoted docstrings were not skipped, and text like `x = "import os"` produced phantom imports. Imports nested in functions and `try`/`except ImportError` blocks are found, files with syntax errors still contribute whatever parses, and `importlib.import_module("...")` and `__import__("...")` calls with literal arguments now count as usage. Reports may legitimately change on upgrade: dependencies that only appeared inside strings or docstrings will newly show up as unused.
