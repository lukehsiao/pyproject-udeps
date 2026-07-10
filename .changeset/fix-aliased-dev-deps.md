---
"pyproject-udeps": patch
---

**fix**: stop reporting used dev dependencies that import under a different name.

A dev dependency imported through an alias (for example `scikit-learn` imported as `sklearn`) was still reported as unused under `--dev`, because the match removed the wrong bookkeeping entry.
