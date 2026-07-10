---
"pyproject-udeps": minor
---

**feat**: support uv and plain PEP 621 projects, not just poetry.

Dependencies are now read from every place `pyproject.toml` can declare them: `[project.dependencies]`, every `[project.optional-dependencies]` extra, `[tool.poetry.dependencies]`, every `[tool.poetry.group.*]` (previously only the `dev` group was checked), PEP 735 `[dependency-groups]`, and the legacy `[tool.uv] dev-dependencies` array. Hybrid layouts take the union, so a wrong guess about which tool manages the project can never drop a declaration. `--virtualenv` also stopped assuming poetry: the environment is discovered from the lockfile and tool tables, using `poetry env info -p` for poetry projects, `$UV_PROJECT_ENVIRONMENT` or `.venv` for uv projects, and `$VIRTUAL_ENV` or `.venv` otherwise.
