<h1 align="center">
    🪚<br>
    poetry-udeps → pyproject-udeps
</h1>

<div align="center">
    <strong>This crate has been renamed to <a href="https://crates.io/crates/pyproject-udeps">pyproject-udeps</a>.</strong>
</div>
<br>

The tool now supports [Poetry](https://python-poetry.org/), [uv](https://docs.astral.sh/uv/), and plain [PEP 621](https://peps.python.org/pep-0621/) projects, so it took the name of the file it actually analyzes.

This final `poetry-udeps` release is a thin wrapper around `pyproject-udeps`: it still works, but prints a reminder to switch. Install the new name instead:

```
cargo install pyproject-udeps --locked
```

or, for a prebuilt binary:

```
cargo binstall pyproject-udeps
```

Development continues at <https://github.com/lukehsiao/pyproject-udeps>.
