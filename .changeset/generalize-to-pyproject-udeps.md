---
"pyproject-udeps": minor
---

**feat**: generalize from poetry to poetry, uv, and plain PEP 621 projects, and rename to `pyproject-udeps`.

The crate previously published as `poetry-udeps` now understands every place `pyproject.toml` can declare a dependency (`[tool.poetry.*]` including all groups, `[project.dependencies]`, `[project.optional-dependencies]`, PEP 735 `[dependency-groups]`, and legacy `[tool.uv] dev-dependencies`), and `--virtualenv` discovers uv and PEP 621 environments instead of assuming poetry. Import extraction moved from a hand-rolled scanner to ruff's error-resilient parser, which fixes several accuracy bugs (multi-name and parenthesized from-imports, `'''` docstrings, phantom imports inside string literals) and adds `importlib.import_module`/`__import__` detection, so reports may legitimately change on upgrade. The ignorefile is now `.pyprojectudepsignore`, with the old name still honored.
