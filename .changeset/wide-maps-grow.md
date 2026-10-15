---
"pyproject-udeps": patch
---

**fix**: the name map now covers the 5,000 most-downloaded PyPI packages and lists every module a package provides, so `import attr` marks `attrs` used, `import bson` marks `pymongo` used, and so on.
A deep import below a mapped module now matches too (`from google.protobuf.json_format import X` marks `protobuf` used), as does `from dbt.adapters import postgres` for `dbt-postgres`, and a declared spelling such as `Google_Auth` matches `import google.auth`.
A standard-library import no longer marks a backport used, so `unicodedata2` is reported when the code only imports `unicodedata`.
