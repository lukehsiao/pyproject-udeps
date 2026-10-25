---
"pyproject-udeps": patch
---

**perf**: Python files are now parsed on one thread per CPU, up to 12, not on one thread, which makes a scan of the 6,281-file standard library about 7 times faster.

With `--virtualenv`, the project and its virtualenv are now scanned at the same time.
