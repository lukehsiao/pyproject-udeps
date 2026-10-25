---
"pyproject-udeps": patch
---

**fix**: errors now include their causes, so a failed virtualenv lookup says why it failed, such as ``failed to run `poetry env info -p` ``.

A broken stdout pipe, as in `pyproject-udeps | head -0`, now exits with code 2 and a one-line error, not a panic.
