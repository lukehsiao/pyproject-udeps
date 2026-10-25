---
"pyproject-udeps": patch
---

**fix**: with `--virtualenv`, a package's imports of its own modules no longer mark it as used.

Before, any installed package that imports itself, such as `graphviz` or `pydantic`, was never reported, even when nothing else used it.
