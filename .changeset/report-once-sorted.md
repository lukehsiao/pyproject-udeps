---
"pyproject-udeps": patch
---

**fix**: each unused dependency is now reported once, and the report is sorted by package name.

A package declared in both the main and dev dependencies counts as a main dependency, so `--dev` no longer prints it twice.
