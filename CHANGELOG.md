# Changelog

## 0.3.1

### Patch Changes

- [`bb6a635`](https://github.com/lukehsiao/pyproject-udeps/commit/bb6a635c4949c2baef175d2b3ae12875eb5e29b2) - **build**: fix publishing token.

<pre>
$ git-stats v0.3.0..v0.3.1
Author           Commits  Changed Files  Insertions  Deletions  Net Δ
Luke Hsiao             1              1          +5          0     +5
dependabot[bot]        1              4          +6         -6      0
Total                  2              5         +11         -6     +5
</pre>

## 0.3.0

### Minor Changes

- [#4](https://github.com/lukehsiao/pyproject-udeps/pull/4) [`e25c754`](https://github.com/lukehsiao/pyproject-udeps/commit/e25c754fd593afcee86292ed994821ccdb825524) - **feat**: publish prebuilt binaries on GitHub releases.

  Releases now attach binaries for ten targets (Linux gnu/musl on x86_64, aarch64, and armv7; macOS x86_64 and aarch64; Windows x86_64 and aarch64) with sha256 checksums, so `cargo binstall pyproject-udeps` works and CI can install via `taiki-e/install-action` without a compile.

- [#4](https://github.com/lukehsiao/pyproject-udeps/pull/4) [`e25c754`](https://github.com/lukehsiao/pyproject-udeps/commit/e25c754fd593afcee86292ed994821ccdb825524) - **feat**: rename `poetry-udeps` to `pyproject-udeps`.

  The tool is no longer poetry-only, so the name follows the file it actually analyzes. Install it with `cargo install pyproject-udeps` (or `cargo binstall pyproject-udeps`); the binary is now `pyproject-udeps`. The ignorefile is `.pyprojectudepsignore`, and an existing `.poetryudepsignore` keeps working as a fallback. One last `poetry-udeps` release on crates.io points here.

- [#4](https://github.com/lukehsiao/pyproject-udeps/pull/4) [`e25c754`](https://github.com/lukehsiao/pyproject-udeps/commit/e25c754fd593afcee86292ed994821ccdb825524) - **feat**: parse Python with ruff's parser instead of scanning text for import statements.

  Imports are now collected from a real AST, which fixes a family of accuracy bugs: `from x import a, b` counted only `a`, `from x import *` counted nothing, `'''`-quoted docstrings were not skipped, and text like `x = "import os"` produced phantom imports. Imports nested in functions and `try`/`except ImportError` blocks are found, files with syntax errors still contribute whatever parses, and `importlib.import_module("...")` and `__import__("...")` calls with literal arguments now count as usage. Reports may legitimately change on upgrade: dependencies that only appeared inside strings or docstrings will newly show up as unused.

- [#4](https://github.com/lukehsiao/pyproject-udeps/pull/4) [`e25c754`](https://github.com/lukehsiao/pyproject-udeps/commit/e25c754fd593afcee86292ed994821ccdb825524) - **feat**: support uv and plain PEP 621 projects, not just poetry.

  Dependencies are now read from every place `pyproject.toml` can declare them: `[project.dependencies]`, every `[project.optional-dependencies]` extra, `[tool.poetry.dependencies]`, every `[tool.poetry.group.*]` (previously only the `dev` group was checked), PEP 735 `[dependency-groups]`, and the legacy `[tool.uv] dev-dependencies` array. Hybrid layouts take the union, so a wrong guess about which tool manages the project can never drop a declaration. `--virtualenv` also stopped assuming poetry: the environment is discovered from the lockfile and tool tables, using `poetry env info -p` for poetry projects, `$UV_PROJECT_ENVIRONMENT` or `.venv` for uv projects, and `$VIRTUAL_ENV` or `.venv` otherwise.

### Patch Changes

- [#4](https://github.com/lukehsiao/pyproject-udeps/pull/4) [`e25c754`](https://github.com/lukehsiao/pyproject-udeps/commit/e25c754fd593afcee86292ed994821ccdb825524) - **fix**: stop reporting used dev dependencies that import under a different name.

  A dev dependency imported through an alias (for example `scikit-learn` imported as `sklearn`) was still reported as unused under `--dev`, because the match removed the wrong bookkeeping entry.

- [#4](https://github.com/lukehsiao/pyproject-udeps/pull/4) [`e25c754`](https://github.com/lukehsiao/pyproject-udeps/commit/e25c754fd593afcee86292ed994821ccdb825524) - **fix**: stop crashing on `import dbt.adapters` and on non-UTF-8 Python files.

  A bare two-segment `import dbt.adapters` panicked the matcher, and a single `.py` file with non-UTF-8 bytes (latin-1 comments in legacy code, say) crashed the project scan. Both now behave: the dbt heuristic only fires with an adapter segment present, and files are read lossily everywhere.

- [#4](https://github.com/lukehsiao/pyproject-udeps/pull/4) [`e25c754`](https://github.com/lukehsiao/pyproject-udeps/commit/e25c754fd593afcee86292ed994821ccdb825524) - **fix**: honor the `--no-ignore` flag.

  The flag was accepted but never read, so there was no way to see the report without ignorefile filtering. It now bypasses the ignorefile as documented.

All notable changes to this project will be documented in this file. See [conventional commits](https://www.conventionalcommits.org/) for commit guidelines.

<pre>
$ git-stats v0.2.10..v0.3.0
Author      Commits  Changed Files  Insertions  Deletions  Net Δ
Luke Hsiao       25            101       +6233      -1906  +4327
Total            25            101       +6233      -1906  +4327
</pre>

---

## [0.2.10](https://github.com/lukehsiao/poetry-udeps/compare/v0.2.9..v0.2.10) - 2025-04-21

### Bug Fixes

- return 0 exit code if ignorefile filters all entries - ([496c2cb](https://github.com/lukehsiao/poetry-udeps/commit/496c2cba44b62a583f82684e081ef85bba87da38)) - Luke Hsiao

---

## [0.2.9](https://github.com/lukehsiao/poetry-udeps/compare/v0.2.8..v0.2.9) - 2025-04-21

### Bug Fixes

- add json-stream and Markdown to name_map - ([c8ee3ea](https://github.com/lukehsiao/poetry-udeps/commit/c8ee3ea9a9b4ec03b0a5d6a8a00844f76cf8d345)) - Luke Hsiao

---

## [0.2.8](https://github.com/lukehsiao/poetry-udeps/compare/v0.2.7..v0.2.8) - 2025-04-21

### Features

- support poetry 2.x using PEP 621 - ([8f91c86](https://github.com/lukehsiao/poetry-udeps/commit/8f91c86942dcafc96d58ab534f9d11350b022fd1)) - Luke Hsiao

---

## [0.2.7](https://github.com/lukehsiao/poetry-udeps/compare/v0.2.6..v0.2.7) - 2024-12-11

### Bug Fixes

- clarify help text for flag behavior - ([3e6ffce](https://github.com/lukehsiao/poetry-udeps/commit/3e6ffcee257dd7db6660a3d1d208ca83caaa9784)) - Luke Hsiao

---

## [0.2.6](https://github.com/lukehsiao/poetry-udeps/compare/v0.2.5..v0.2.6) - 2024-12-11

### Features

- add support for `.poetryudepsignore` - ([97be5d3](https://github.com/lukehsiao/poetry-udeps/commit/97be5d34a6817711082d27d43a21c9115960eef9)) - Luke Hsiao

---

## [0.2.5](https://github.com/lukehsiao/poetry-udeps/compare/v0.2.4..v0.2.5) - 2024-12-11

### Bug Fixes

- **(name-map)** add `vl-convert-python`'s alias - ([c03d52d](https://github.com/lukehsiao/poetry-udeps/commit/c03d52d4c4d7147d8203c4c3f7023dbef5552d0a)) - Luke Hsiao

### Documentation

- **(LICENSE)** use `md` extension for nicer rendering - ([c7f71c5](https://github.com/lukehsiao/poetry-udeps/commit/c7f71c561b04c9ad1edcf51a715324306e151c53)) - Luke Hsiao
- **(README)** update link to license file - ([fbb08ce](https://github.com/lukehsiao/poetry-udeps/commit/fbb08ce2f4c8d829e27a779f20fc24438c95d6d2)) - Nicholas Chiang

---

## [0.2.4](https://github.com/lukehsiao/poetry-udeps/compare/v0.2.3..v0.2.4) - 2024-10-17

This release adds a couple more names to the name map to reduce false positives.

### Documentation

- **(README)** update benchmarks for deptry 0.14.0 - ([ee2eb53](https://github.com/lukehsiao/poetry-udeps/commit/ee2eb533a55e324713729e6a66ce1d139b7da53a)) - Luke Hsiao
- **(README)** add `un-pack` as related work - ([0805323](https://github.com/lukehsiao/poetry-udeps/commit/0805323735fa47c140bc26cce68f7d868dfb8120)) - Luke Hsiao

### Refactor

- enable and fix pedantic clippy lints - ([a07d11d](https://github.com/lukehsiao/poetry-udeps/commit/a07d11d2a73c6d5ca0932e4f483a7d43a4ad2e46)) - Luke Hsiao
- use `tracing_log` to simplify main - ([dde6945](https://github.com/lukehsiao/poetry-udeps/commit/dde6945a861d8c1efc1a00220ff18bee06f7d8f7)) - Luke Hsiao

---

## [0.2.3](https://github.com/lukehsiao/poetry-udeps/compare/v0.2.2..v0.2.3) - 2024-03-06

### Bug Fixes

- ignore dev-deps if they are missing - ([29e55c1](https://github.com/lukehsiao/poetry-udeps/commit/29e55c1eb7db93efd6b385b34f849c1e885e84b1)) - Luke Hsiao

### Documentation

- **(README)** add toc and description of approach - ([8892b4f](https://github.com/lukehsiao/poetry-udeps/commit/8892b4fd6c14b78d29972bc0aceb8253a847c832)) - Luke Hsiao

---

## [0.2.2](https://github.com/lukehsiao/poetry-udeps/compare/v0.2.1..v0.2.2) - 2024-01-21

### Documentation

- **(CHANGELOG)** fix links to previous tags - ([8c2e2fe](https://github.com/lukehsiao/poetry-udeps/commit/8c2e2fefad43615db0da2a049abde8e8f5de504f)) - Luke Hsiao

### Refactor

- default to error level, not warn level logs - ([47fc220](https://github.com/lukehsiao/poetry-udeps/commit/47fc22008589d6ee46d5e881dd93bbaa96ef10d4)) - Luke Hsiao
- log better msg if pyproject.toml not found - ([77a7be7](https://github.com/lukehsiao/poetry-udeps/commit/77a7be79ece6f8cbe99f8ac1fc70d3306eda8583)) - Luke Hsiao

---

## [0.2.1](https://github.com/lukehsiao/poetry-udeps/compare/v0.2.0..v0.2.1) - 2024-01-20

### Bug Fixes

- only return 1 if udeps were found - ([4cc7015](https://github.com/lukehsiao/poetry-udeps/commit/4cc7015b742bf3654f16cf164ba64b539d1bf8a3)) - Luke Hsiao

### Documentation

- **(CHANGELOG)** add entry for v0.2.1 - ([aeed851](https://github.com/lukehsiao/poetry-udeps/commit/aeed85187cf2fdac4ce98bf5da121737ece5995b)) - Luke Hsiao

---

## [0.2.0](https://github.com/lukehsiao/poetry-udeps/compare/v0.1.6..v0.2.0) - 2024-01-20

### Documentation

- **(CHANGELOG)** add entry for v0.2.0 - ([d0368a2](https://github.com/lukehsiao/poetry-udeps/commit/d0368a2f1dd007adf5fd15c62a818ca2032356bc)) - Luke Hsiao

### Refactor

- [**breaking**] return an exit code 1 if udeps were found - ([775ac08](https://github.com/lukehsiao/poetry-udeps/commit/775ac08cd0ae4b1dcc6141cef3b91f7cadf7d6ce)) - Luke Hsiao

---

## [0.1.6](https://github.com/lukehsiao/poetry-udeps/compare/v0.1.5..v0.1.6) - 2024-01-18

### Bug Fixes

- **(name_map)** add alias for `celery-redbeat` - ([c197b58](https://github.com/lukehsiao/poetry-udeps/commit/c197b58b25684bccbfbb38da23b9b10859aa1de3)) - Luke Hsiao

### Documentation

- **(CHANGELOG)** add entry for v0.1.6 - ([5ba79a9](https://github.com/lukehsiao/poetry-udeps/commit/5ba79a9c5ee02f2f20b2054ffd8dda1150edfb01)) - Luke Hsiao

### Styling

- format with cargo fmt - ([2073598](https://github.com/lukehsiao/poetry-udeps/commit/2073598446f0c0fbe39ca27c5e5d123bdf78c893)) - Luke Hsiao

---

## [0.1.5](https://github.com/lukehsiao/poetry-udeps/compare/v0.1.4..v0.1.5) - 2024-01-18

### Documentation

- **(CHANGELOG)** add entry for v0.1.5 - ([8341790](https://github.com/lukehsiao/poetry-udeps/commit/8341790778cab6e7d3d1eb87b32611d3476671e6)) - Luke Hsiao

### Refactor

- address clippy lint for `or_default()` - ([c89d6f6](https://github.com/lukehsiao/poetry-udeps/commit/c89d6f658ffb9cb7148bee0f85ebc11da6cfb01f)) - Luke Hsiao

---

## [0.1.4](https://github.com/lukehsiao/poetry-udeps/compare/v0.1.3..v0.1.4) - 2024-01-18

### Documentation

- **(CHANGELOG)** add entry for v0.1.4 - ([d7eb582](https://github.com/lukehsiao/poetry-udeps/commit/d7eb582e9190c2313d7ab49bafa1097de53a1c62)) - Luke Hsiao
- **(README)** link license badge to license - ([c6a4229](https://github.com/lukehsiao/poetry-udeps/commit/c6a4229d8feb1d3d2234547a3cc9a4a40144a3ab)) - Luke Hsiao

---

## [0.1.3](https://github.com/lukehsiao/poetry-udeps/compare/v0.1.2..v0.1.3) - 2023-08-08

### Bug Fixes

- support poetry 1.2+'s dependency groups - ([97dc80d](https://github.com/lukehsiao/poetry-udeps/commit/97dc80d65f27721fe19f4973189b197af2539ea7)) - Luke Hsiao

### Documentation

- **(CHANGELOG)** add entry for v0.1.3 - ([4eb03cf](https://github.com/lukehsiao/poetry-udeps/commit/4eb03cf971ba06722e7beeb69f71dffd7823eddf)) - Luke Hsiao
- **(README)** set expectation of false positives - ([22defbf](https://github.com/lukehsiao/poetry-udeps/commit/22defbf823cc3b3b0933286262b90dd651806f4f)) - Luke Hsiao
- **(README)** add fawltydeps, py-unused-deps - ([910671d](https://github.com/lukehsiao/poetry-udeps/commit/910671d166cf5225aadd8a07d3db4936b73182bc)) - Luke Hsiao

---

## [0.1.2](https://github.com/lukehsiao/poetry-udeps/compare/v0.1.1..v0.1.2) - 2023-08-08

### Documentation

- **(CHANGELOG)** add entry for v0.1.2 - ([9b7ee8a](https://github.com/lukehsiao/poetry-udeps/commit/9b7ee8a6c22fc2d15f40da1212405c7f2aa1c8ce)) - Luke Hsiao

### Refactor

- clean up info-level log formatting - ([33c67bc](https://github.com/lukehsiao/poetry-udeps/commit/33c67bc533e17cfedaac6653b5364cd684574b53)) - Luke Hsiao

---

## [0.1.1](https://github.com/lukehsiao/poetry-udeps/compare/v0.1.0..v0.1.1) - 2023-08-08

### Bug Fixes

- replace `.`, not `,` for google-style naming - ([b0a0827](https://github.com/lukehsiao/poetry-udeps/commit/b0a08270da535fea6bf81a5f63cbf4784f0f9e41)) - Luke Hsiao
- don't include `python` in the dependencies - ([4c50a31](https://github.com/lukehsiao/poetry-udeps/commit/4c50a31deb6b8952829273385538767c0792f83d)) - Luke Hsiao

### Documentation

- **(CHANGELOG)** add initial changelog - ([3728af4](https://github.com/lukehsiao/poetry-udeps/commit/3728af4a9cb8ce2544c30571e6ae8c4c7f430028)) - Luke Hsiao
- **(CHANGELOG)** add entry for v0.1.1 - ([1aa86ef](https://github.com/lukehsiao/poetry-udeps/commit/1aa86ef5eaa4a1eb7cea18ca839399a12d76bad5)) - Luke Hsiao
- **(README)** add benchmark - ([4943c69](https://github.com/lukehsiao/poetry-udeps/commit/4943c698bb68eafccca1a9a5feaef1c54b55588b)) - Luke Hsiao

### Features

- **(name-map)** include more common packages - ([7e7cc6d](https://github.com/lukehsiao/poetry-udeps/commit/7e7cc6dee6a7a18456c9152a569f010d419ffe8f)) - Luke Hsiao
- add debug-level logs for seeing what is checked - ([b247e87](https://github.com/lukehsiao/poetry-udeps/commit/b247e87d18855d5dde727441092d43f1b1a75b20)) - Luke Hsiao
- check two-level package names for better Google support - ([9559064](https://github.com/lukehsiao/poetry-udeps/commit/95590641da9b5887a38b7c3d953d5ff58e03a751)) - Luke Hsiao

---

## [0.1.0] - 2023-08-08

### Bug Fixes

- ensure map entries are ordered - ([473edf3](https://github.com/lukehsiao/poetry-udeps/commit/473edf384b22c4e332149cc7a66096ba0d7356ae)) - Luke Hsiao

### Documentation

- **(README)** add some TODOs for future reference - ([667b472](https://github.com/lukehsiao/poetry-udeps/commit/667b4722d347caf298b26c5ce0c8b1508d5b568d)) - Luke Hsiao
- **(README)** add badge placeholders - ([ffcc7e5](https://github.com/lukehsiao/poetry-udeps/commit/ffcc7e5143a0a7ade5128330c81d66e202bde2df)) - Luke Hsiao
- **(README)** add sections - ([17da2ec](https://github.com/lukehsiao/poetry-udeps/commit/17da2ecf6c1a8a24ba784c0f0676132122c04f21)) - Luke Hsiao
- **(README)** populate more sections - ([160d67b](https://github.com/lukehsiao/poetry-udeps/commit/160d67b3d406414e884745f95ac11dc1c05be8a3)) - Luke Hsiao
- **(README)** drop language badges for plain text - ([aee0c5b](https://github.com/lukehsiao/poetry-udeps/commit/aee0c5b3e0b0a2d6956d728afd9381273c42974a)) - Luke Hsiao
- **(README)** add benchmarks section - ([082c5cb](https://github.com/lukehsiao/poetry-udeps/commit/082c5cb0b36568fd17f8243df0d0834b6e1922df)) - Luke Hsiao
- **(changelog)** change git-cliff template - ([70279f7](https://github.com/lukehsiao/poetry-udeps/commit/70279f79f8aea46f67500e9d44bf373cb5e33e80)) - Luke Hsiao

### Refactor

- add fast recursive directory traversal - ([190203f](https://github.com/lukehsiao/poetry-udeps/commit/190203fdc77ed7434bf94447707961d4ecea0895)) - Luke Hsiao
- drop scanning jupyter notebooks for now - ([3a80557](https://github.com/lukehsiao/poetry-udeps/commit/3a80557cc567eeb2e1a24ea75609e53767325f89)) - Luke Hsiao
- parse dependencies from pyproject.toml - ([4b80a72](https://github.com/lukehsiao/poetry-udeps/commit/4b80a72d43ae56fac1dfda28cf3af139dbb3cb88)) - Luke Hsiao
- use a static map for getting deps - ([36af56a](https://github.com/lukehsiao/poetry-udeps/commit/36af56aad99676b742d9b8a91b0327471f798aa6)) - Luke Hsiao
- update nom parsers - ([935a9cf](https://github.com/lukehsiao/poetry-udeps/commit/935a9cf0d4bf35065afe25648d3b69f3145c7a23)) - Luke Hsiao
- use tracing for structured logging - ([4d7120b](https://github.com/lukehsiao/poetry-udeps/commit/4d7120bd34543d6a50d1aa13b56d5002cdbf72b4)) - Luke Hsiao
- add one variant of boto3-stubs - ([3de0e22](https://github.com/lukehsiao/poetry-udeps/commit/3de0e226e28d80bc7b06912e1e625ffec1911e25)) - Luke Hsiao

### WIP

- adding nom parser combinator for getting packages - ([a8aa949](https://github.com/lukehsiao/poetry-udeps/commit/a8aa9490fbc5aeecbc565351877fe118465ac487)) - Luke Hsiao
