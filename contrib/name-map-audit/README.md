# Name-map audit

Tooling to audit [`src/name_map.toml`](../../src/name_map.toml) against the modules that the most-downloaded PyPI packages actually ship, so map entries come from wheel contents rather than folklore.

1. `python fetch_modules.py 5000` downloads the top-packages list from [top-pypi-packages](https://hugovk.github.io/top-pypi-packages/) and lists each package's public modules by range-reading the central directory of its wheel from PyPI's CDN (three small HTTP requests per package, no full downloads).
   Results are cached in `modules.json`, and the list in `top.json`.
   Delete both to audit a newer list, or after changing how modules are listed.
2. `python analyze.py` replicates the matching in `src/matching.rs` and reports every module a package ships that would not mark the package as used.
   `python analyze.py --write` adds those modules to `src/name_map.toml`.
   It also drops listed modules that add nothing: the convention or another listed module already matches them, or their name is junk or a standard-library module.
   It never removes anything else, so hand-written entries survive.

Run the analyzer on the newest CPython.
`sys.stdlib_module_names` decides which backports get entries: `legacy-cgi` provides `cgi` only on Pythons that removed it from the standard library.

The analyzer maps a package at its *entry points*: the shallowest modules it defines that no complement shares.
The docstring of `audit()` in `analyze.py` explains the rule.
The report section **suspect map modules** needs a human.
It lists modules that the wheel does not define and that either a release dropped or a complement shares.
A shared entry, such as a bare `opentelemetry`, would mark the package used for imports of its siblings.
Older releases may still ship a dropped module, so decide by hand whether to remove it.

When the audit cannot see the module that users import, add an entry by hand.
For example, plugins ship into `airflow`, so the audit maps `apache-airflow-core` only below it, and the map lists `airflow` by hand.

Packages that ship no modules (`types-*` stubs, `nvidia-*` binary wheels, metapackages) are not mapped, since reporting them as never imported is correct.

Both scripts use only the Python standard library, and the analyzer refuses to run before Python 3.13.
