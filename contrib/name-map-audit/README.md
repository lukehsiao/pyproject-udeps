# Name-map audit

Tooling used to audit [`src/name_map.rs`](../../src/name_map.rs) against the import names of the most-downloaded PyPI packages, so map entries come from wheel contents rather than folklore.

1. `python fetch_modules.py 1500` downloads the top-packages list from [top-pypi-packages](https://hugovk.github.io/top-pypi-packages/) and resolves each package's importable modules by range-reading the central directory of its wheel from PyPI's CDN (two small HTTP requests per package, no full downloads).
  Results are cached in `modules.json`.
2. `python analyze.py` replicates the matching heuristics from `src/matching.rs` in Python and prints every package whose real modules would not be matched from its declared name.

Failures tagged `NO-MODULES` are usually fine: stub-only packages (`types-*`, `*-stubs`), binary library wheels (`nvidia-*`), and metapackages ship nothing importable, so reporting them as unused is correct.
`MISMATCH` failures are candidates for new `KNOWN_NAMES` entries; verify the module name against the wheel data before adding one.

Both scripts use only the Python standard library.
