---
"pyproject-udeps": patch
---

**fix**: upgrade `ruff_python_ast` and `ruff_python_parser` to 0.0.6, which raises the minimum supported Rust version to 1.95. The 0.0.4 releases pulled `compact_str` 0.9 while deriving `GetSize` through `get-size2`, whose 0.10.2 release moved to `compact_str` 0.10. That combination no longer compiles, so staying on 0.0.4 would have meant pinning `get-size2` to a version we do not otherwise depend on.
