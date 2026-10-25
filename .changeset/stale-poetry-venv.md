---
"pyproject-udeps": patch
---

**fix**: with `--virtualenv`, a poetry virtualenv path that no longer exists now falls back to `.venv`, or fails with exit code 2.

Before, the tool searched nothing and reported every dependency the virtualenv would have used.
