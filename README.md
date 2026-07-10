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
Consequently, it is _likely_ that you will see false positives.
Hopefully, the list of positives is small enough for this tool to be useful, and to be easy to manually audit.

Additional name mappings can be added to [`src/name_map.rs`](src/name_map.rs) to improve accuracy.

**Contents**

-   [Install](#install)
    -   [From crates.io](#from-cratesio)
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
This is useful when you have packages you know are false positives (e.g., `asyncpg`) and do not want them to continually show up in the output.

## How does this work?

`pyproject-udeps` parses every Python file in your project with [ruff](https://github.com/astral-sh/ruff)'s error-resilient parser and collects the imports from the AST: plain and `from` imports (wherever they are nested), plus `importlib.import_module("...")` and `__import__("...")` calls with literal arguments.
Files with syntax errors still contribute the imports that do parse, and text inside strings, docstrings, and comments is never mistaken for an import.
Those imports are then matched against the declared dependencies using an embedded name map and a handful of naming-convention heuristics; whatever is never matched gets reported.

Some dependencies are legitimately never imported by your code.
For example, sqlalchemy's async sessions might depend on `asyncpg`, even though your immediate project never imports it.
To help with that (somewhat), you can use the option (`--virtualenv`) to include searching through all the Python files in your virtualenv as well.

## Related Tools

- [deptry](https://github.com/fpgmaas/deptry) (python/rust): Find unused, missing and transitive dependencies in a Python project.
- [pip-extra-reqs](https://github.com/r1chardj0n3s/pip-check-reqs) (python): find packages that should be in requirements for a project.
- [fawltydeps](https://github.com/tweag/FawltyDeps) (python): Python dependency checker.
- [py-unused-deps](https://github.com/matthewhughes934/py-unused-deps) (python): Find unused dependencies in your Python packages.
- [un-pack](https://github.com/bnkc/unpack) (rust): Unpack python packages from your project and more.

### Benchmarks

These numbers predate the rename and the switch to ruff's parser (they were measured with the original scanner as `poetry-udeps` v0.2.x); treat them as a rough baseline until they are rerun.

`pyproject-udeps` only checks for unused dependencies.
Below, we benchmark this single feature on a desktop with an AMD Ryzen 7 7800X3D and 64 GB of RAM.
The target repository is a private repository consisting of ~170k lines of Python code.

```
❯ tokei -C -t Python
===============================================================================
 Language            Files        Lines         Code     Comments       Blanks
===============================================================================
 Python                904       194995       167640         9686        17669
===============================================================================
 Total                 904       194995       167640         9686        17669
===============================================================================
```

#### Results

##### poetry-udeps (now pyproject-udeps)
```
❯ hyperfine --warmup 2 -i 'poetry-udeps'
Benchmark 1: poetry-udeps
  Time (mean ± σ):     110.3 ms ±   0.7 ms    [User: 203.2 ms, System: 15.8 ms]
  Range (min … max):   108.9 ms … 111.6 ms    27 runs

  Warning: Ignoring non-zero exit code.
```

##### deptry
For `deptry`, only `DEP002` (unused dependencies) is considered.
Note this is running deptry v0.14.0, with core parts rewritten in Rust.

```
❯ hyperfine --warmup 2 -i 'deptry -i DEP001,DEP003,DEP004 .'
Benchmark 1: deptry -i DEP001,DEP003,DEP004 .
  Time (mean ± σ):     165.2 ms ±   1.8 ms    [User: 389.4 ms, System: 38.9 ms]
  Range (min … max):   161.6 ms … 168.8 ms    18 runs
```

##### pip-extra-reqs
`pip-extra-reqs` was unable to run on this project.

```
❯ pip-extra-reqs .
Traceback (most recent call last):
  ...
UnicodeDecodeError: 'utf-8' codec can't decode byte 0xb1 in position 81: invalid start byte
```

##### fawltydeps
```
❯ hyperfine --warmup 2 -i 'fawltydeps --check-unused --deps pyproject.toml'
Benchmark 1: fawltydeps --check-unused --deps pyproject.toml
  Time (mean ± σ):      3.570 s ±  0.015 s    [User: 3.179 s, System: 0.379 s]
  Range (min … max):    3.549 s …  3.595 s    10 runs
```

##### py-unused-deps

I was unable to successfully run `py-unused-deps` on this project.

## Trophy Case

This is a list of cases where unused dependencies were found using `pyproject-udeps`. You are welcome to expand it:

- TODO

## License

This tool is distributed under the terms of the Blue Oak license.
Any contributions are licensed under the same license, and acknowledge via the [Developer Certificate of Origin](https://developercertificate.org/).

See [LICENSE](LICENSE.md) for details.
