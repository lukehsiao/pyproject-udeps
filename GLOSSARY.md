# pyproject-udeps

Finds dependencies a Python project declares but never imports.

## Language

### Dependencies

**Declared dependency**:
A package the project lists in its `pyproject.toml`.
_Avoid_: requirement, dep

**Main dependency**:
A declared dependency the project needs at runtime, including optional extras.
_Avoid_: runtime dependency, prod dependency

**Dev dependency**:
A declared dependency listed only in a development group or a tool's dev list.
_Avoid_: group dependency, test dependency

**Unused dependency**:
A declared dependency that no import refers to, other than its self-imports.
_Avoid_: udep, dead dependency

### Names

**Package name**:
The normalized name that identifies a package, so `ruamel.yaml`, `ruamel_yaml`, and `Ruamel-YAML` are one package.
_Avoid_: distribution, requirement, project name

**Declared spelling**:
A package's name exactly as written in the manifest.
_Avoid_: raw name, original name

**Import**:
The dotted module path that an import statement or a dynamic import call may reach, so `from a import b` is the import `a.b`.
_Avoid_: module, import statement

**Self-import**:
An import of a dependency's module from a file that the same dependency installed, as its `RECORD` lists.
_Avoid_: internal import, circular import

**Name map**:
The curated list of the modules a package provides, for packages whose modules do not follow from the package name.
_Avoid_: alias table, known modules

### Configuration

**Ignorefile**:
The user's list of packages never to report.
_Avoid_: allowlist, exclusions

**Virtualenv flavor**:
The tool that manages a project's environment: poetry, uv, or plain PEP 621.
_Avoid_: backend, package manager
