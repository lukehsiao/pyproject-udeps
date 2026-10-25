---
"pyproject-udeps": patch
---

**fix**: the Python interpreter requirement is never reported, however it is spelled, so `Python = "^3.11"` no longer shows up as an unused dependency.
