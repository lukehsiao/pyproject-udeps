---
"pyproject-udeps": patch
---

**feat**: declared package names are now matched case-insensitively with `-`, `_`, and `.` treated as equivalent (PEP 503 normalization), and imports now match packages keyed to any dotted prefix of the module path (so `from airflow.providers.common.sql.hooks.sql import X` marks `apache-airflow-providers-common-sql` as used). Together these eliminate whole classes of false positives, like declaring `Flask` while importing `flask`, or importing a deep submodule of a namespace package.
