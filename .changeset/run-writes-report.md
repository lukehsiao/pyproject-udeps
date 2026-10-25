---
"pyproject-udeps": minor
---

**breaking**: the library's `run` now takes stdout and stderr writers, writes the report and any error itself, and returns the exit code.

Command-line usage is not affected by this break.
