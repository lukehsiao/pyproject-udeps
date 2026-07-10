---
"pyproject-udeps": patch
---

**fix**: stop crashing on `import dbt.adapters` and on non-UTF-8 Python files.

A bare two-segment `import dbt.adapters` panicked the matcher, and a single `.py` file with non-UTF-8 bytes (latin-1 comments in legacy code, say) crashed the project scan. Both now behave: the dbt heuristic only fires with an adapter segment present, and files are read lossily everywhere.
