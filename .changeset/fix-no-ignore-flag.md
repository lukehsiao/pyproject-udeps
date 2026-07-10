---
"pyproject-udeps": patch
---

**fix**: honor the `--no-ignore` flag.

The flag was accepted but never read, so there was no way to see the report without ignorefile filtering. It now bypasses the ignorefile as documented.
