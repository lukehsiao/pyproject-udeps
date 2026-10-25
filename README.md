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

`pyproject-udeps` finds the unused dependencies that `pyproject.toml` declares.
It takes its idea from [`cargo-udeps`](https://github.com/est31/cargo-udeps).
It works with [Poetry](https://python-poetry.org/), [uv](https://docs.astral.sh/uv/), and plain [PEP 621](https://peps.python.org/pep-0621/) projects.
Its old name was `poetry-udeps`.

The name of a Python package is not always the name that you import.
Thus, you will _likely_ see false positives: the tool reports a package as unused, but your code imports it under a name that the tool does not know.
You will also see packages that your code never imports but that still do work at runtime, for example a database driver that a connection string selects.
These packages are not detection errors.
Import-based analysis cannot find their runtime use.
We hope that both lists stay short, so that the tool is useful and you can examine its report by hand.

To make the tool more accurate, add name mappings to [`src/name_map.toml`](src/name_map.toml).

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

To download a prebuilt binary instead, use [cargo-binstall](https://github.com/cargo-bins/cargo-binstall):

```
cargo binstall pyproject-udeps
```

### Arch

On Arch Linux, install from the [AUR](https://aur.archlinux.org/) with your AUR helper, for example [`paru`](https://github.com/Morganamilo/paru) or [`yay`](https://github.com/Jguer/yay):

```
paru -S pyproject-udeps       # builds from source
paru -S pyproject-udeps-bin   # prebuilt binary
```

### mise

[mise](https://mise.jdx.dev/) can install a prebuilt binary from the GitHub release.
It checks the checksum and the SLSA provenance of the binary:

```
mise use -g github:lukehsiao/pyproject-udeps
```

### In GitHub Actions

In CI, [taiki-e/install-action](https://github.com/taiki-e/install-action) installs a prebuilt binary quickly:

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

The step fails with exit code 1 when it finds unused dependencies.
It prints them one per line, sorted by package name.

## Usage

Run the tool in the root of your project, next to `pyproject.toml`.

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

The tool reads dependencies from every place that can declare them, so mixed and hybrid projects work: `[tool.poetry.dependencies]`, PEP 621 `[project.dependencies]` and `[project.optional-dependencies]`, every `[tool.poetry.group.*]`, PEP 735 `[dependency-groups]`, and legacy `[tool.uv] dev-dependencies`.

### Using `.pyprojectudepsignore`

`pyproject-udeps` can ignore the packages that a `.pyprojectudepsignore` file lists.
It also reads the legacy name, `.poetryudepsignore`.
The file is plain text, with one package name on each line.
Any spelling of the name works: `pyyaml` also ignores a declared `PyYAML`.
The tool skips empty lines and lines that start with `#`.
Use the file for packages that you never want to see in the report again.
Some of these are false positives, which your code imports under a name that the tool does not know.
Others are packages that your code never imports but that are necessary at runtime, for example `asyncpg`.
SQLAlchemy selects `asyncpg` through a connection string, so your code never imports it.

## How does this work?

`pyproject-udeps` parses each Python file in your project with the error-resilient parser of [ruff](https://github.com/astral-sh/ruff).
It collects the imports from the syntax tree: plain and `from` imports at any depth, and calls to `importlib.import_module("...")` and `__import__("...")` that have a literal argument.
A file with syntax errors still gives the imports that parse.
The tool never takes the text in strings, docstrings, or comments as an import.
Then it matches the imports to the declared dependencies, with an embedded name map and some rules for naming conventions.
It reports each dependency that no import matches.

Your code never imports some dependencies, and that is correct.
For example, the async sessions of SQLAlchemy can need `asyncpg`, but your project does not import it.
The `--virtualenv` option helps with this a little: it also searches all the Python files in your virtualenv.

## Related Tools

- [deptry](https://github.com/fpgmaas/deptry) (python/rust): Find unused, missing and transitive dependencies in a Python project.
- [creosote](https://github.com/fredrikaverpil/creosote) (python): Identify unused dependencies and avoid a bloated virtual environment.
- [fawltydeps](https://github.com/tweag/FawltyDeps) (python): Python dependency checker.
- [pip-extra-reqs](https://github.com/r1chardj0n3s/pip-check-reqs) (python): find packages that should be in requirements for a project.
- [py-unused-deps](https://github.com/matthewhughes934/py-unused-deps) (python): Find unused dependencies in your Python packages.
- [pytomlcleaner](https://github.com/t3an/pytomlcleaner) (python): find and remove unused dependencies in pyproject.toml.
- [un-pack](https://github.com/bnkc/unpack) (rust): Unpack python packages from your project and more.
  It has had no changes since 2024.

### Benchmarks

`pyproject-udeps` only finds unused dependencies, so this benchmark measures only that feature.

The target is [PrefectHQ/prefect](https://github.com/PrefectHQ/prefect) at release [`3.9.0`](https://github.com/PrefectHQ/prefect/tree/1ef3e516d3b3449d19f65b30c45d1e0bff656396) (2026-10-21).
It is a popular project in active development, with a standard uv-based PEP 621 layout and 57 declared runtime dependencies.
It is large enough to put a load on each tool, and it is public, so you can reproduce these numbers.

```
❯ tokei -C -t Python
 Language              Files        Lines         Code     Comments       Blanks
 Python                 1926       712105       589454        20079       102572
```

The benchmarks ran with [hyperfine](https://github.com/sharkdp/hyperfine) on a desktop with an AMD Ryzen 7 7800X3D and 64 GB of RAM (Arch Linux, Python 3.14.8).
Each tool ran in its recommended configuration against a checkout after a full `uv sync`.

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

The deptry `--ignore` limits it to `DEP002` (unused dependencies), which is what the other tools check.
deptry, pip-extra-reqs (from pip-check-reqs), and py-unused-deps resolve import names from the metadata of installed packages, so they ran from the project's venv.
The other tools ran from a separate tool venv.
`requirements-direct.txt` holds `[project.dependencies]`, with one requirement on each line.
pip-check-reqs 3.1.0 can also read `pyproject.toml` directly, and it reports the same packages.
Prefect 3.9.0 needs uv 0.13.4 or newer.
[vl-convert](https://github.com/vega/vl-convert) makes the chart from [contrib/benchmark.vl.json](contrib/benchmark.vl.json).

</details>

#### Quality

Speed is only useful if the report is correct, so a manual audit checked each reported package against the Prefect source.
In this audit, a "false positive" is a detection error: the tool reported a package that the repository imports.
The audit covers the main dependencies and every extra.
The repository never imports six of its declared dependencies: `aiosqlite`, `jinja2-humanize-extension`, `rfc3339-validator`, and `semver` from the main dependencies, and `opentelemetry-distro` and `opentelemetry-instrumentation-logging` from the `otel` extra.
A perfect import-scanning tool reports these six and nothing else.
Whether you can safely *remove* each of the six is a different question.
The text after the audit notes answers it.

| Tool | Version | Reported | Verified never-imported (of 6) | False positives |
|:---|:---|---:|---:|:---|
| `pyproject-udeps` | 0.4.0 | 6 | 6 | 0 |
| `deptry` | 0.25.1 | 4 | 4 | 0 |
| `py-unused-deps` | 0.4.2 | 5 | 4 | 1 |
| `creosote` | 5.2.0 | 7 | 4 | 3 |
| `pytomlcleaner` | 1.0.0 | 0 | 0 | 0 |
| `pip-extra-reqs` | 3.1.0 | 5 | 4 | 1 |
| `fawltydeps` | 0.20.0 | 28 | 4 | 12, plus 12 dev-group flags |

The audit notes, in the same order:

- **pyproject-udeps** found all six, with no false positives.
  It correctly credits two `opentelemetry-*` packages that `src/prefect/_internal/metrics.py` [imports lazily in a function body](https://github.com/PrefectHQ/prefect/blob/1ef3e516d3b3449d19f65b30c45d1e0bff656396/src/prefect/_internal/metrics.py#L79-L84).
  This pattern causes problems for several of the other tools.
  Its accuracy comes from two things.
  First, an audit checks [`src/name_map.toml`](src/name_map.toml) against the wheel contents of the 5,000 most-downloaded PyPI packages (see [contrib/name-map-audit](contrib/name-map-audit/)).
  Second, the matcher accepts any dotted prefix of an import path, so a deep namespace import finds its package.
  Expect some false positives for packages that the map does not know yet.
  When you find one, please send a PR.
- **deptry** must run from the project's venv.
  When it runs from a different location, it cannot resolve import names, and it reports 17 packages with 11 false positives.
  In the venv, it has no false positives.
  But it decides usage from the top-level modules that each installed distribution claims, so it cannot tell namespace siblings apart.
  Because of this, it misses two of the six: `opentelemetry-distro` and `opentelemetry-instrumentation-logging` install into the shared `opentelemetry/` namespace directory.
  `opentelemetry-api`, which Prefect uses, also provides that directory.
  It also skips `tests/` by default.
- **py-unused-deps** needs the project distribution to be installed.
  Its one false positive, `griffe`, also occurs with pip-extra-reqs and fawltydeps.
  griffe 2.x is now a metapackage, and its importable module ships in `griffelib`, so the griffe metadata claims no modules.
  py-unused-deps never examines the two `opentelemetry-*` packages, because it reads extras only when you give them with its `-e` flag.
- **creosote** has three false positives: `apprise`, `dateparser`, and `whenever`.
  Each is an import in a function body or in a conditional branch, and [its scanner skips these](https://github.com/fredrikaverpil/creosote/blob/v5.2.0/src/creosote/parsers.py#L346) unless you give `--include-deferred`.
  With that flag, it reports exactly the four main-dependency packages, with no false positives.
  By default it reads only `[project.dependencies]`, so it never examines the `otel` extra.
- **pytomlcleaner** reported nothing: no false positives, but also none of the six.
  [Its matching logic](https://github.com/t3an/pytomlcleaner/blob/c8b059a03c5772808a32f09b129f1b7caa925c68/src/pytomlcleaner/cleaner.py#L200-L208) shows why (`is_similar`, at a commit whose `cleaner.py` is the same as in the 1.0.0 wheel).
  Each dotted path segment and each imported symbol in the codebase becomes a "used" token.
  Here there are 4,738 of them, and some are single letters.
  A dependency counts as used when a token is a substring of its name, or when difflib finds the two 60% similar.
  The token `e` alone is a substring of all six names, so on a codebase of this size the report is always empty.
- **pip-extra-reqs** has one false positive, `griffe` (a metapackage, as above).
  It skips `pendulum`, because a `python_version<'3.13'` marker keeps `pendulum` out of the Python 3.14 venv.
  It says that it cannot tell if `pendulum` is used.
- **fawltydeps** checks every extra and every dependency group by default.
  In addition to `griffe`, it reports 11 integration extras (`prefect-azure`, `prefect-kubernetes`, ...).
  Only their own package in `src/integrations/` imports them.
  It also reports 12 dev-group entries.
  Most are CLI tools that nothing imports (`mkdocs-*`, `vale`, `virtualenv`, ...), and the others are `pillow` and Prefect itself.

Now, can you safely remove the six?
You cannot remove any of them, and they fail in different ways.
Three fail with an error:

- `aiosqlite` is the async SQLite driver of SQLAlchemy.
  The [default `sqlite+aiosqlite://` connection string](https://github.com/PrefectHQ/prefect/blob/1ef3e516d3b3449d19f65b30c45d1e0bff656396/src/prefect/settings/models/_defaults.py#L77) selects it.
- Prefect [loads `jinja2-humanize-extension` by a string module path](https://github.com/PrefectHQ/prefect/blob/1ef3e516d3b3449d19f65b30c45d1e0bff656396/src/prefect/server/utilities/user_templates.py#L82) when it builds its Jinja environment.
- `pydantic_extra_types.semantic_version` [raises an error at import when `semver` is missing](https://github.com/pydantic/pydantic-extra-types/blob/v2.11.1/pydantic_extra_types/semantic_version.py#L9-L14).
  Prefect [imports that type in its block tests](https://github.com/PrefectHQ/prefect/blob/1ef3e516d3b3449d19f65b30c45d1e0bff656396/tests/blocks/test_core.py#L15), so the test suite fails.
  [#18363](https://github.com/PrefectHQ/prefect/pull/18363) added the dependency and the test together.

Three fail silently, which is worse:

- jsonschema imports `rfc3339-validator` [in `contextlib.suppress(ImportError)`](https://github.com/python-jsonschema/jsonschema/blob/v4.26.0/jsonschema/_format.py#L395-L402) to add its `date-time` format check.
  Without it, [Prefect's parameter validation](https://github.com/PrefectHQ/prefect/blob/1ef3e516d3b3449d19f65b30c45d1e0bff656396/src/prefect/utilities/schema_tools/validation.py#L111-L116) stops rejecting malformed timestamps, and nothing tells you.
- `opentelemetry-instrument` activates `opentelemetry-distro` and `opentelemetry-instrumentation-logging` through entry points.
  [Prefect's load-testing harness starts the server with it](https://github.com/PrefectHQ/prefect/blob/1ef3e516d3b3449d19f65b30c45d1e0bff656396/load_testing/run-server.sh#L87).
  When no distro is installed, `opentelemetry-instrument` [uses a default distro](https://github.com/open-telemetry/opentelemetry-python-contrib/blob/v0.65b0/opentelemetry-instrumentation/src/opentelemetry/instrumentation/auto_instrumentation/_load.py#L51-L73) and shows no error.

Prefect never imports these six, but each one does necessary work.
A report of them is not a false positive, because "never imported" is true.
An ignorefile, or `--virtualenv`, is the correct tool for them.
`pyproject-udeps -e` also searches the installed environment.
It finds the imports in `sqlalchemy`, `jsonschema`, and `pydantic_extra_types`.
A package's imports of its own modules do not count, so the report becomes `jinja2-humanize-extension`, `opentelemetry-distro`, and `opentelemetry-instrumentation-logging`.

## Trophy Case

This list shows cases where `pyproject-udeps` found unused dependencies.
You can add to it:

- [Chainlit/chainlit@#2980 ("chore(deps): drop unused audioop-lts core dependency")](https://github.com/Chainlit/chainlit/pull/2980)
- [feast-dev/feast@#6599 ("chore: Remove unused jsonschema dependency, declare attrs")](https://github.com/feast-dev/feast/pull/6599)
- [mealie-recipes/mealie@#7872 ("chore(deps): remove unused aiofiles dependency")](https://github.com/mealie-recipes/mealie/pull/7872)
- [mitmproxy/mitmproxy@#8319 ("build(deps): remove unused msgpack dependency")](https://github.com/mitmproxy/mitmproxy/pull/8319)

## License

This tool uses the Blue Oak license.
All contributions use the same license, and each contribution acknowledges the [Developer Certificate of Origin](https://developercertificate.org/).

See [LICENSE](LICENSE.md) for details.
