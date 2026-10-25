---
"pyproject-udeps": patch
---

**fix**: a dotted declared spelling with uppercase letters, such as `Acme.Widgets`, now matches `from Acme import Widgets` and imports below `Acme.Widgets`, not only `import Acme.Widgets`.
