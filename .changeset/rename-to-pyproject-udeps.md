---
"pyproject-udeps": minor
---

**feat**: rename `poetry-udeps` to `pyproject-udeps`.

The tool is no longer poetry-only, so the name follows the file it actually analyzes. Install it with `cargo install pyproject-udeps` (or `cargo binstall pyproject-udeps`); the binary is now `pyproject-udeps`. The ignorefile is `.pyprojectudepsignore`, and an existing `.poetryudepsignore` keeps working as a fallback. One last `poetry-udeps` release on crates.io points here.
