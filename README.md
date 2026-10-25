<h1 align="center">
    🪚<br>
    pyproject-udeps
</h1>

<div align="center">
    <strong>Find unused dependencies in pyproject.toml.</strong>
</div>
<br>
<div align="center">
  <a href="https://github.com/lukehsiao/pyproject-udeps/actions/workflows/general.yml">
    <img src="https://img.shields.io/github/actions/workflow/status/lukehsiao/pyproject-udeps/general.yml" alt="Build Status">
  </a>
  <a href="https://crates.io/crates/pyproject-udeps">
    <img src="https://img.shields.io/crates/v/pyproject-udeps" alt="Version">
  </a>
  <a href="https://github.com/lukehsiao/pyproject-udeps/blob/main/LICENSE.md">
    <img src="https://img.shields.io/crates/l/pyproject-udeps" alt="License">
  </a>
</div>
<br>

`pyproject-udeps` is inspired by [`cargo-udeps`](https://github.com/est31/cargo-udeps) and finds unused dependencies declared in `pyproject.toml`.
It works with [Poetry](https://python-poetry.org/), [uv](https://docs.astral.sh/uv/), and plain [PEP 621](https://peps.python.org/pep-0621/) projects.
It was previously published as `poetry-udeps`.

Python dependencies do not always map 1:1 with their import names.
Consequently, it is _likely_ that you will see false positives: packages flagged as unused even though your code imports them under a name the tool does not recognize.
You will also see packages that are correctly flagged as never imported but still do something at runtime (e.g., database drivers selected via connection string); those are not detection errors, just an inherent limit of import-based analysis.
Hopefully, both lists are small enough for this tool to be useful, and to be easy to manually audit.

Additional name mappings can be added to [`src/name_map.toml`](src/name_map.toml) to improve accuracy.

**Contents**

-   [Install](#install)
    -   [From crates.io](#from-cratesio)
    -   [Arch](#arch)
    -   [mise](#mise)
    -   [In GitHub Actions](#in-github-actions)
-   [Usage](#usage)
-   [How does this work?](#how-does-this-work)
-   [Related Tools](#related-tools)
    -   [Benchmarks](#benchmarks)
-   [Trophy Case](#trophy-case)
-   [License](#license)

## Install

### From crates.io

```
cargo install pyproject-udeps --locked
```

Or, with [cargo-binstall](https://github.com/cargo-bins/cargo-binstall) to download a prebuilt binary:

```
cargo binstall pyproject-udeps
```

### Arch

On Arch Linux, install from the [AUR](https://aur.archlinux.org/) using your preferred helper (e.g. [`paru`](https://github.com/Morganamilo/paru) or [`yay`](https://github.com/Jguer/yay)):

```
paru -S pyproject-udeps       # builds from source
paru -S pyproject-udeps-bin   # prebuilt binary
```

### mise

[mise](https://mise.jdx.dev/) can install a prebuilt binary directly from the GitHub release, verifying its checksum and SLSA provenance:

```
mise use -g github:lukehsiao/pyproject-udeps
```

### In GitHub Actions

Prebuilt binaries install quickly in CI via [taiki-e/install-action](https://github.com/taiki-e/install-action):

```yaml
jobs:
  unused-deps:
    runs-on: ubuntu-latest
    permissions:
      contents: read
    steps:
      - uses: actions/checkout@v5
        with:
          persist-credentials: false
      - uses: taiki-e/install-action@v2
        with:
          tool: pyproject-udeps
      - name: Check for unused dependencies
        run: pyproject-udeps
```

The step fails (exit code 1) when unused dependencies are found and prints them one per line, sorted by package name.

## Usage

This is meant to be run in the root of your project, next to `pyproject.toml`.

```
Find unused dependencies in pyproject.toml

Usage: pyproject-udeps [OPTIONS]

Options:
  -v, --verbose...
          Increase logging verbosity

  -q, --quiet...
          Decrease logging verbosity

  -e, --virtualenv
          Look for dependency usage in the project virtualenv.

          Assumes you have already installed all dependencies. The virtualenv is discovered from the
          project's lockfile and tool tables: poetry projects via `poetry env info -p`, uv projects
          via `$UV_PROJECT_ENVIRONMENT`, and other PEP 621 projects via `$VIRTUAL_ENV`, each falling
          back to `.venv`.

  -d, --dev
          Look for unused dependencies in dev-dependencies.

          Many projects include dev deps like CLI tools that are intentionally not directly used in
          the codebase.

      --no-ignore
          Do not ignore the packages in the ignorefile.

          The ignorefile is .pyprojectudepsignore, or the legacy .poetryudepsignore as a fallback.

  -h, --help
          Print help (see a summary with '-h')

  -V, --version
          Print version
```

Dependencies are read from every place they can be declared, so mixed and hybrid projects work: `[tool.poetry.dependencies]`, PEP 621 `[project.dependencies]` and `[project.optional-dependencies]`, every `[tool.poetry.group.*]`, PEP 735 `[dependency-groups]`, and legacy `[tool.uv] dev-dependencies`.

### Using `.pyprojectudepsignore`

`pyproject-udeps` supports ignoring packages from a `.pyprojectudepsignore` file (the legacy `.poetryudepsignore` name is also honored).
This file is a simple text file with 1 package name per line, in any spelling: `pyyaml` also ignores a declared `PyYAML`.
Empty lines, and lines starting with `#` are ignored.
This is useful for packages you never want to see in the output again, whether they are false positives (imported under a name the tool does not know) or genuinely never imported but still needed at runtime (e.g., `asyncpg`, which sqlalchemy selects via connection string without your code ever importing it).

## How does this work?

`pyproject-udeps` parses every Python file in your project with [ruff](https://github.com/astral-sh/ruff)'s error-resilient parser and collects the imports from the AST: plain and `from` imports (wherever they are nested), plus `importlib.import_module("...")` and `__import__("...")` calls with literal arguments.
Files with syntax errors still contribute the imports that do parse, and text inside strings, docstrings, and comments is never mistaken for an import.
Those imports are then matched against the declared dependencies using an embedded name map and a handful of naming-convention heuristics; whatever is never matched gets reported.

Some dependencies are legitimately never imported by your code.
For example, sqlalchemy's async sessions might depend on `asyncpg`, even though your immediate project never imports it.
To help with that (somewhat), you can use the option (`--virtualenv`) to include searching through all the Python files in your virtualenv as well.

## Related Tools

- [deptry](https://github.com/fpgmaas/deptry) (python/rust): Find unused, missing and transitive dependencies in a Python project.
- [creosote](https://github.com/fredrikaverpil/creosote) (python): Identify unused dependencies and avoid a bloated virtual environment.
- [fawltydeps](https://github.com/tweag/FawltyDeps) (python): Python dependency checker.
- [pip-extra-reqs](https://github.com/r1chardj0n3s/pip-check-reqs) (python): find packages that should be in requirements for a project.
- [py-unused-deps](https://github.com/matthewhughes934/py-unused-deps) (python): Find unused dependencies in your Python packages.
- [pytomlcleaner](https://github.com/t3an/pytomlcleaner) (python): find and remove unused dependencies in pyproject.toml.
- [un-pack](https://github.com/bnkc/unpack) (rust): Unpack python packages from your project and more.
  Dormant since 2024.

### Benchmarks

`pyproject-udeps` only checks for unused dependencies, so that is the single feature benchmarked here.

The target is [PrefectHQ/prefect](https://github.com/PrefectHQ/prefect) at release [`3.9.0`](https://github.com/PrefectHQ/prefect/tree/1ef3e516d3b3449d19f65b30c45d1e0bff656396) (2026-10-21): a popular, actively developed project with a standard uv-based PEP 621 layout and 57 declared runtime dependencies.
It is large enough to stress every tool, and public, so these numbers are reproducible.

```
❯ tokei -C -t Python
 Language              Files        Lines         Code     Comments       Blanks
 Python                 1926       712105       589454        20079       102572
```

Benchmarks were run with [hyperfine](https://github.com/sharkdp/hyperfine) on a desktop with an AMD Ryzen 7 7800X3D and 64 GB of RAM (Arch Linux, Python 3.14.8), with each tool in its recommended configuration against a fully `uv sync`ed checkout.

![Bar chart of mean wall-clock time per tool when checking PrefectHQ/prefect for unused dependencies. pyproject-udeps is fastest at 35 milliseconds, followed by deptry at 0.28 s, py-unused-deps at 1.73 s, creosote at 2.27 s, pytomlcleaner at 4.06 s, fawltydeps at 6.18 s, and pip-extra-reqs at 6.96 s.](contrib/assets/benchmark.svg)

#### Speed

| Tool | Mean [ms] | Min [ms] | Max [ms] | Relative |
|:---|---:|---:|---:|---:|
| `pyproject-udeps` | 35.0 ± 3.3 | 30.5 | 43.0 | 1.00 |
| `deptry` | 282.0 ± 4.3 | 275.3 | 288.9 | 8.07 ± 0.77 |
| `py-unused-deps` | 1732.0 ± 77.7 | 1658.1 | 1886.3 | 49.55 ± 5.18 |
| `creosote` | 2271.3 ± 14.3 | 2249.1 | 2295.1 | 64.98 ± 6.15 |
| `pytomlcleaner` | 4061.1 ± 54.1 | 3970.8 | 4122.9 | 116.19 ± 11.08 |
| `fawltydeps` | 6179.3 ± 67.4 | 6067.4 | 6288.4 | 176.78 ± 16.80 |
| `pip-extra-reqs` | 6955.8 ± 29.9 | 6902.3 | 7007.4 | 199.00 ± 18.81 |

<details>
<summary>Reproduction commands</summary>

```console
❯ git clone --depth 1 --branch 3.9.0 https://github.com/PrefectHQ/prefect  # 1ef3e516d3b3449d19f65b30c45d1e0bff656396
❯ cd prefect && uv sync --python 3.14
❯ hyperfine --warmup 2 -i \
    'pyproject-udeps' \
    'deptry . --ignore DEP001,DEP003,DEP004,DEP005' \
    'py-unused-deps -d prefect src' \
    'creosote' \
    'pytomlcleaner' \
    'pip-extra-reqs --requirements-file requirements-direct.txt src' \
    'fawltydeps --check-unused --deps pyproject.toml'
```

The deptry `--ignore` restricts it to `DEP002` (unused dependencies), matching what the other tools check.
deptry, pip-extra-reqs (pip-check-reqs), and py-unused-deps were installed into the project's venv because they resolve import names from installed package metadata; the rest ran from a separate tool venv.
`requirements-direct.txt` is `[project.dependencies]` written out one requirement per line.
pip-check-reqs 3.1.0 can also read `pyproject.toml` directly and reports the same packages.
Prefect 3.9.0 requires uv 0.13.4 or newer.
The chart is generated from [contrib/benchmark.vl.json](contrib/benchmark.vl.json) with [vl-convert](https://github.com/vega/vl-convert).

</details>

#### Quality

Speed only matters if the output is trustworthy, so every reported package was audited by hand against the Prefect source.
Throughout this audit, "false positive" means a detection error: the tool reported a package that the repository does import.
The audit covers the main dependencies and every extra.
Six declared dependencies are verifiably never imported anywhere in the repository: `aiosqlite`, `jinja2-humanize-extension`, `rfc3339-validator`, and `semver` from the main dependencies, plus `opentelemetry-distro` and `opentelemetry-instrumentation-logging` from the `otel` extra.
A perfect import-scanning tool reports those six and nothing else.
Whether each of the six is safe to actually *remove* is a separate question, revisited after the audit notes.

| Tool | Version | Reported | Verified never-imported (of 6) | False positives |
|:---|:---|---:|---:|:---|
| `pyproject-udeps` | 0.4.0 | 6 | 6 | 0 |
| `deptry` | 0.25.1 | 4 | 4 | 0 |
| `py-unused-deps` | 0.4.2 | 5 | 4 | 1 |
| `creosote` | 5.2.0 | 7 | 4 | 3 |
| `pytomlcleaner` | 1.0.0 | 0 | 0 | 0 |
| `pip-extra-reqs` | 3.1.0 | 5 | 4 | 1 |
| `fawltydeps` | 0.20.0 | 28 | 4 | 12, plus 12 dev-group flags |

Notes from the audit, in the same order:

- **pyproject-udeps** found all six with no false positives.
  It correctly credits two `opentelemetry-*` packages that are [imported lazily inside a function body](https://github.com/PrefectHQ/prefect/blob/1ef3e516d3b3449d19f65b30c45d1e0bff656396/src/prefect/_internal/metrics.py#L79-L84) deep in `src/prefect/_internal/metrics.py`, a pattern that trips up several of the other tools.
  Accuracy rests on two things: [`src/name_map.toml`](src/name_map.toml) is audited against the wheel contents of the 5,000 most-downloaded PyPI packages (see [contrib/name-map-audit](contrib/name-map-audit/)), and the matcher accepts any dotted prefix of an import path, so deep namespace imports find their package.
  Expect the occasional false positive on packages the map does not know yet, and please send a PR when you hit one.
- **deptry** must run from inside the project's venv; installed elsewhere it cannot resolve import names and its report balloons to 17 packages with 11 false positives.
  In-venv it has no false positives, but it decides usage by mapping each dependency to the top-level modules its installed distribution claims, which makes namespace siblings indistinguishable.
  That costs it two of the six: `opentelemetry-distro` and `opentelemetry-instrumentation-logging` install into the shared `opentelemetry/` namespace directory that `opentelemetry-api` (genuinely used) also provides.
  It also skips `tests/` by default.
- **py-unused-deps** requires the project distribution to be installed.
  Its one false positive (`griffe`) is shared with pip-extra-reqs and fawltydeps: griffe 2.x became a metapackage whose importable module ships in `griffelib`, so its metadata claims no modules.
  It never evaluates the two `opentelemetry-*` packages, because extras are only considered when passed via its `-e` flag.
- **creosote**'s three false positives (`apprise`, `dateparser`, `whenever`) are all imports that sit inside a function body or a conditional branch, which [its scanner skips](https://github.com/fredrikaverpil/creosote/blob/v5.2.0/src/creosote/parsers.py#L346) unless you pass `--include-deferred`.
  With that flag it reports exactly the four main-dependency packages, with no false positives.
  It reads only `[project.dependencies]` by default, so it never evaluates the `otel` extra.
- **pytomlcleaner** reported nothing at all: zero false positives, but also zero of the six.
  [Its matching logic](https://github.com/t3an/pytomlcleaner/blob/c8b059a03c5772808a32f09b129f1b7caa925c68/src/pytomlcleaner/cleaner.py#L200-L208) (`is_similar`, at a commit whose `cleaner.py` matches the 1.0.0 wheel) explains why: every dotted path segment and imported symbol in the codebase becomes a "used" token (4,738 of them here, including single letters), and a dependency is credited as used when any token is a substring of its name or is 60% similar by difflib.
  The token `e` alone is a substring of all six names, so on a codebase this large the report is structurally empty.
- **pip-extra-reqs** false-positived only on `griffe` (metapackage, as above).
  It skips `pendulum`, which is guarded by a `python_version<'3.13'` marker and therefore not installed in the Python 3.14 venv, and says it cannot tell whether it is used.
- **fawltydeps** checks every extra and dependency group by default.
  Beyond `griffe`, it flags 11 integration extras (`prefect-azure`, `prefect-kubernetes`, ...) that are imported only by their own package inside `src/integrations/`, and 12 dev-group entries: mostly CLI tools that are never imported (`mkdocs-*`, `vale`, `virtualenv`, ...), plus `pillow` and Prefect itself.

Now, back to whether the six are safe to remove.
None of them are, and the failure modes differ.
Three break loudly: `aiosqlite` is SQLAlchemy's async SQLite driver, selected by the [default `sqlite+aiosqlite://` connection string](https://github.com/PrefectHQ/prefect/blob/1ef3e516d3b3449d19f65b30c45d1e0bff656396/src/prefect/settings/models/_defaults.py#L77); `jinja2-humanize-extension` is [loaded by a string module path](https://github.com/PrefectHQ/prefect/blob/1ef3e516d3b3449d19f65b30c45d1e0bff656396/src/prefect/server/utilities/user_templates.py#L82) when Prefect builds its Jinja environment; and `pydantic_extra_types.semantic_version` [raises at import if `semver` is missing](https://github.com/pydantic/pydantic-extra-types/blob/v2.11.1/pydantic_extra_types/semantic_version.py#L9-L14), and Prefect [imports that type in its block tests](https://github.com/PrefectHQ/prefect/blob/1ef3e516d3b3449d19f65b30c45d1e0bff656396/tests/blocks/test_core.py#L15), so the test suite breaks ([#18363](https://github.com/PrefectHQ/prefect/pull/18363) added the dependency together with the test).
Three degrade silently, which is worse.
jsonschema imports `rfc3339-validator` [under `contextlib.suppress(ImportError)`](https://github.com/python-jsonschema/jsonschema/blob/v4.26.0/jsonschema/_format.py#L395-L402) to register its `date-time` format check, so without it [Prefect's parameter validation](https://github.com/PrefectHQ/prefect/blob/1ef3e516d3b3449d19f65b30c45d1e0bff656396/src/prefect/utilities/schema_tools/validation.py#L111-L116) quietly stops rejecting malformed timestamps.
`opentelemetry-distro` and `opentelemetry-instrumentation-logging` are activated through entry points by `opentelemetry-instrument`, which [Prefect's own load-testing harness launches the server under](https://github.com/PrefectHQ/prefect/blob/1ef3e516d3b3449d19f65b30c45d1e0bff656396/load_testing/run-server.sh#L87); with no distro installed, it [falls back to a default one](https://github.com/open-telemetry/opentelemetry-python-contrib/blob/v0.65b0/opentelemetry-instrumentation/src/opentelemetry/instrumentation/auto_instrumentation/_load.py#L51-L73) without an error.

All six are load-bearing without ever being imported.
Reporting them is not a false positive, since the claim "never imported" is true, but they are exactly what an ignorefile (or `--virtualenv`) is for.
Running `pyproject-udeps -e` searches the installed environment too, finds the imports living inside `sqlalchemy`, `jsonschema`, and `pydantic_extra_types`, and narrows the report to `jinja2-humanize-extension` and `opentelemetry-distro`.
It also drops `opentelemetry-instrumentation-logging`, but only because the package imports its own modules, not because anything else uses it.

## Trophy Case

This is a list of cases where unused dependencies were found using `pyproject-udeps`.
You are welcome to expand it:

- [Chainlit/chainlit@#2980 ("chore(deps): drop unused audioop-lts core dependency")](https://github.com/Chainlit/chainlit/pull/2980)
- [feast-dev/feast@#6599 ("chore: Remove unused jsonschema dependency, declare attrs")](https://github.com/feast-dev/feast/pull/6599)
- [mealie-recipes/mealie@#7872 ("chore(deps): remove unused aiofiles dependency")](https://github.com/mealie-recipes/mealie/pull/7872)
- [mitmproxy/mitmproxy@#8319 ("build(deps): remove unused msgpack dependency")](https://github.com/mitmproxy/mitmproxy/pull/8319)

## License

This tool is distributed under the terms of the Blue Oak license.
Any contributions are licensed under the same license, and acknowledge via the [Developer Certificate of Origin](https://developercertificate.org/).

See [LICENSE](LICENSE.md) for details.
