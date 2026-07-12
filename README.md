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

Additional name mappings can be added to [`src/name_map.rs`](src/name_map.rs) to improve accuracy.

**Contents**

-   [Install](#install)
    -   [From crates.io](#from-cratesio)
    -   [Arch](#arch)
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

The step fails (exit code 1) when unused dependencies are found and prints them one per line.

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
          via `$UV_PROJECT_ENVIRONMENT` or `.venv`, and other PEP 621 projects via `$VIRTUAL_ENV` or
          `.venv`.

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
This file is a simple text file with 1 package name per line.
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
- [un-pack](https://github.com/bnkc/unpack) (rust): Unpack python packages from your project and more. Dormant since 2024.

### Benchmarks

`pyproject-udeps` only checks for unused dependencies, so that is the single feature benchmarked here.

The target is [PrefectHQ/prefect](https://github.com/PrefectHQ/prefect) at commit [`0e74350`](https://github.com/PrefectHQ/prefect/tree/0e7435055e18952aa8604dab78507b087a18defb) (2026-07-11): a popular, actively developed project with a standard uv-based PEP 621 layout and 57 declared runtime dependencies.
It is large enough to stress every tool, and public, so these numbers are reproducible.

```
❯ tokei -C -t Python
 Language              Files        Lines         Code     Comments       Blanks
 Python                 1885       680528       562678        19365        98485
```

Benchmarks were run with [hyperfine](https://github.com/sharkdp/hyperfine) on a desktop with an AMD Ryzen 7 7800X3D and 64 GB of RAM (Arch Linux, Python 3.14.5), with each tool in its recommended configuration against a fully `uv sync`ed checkout.

![Bar chart of mean wall-clock time per tool when checking PrefectHQ/prefect for unused dependencies. pyproject-udeps is fastest at 0.22 seconds, followed by deptry at 0.27 s, py-unused-deps at 1.59 s, creosote at 2.11 s, pytomlcleaner at 3.73 s, pip-extra-reqs at 4.75 s, and fawltydeps at 5.41 s.](contrib/assets/benchmark.svg)

#### Speed

| Tool | Mean [ms] | Min [ms] | Max [ms] | Relative |
|:---|---:|---:|---:|---:|
| `pyproject-udeps` | 217.2 ± 2.3 | 213.4 | 221.0 | 1.00 |
| `deptry` | 268.8 ± 4.9 | 262.6 | 277.2 | 1.24 ± 0.03 |
| `py-unused-deps` | 1587.8 ± 9.4 | 1573.7 | 1604.3 | 7.31 ± 0.09 |
| `creosote` | 2097.7 ± 12.4 | 2075.0 | 2119.7 | 9.66 ± 0.12 |
| `pytomlcleaner` | 3701.9 ± 31.9 | 3655.6 | 3750.1 | 17.04 ± 0.23 |
| `pip-extra-reqs` | 4744.7 ± 20.2 | 4716.7 | 4777.0 | 21.84 ± 0.25 |
| `fawltydeps` | 5402.0 ± 34.2 | 5348.3 | 5456.5 | 24.87 ± 0.31 |

<details>
<summary>Reproduction commands</summary>

```console
❯ git clone --depth 1 https://github.com/PrefectHQ/prefect  # 0e7435055e18952aa8604dab78507b087a18defb
❯ cd prefect && uv sync
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
pip-check-reqs cannot read `pyproject.toml`, so `requirements-direct.txt` is `[project.dependencies]` written out one requirement per line.
The chart is generated from [contrib/benchmark.vl.json](contrib/benchmark.vl.json) with [vl-convert](https://github.com/vega/vl-convert).

</details>

#### Quality

Speed only matters if the output is trustworthy, so every reported package was audited by hand against the Prefect source.
Throughout this audit, "false positive" means a detection error: the tool reported a package that the repository does import.
Seven declared dependencies are verifiably never imported anywhere in the repository: `aiosqlite`, `jinja2-humanize-extension`, `rfc3339-validator`, `ruamel.yaml.clib`, and `semver` from the main dependencies, plus `opentelemetry-distro` and `opentelemetry-instrumentation-logging` from the `otel` extra.
A perfect import-scanning tool reports those seven and nothing else.
Whether each of the seven is safe to actually *remove* is a separate question, revisited after the audit notes.

| Tool | Version | Reported | Verified never-imported (of 7) | False positives |
|:---|:---|---:|---:|:---|
| `pyproject-udeps` | 0.3.5 | 7 | 7 | 0 |
| `deptry` | 0.25.1 | 4 | 4 | 0 |
| `py-unused-deps` | 0.4.2 | 5 | 4 | 1 |
| `creosote` | 5.2.0 | 9 | 4 | 5 |
| `pytomlcleaner` | 1.0.0 | 0 | 0 | 0 |
| `pip-extra-reqs` | 2.5.6 | 7 | 5 | 2 |
| `fawltydeps` | 0.20.0 | 28 | 4 | 1, plus 23 dev-group flags |

Notes from the audit, in the same order:

- **pyproject-udeps** found all seven with no false positives. It correctly credits two `opentelemetry-*` packages that are [imported lazily inside function bodies](https://github.com/PrefectHQ/prefect/blob/0e7435055e18952aa8604dab78507b087a18defb/src/prefect/_internal/metrics.py#L79-L83) deep in `src/prefect/_internal/metrics.py`, a pattern that trips up several of the other tools. Accuracy rests on two things: [`src/name_map.rs`](src/name_map.rs) is audited against the wheel contents of the 1,500 most-downloaded PyPI packages (see [contrib/name-map-audit](contrib/name-map-audit/)), and the matcher accepts any dotted prefix of an import path, so deep namespace imports find their package. Expect the occasional false positive on packages the map does not know yet, and please send a PR when you hit one.
- **deptry** must run from inside the project's venv; installed elsewhere it cannot resolve import names and its report balloons to 16 packages with 7 false positives. In-venv it has no false positives, but crediting whole namespace packages means it misses `ruamel.yaml.clib` and both `opentelemetry-*` extras. It also skips `tests/` by default.
- **py-unused-deps** requires the project distribution to be installed. Its one false positive (`griffe`) is shared with the other metadata-driven tools: griffe 2.x became a metapackage whose importable module ships in `griffelib`, so its metadata claims no modules.
- **creosote**'s five false positives (`apprise`, `dateparser`, `pendulum`, `pydantic_extra_types`, `whenever`) are all imports that sit inside a function body or a conditional branch, which its scanner does not see.
- **pytomlcleaner** reported nothing at all: zero false positives, but also zero of the seven. [Its matching logic](https://github.com/t3an/pytomlcleaner/blob/c8b059a03c5772808a32f09b129f1b7caa925c68/src/pytomlcleaner/cleaner.py#L161) (`is_similar` and `identify_unused`, at the commit matching the 1.0.0 wheel) explains why: every dotted path segment and imported symbol in the codebase becomes a "used" token (4,604 of them here, including single letters), and a dependency is credited as used when any token is a substring of its name or is 60% similar by difflib. At that bar `semver` is credited by the token `ever` and `ruamel.yaml.clib` by `cli`, so on a codebase this large the report is structurally empty.
- **pip-extra-reqs** false-positived on `griffe` (metapackage, as above) and `pendulum`, which is guarded by a `python_version<'3.13'` marker and therefore not installed in the Python 3.14 venv it inspects.
- **fawltydeps** checks every declared dependency group by default, so beyond `griffe` it flags 23 dev-group entries: CLI tools that are never imported (`mkdocs-*`, `vale`, `virtualenv`, ...) and Prefect's own workspace packages, including `prefect` itself.

Now, back to whether the seven are safe to remove. None of them are, and the failure modes differ. Two break loudly: `aiosqlite` is SQLAlchemy's async SQLite driver, selected by the [`sqlite+aiosqlite://` connection string](https://github.com/PrefectHQ/prefect/blob/0e7435055e18952aa8604dab78507b087a18defb/src/prefect/settings/models/_defaults.py#L77), and `jinja2-humanize-extension` is [loaded by a string module path](https://github.com/PrefectHQ/prefect/blob/0e7435055e18952aa8604dab78507b087a18defb/src/prefect/server/utilities/user_templates.py#L82) when Prefect builds its Jinja environment. Four degrade silently, which is worse: jsonschema imports `rfc3339-validator` [under `contextlib.suppress(ImportError)`](https://github.com/python-jsonschema/jsonschema/blob/v4.26.0/jsonschema/_format.py#L393-L402) to register its `date-time` format check, so without it [Prefect's parameter validation](https://github.com/PrefectHQ/prefect/blob/0e7435055e18952aa8604dab78507b087a18defb/src/prefect/utilities/schema_tools/validation.py#L56) quietly stops rejecting malformed timestamps; [`ruamel.yaml.clib`](https://pypi.org/project/ruamel.yaml.clib/) is the C accelerator behind ruamel.yaml, so parsing just gets slower; and `opentelemetry-distro` and `opentelemetry-instrumentation-logging` are activated through entry points by `opentelemetry-instrument`, which [Prefect's own load-testing harness launches the server under](https://github.com/PrefectHQ/prefect/blob/0e7435055e18952aa8604dab78507b087a18defb/load_testing/run-server.sh#L87), so telemetry auto-configuration and log instrumentation vanish without an error. The last one is the sneakiest: nothing in the repository mentions `semver` outside lockfiles and docs prose, and it looks like the one genuinely dead dependency. We drafted the removal PR, and the test suite caught it: `pydantic_extra_types.semantic_version` [raises at import if `semver` is missing](https://github.com/pydantic/pydantic-extra-types/blob/v2.11.1/pydantic_extra_types/semantic_version.py#L8-L13), and Prefect [imports that type in its block tests](https://github.com/PrefectHQ/prefect/blob/0e7435055e18952aa8604dab78507b087a18defb/tests/blocks/test_core.py#L15) because block fields of that type are a supported serialization case ([#18363](https://github.com/PrefectHQ/prefect/pull/18363) added the dependency together with the test).

All seven are load-bearing without ever being imported. Reporting them is not a false positive, since the claim "never imported" is true, but they are exactly what an ignorefile (or `--virtualenv`) is for. Running `pyproject-udeps -e` searches the installed environment too, finds the imports living inside `sqlalchemy`, `jsonschema`, `pydantic_extra_types`, and the opentelemetry packages, and narrows the report to the three packages nothing imports anywhere: `jinja2-humanize-extension`, `opentelemetry-distro`, and `ruamel.yaml.clib`.

## Trophy Case

This is a list of cases where unused dependencies were found using `pyproject-udeps`. You are welcome to expand it:

- TODO

## License

This tool is distributed under the terms of the Blue Oak license.
Any contributions are licensed under the same license, and acknowledge via the [Developer Certificate of Origin](https://developercertificate.org/).

See [LICENSE](LICENSE.md) for details.
