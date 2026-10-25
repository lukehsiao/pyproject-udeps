"""Audit src/name_map.toml against the wheel contents in modules.json.

Simulates the matching in src/matching.rs and reports every module a
package ships that would not mark the package as used, map modules that add
nothing, and map modules the wheel does not define. With --write, applies
the additions and drops the modules that add nothing. Run fetch_modules.py
first.
"""

import argparse
import json
import re
import sys
import tomllib
from collections import defaultdict
from functools import cache
from pathlib import Path

HERE = Path(__file__).parent
TOP = HERE / "top.json"
MODULES = HERE / "modules.json"
MAP = HERE.parent.parent / "src" / "name_map.toml"

# Top-level modules that are packaging accidents or that a project's own
# code often defines. Mapping one would let a project's own `tests` or `cli`
# mark a dependency as used. Names ending in `tests` are junk too.
JUNK = {
    "Demos",
    "alloy",
    "benchmark",
    "benchmarks",
    "bin",
    "build",
    "build_backend",
    "build_utils",
    "certs",
    "ci",
    "cli",
    "common_util",
    "conftest",
    "datastore",
    "dev",
    "doc",
    "docs",
    "example",
    "examples",
    "exceptions",
    "hatch_build",
    "interactive",
    "misc",
    "saml2test",
    "samples",
    "scripts",
    "services",
    "setup",
    "simulation",
    "src",
    "studio",
    "test",
    "testfunctions",
    "testing",
    "third_party",
    "tools",
    "utils",
    "version",
}

# Wheel directories that a package's .pth file adds to sys.path, so the
# modules inside them import as top-level names (`win32/lib/win32con.py` is
# `import win32con`). Deeper directories come first so they strip first.
PTH_DIRS = {"pywin32": ["win32.lib", "win32", "pythonwin"]}


def normalize(name):
    """PEP 503 normalization, as PackageName::new() in src/package.rs."""
    return re.sub(r"[-_.]+", "-", name).lower()


def import_keys(module):
    """Mirror of import_keys() in src/matching.rs for `import module`: every
    dotted prefix of the module, plus dbt.<name> for a dbt adapter. The Rust
    version also extends `from a.b import c` to a.b.c, which yields the same
    keys as `import a.b.c`."""
    keys = prefixes(module)
    parts = module.split(".")
    if parts[:2] == ["dbt", "adapters"] and len(parts) >= 3:
        keys.append(f"dbt.{parts[2]}")
    return keys


def convention_keys(package):
    """The keys package_keys() in src/matching.rs derives without the map,
    for the normalized spelling. Other spellings add keys and never remove
    them, so what this covers is covered for every spelling."""
    n = normalize(package)
    return {n.replace("-", "."), n.replace("-", "_")}


def matches(keys, module):
    """Whether importing `module`, or anything below it, hits a key. Map
    modules are dotted paths already, so they are keys as written."""
    return not keys.isdisjoint(import_keys(module))


def is_evidence(module):
    """Whether importing `module` can show that a package is used.

    A standard-library name resolves to the standard library, so its import
    says nothing about a backport such as enum34 or futures.
    """
    top = module.split(".")[0]
    return not (top in JUNK or top.endswith("tests") or top in sys.stdlib_module_names)


def prefixes(module):
    """`a`, `a.b`, `a.b.c` for `a.b.c`."""
    parts = module.split(".")
    return [".".join(parts[:k]) for k in range(1, len(parts) + 1)]


def audit(known, module_sets):
    """Compare the map with wheel data.

    A package's entry points are the shallowest modules it defines that no
    complement shares. A complement is another package that ships modules
    below the same module, mostly different ones: an import below that
    module may then come from either package. databricks-sdk and
    databricks-sql-connector both ship a pkgutil-style
    `databricks/__init__.py` above disjoint subpackages, so databricks-sdk
    enters at databricks.sdk. When one package mostly contains the other's
    modules, as a fork, a slim build, or a drop-in replacement does, the
    two are interchangeable, so mlflow-tracing and mlflow-skinny both enter
    at `mlflow`. Namespace directories define no module, so
    google-cloud-storage enters at google.cloud.storage. Every entry point
    must match.

    Returns (additions, suspect): unmatched entry points per package, and
    map modules the wheel does not define that either nothing in the wheel
    lies below (a release dropped them) or a complement shares, as the
    `opentelemetry` namespace that every opentelemetry-* package ships into.
    """
    nesters = defaultdict(set)
    for package, modules in module_sets.items():
        for module in modules:
            for prefix in prefixes(module)[:-1]:
                nesters[prefix].add(package)

    @cache
    def below(package, module):
        return frozenset(m for m in module_sets[package] if m.startswith(f"{module}."))

    @cache
    def has_complement(package, module):
        own = below(package, module)
        for other in nesters[module] - {package}:
            theirs = below(other, module)
            if 2 * len(own & theirs) < min(len(own), len(theirs)):
                return True
        return False

    additions, suspect = {}, {}
    for package, defined in module_sets.items():
        mapped = known.get(package, [])
        if doubtful := [
            m
            for m in mapped
            if defined
            and m not in defined
            and (not below(package, m) or has_complement(package, m))
        ]:
            suspect[package] = doubtful
        # A module with nothing below it has no complement and qualifies
        # itself, so a module without an entry point has descendants with one.
        entry_points = {
            next((p for p in prefixes(m) if p in defined and not has_complement(package, p)), None)
            for m in defined
            if is_evidence(m)
        }
        entry_points.discard(None)
        keys = convention_keys(package) | set(mapped)
        if add := sorted(m for m in entry_points if not matches(keys, m)):
            additions[package] = add
    return additions, suspect


def removable(known):
    """Map modules that add nothing: they are no evidence of use, or the
    rest of their package's keys already match them."""
    found = {}
    for package, modules in known.items():
        for module in modules:
            others = {m for m in modules if m != module}
            if not is_evidence(module) or matches(convention_keys(package) | others, module):
                found.setdefault(package, []).append(module)
    return found


def read_map():
    """Returns (header, map). The header is the leading comment block."""
    lines = MAP.read_text().splitlines(keepends=True)
    split = next(
        (i for i, line in enumerate(lines) if line.strip() and not line.startswith("#")),
        len(lines),
    )
    # Package and module names cannot contain `#`, so any `#` past the
    # header starts a comment.
    for i, line in enumerate(lines[split:], split + 1):
        if "#" in line:
            sys.exit(f"{MAP}:{i}: comments outside the header would be lost")
    return "".join(lines[:split]), tomllib.loads("".join(lines))


def write_map(header, known):
    """Rewrites the map sorted, one entry per line where it fits in 79
    columns and one module per line where it does not."""
    entries = []
    for package, modules in sorted(known.items()):
        quoted = [f'"{m}"' for m in sorted(modules)]
        line = f"{package} = [{', '.join(quoted)}]\n"
        if len(line) > 80:
            line = f"{package} = [\n" + "".join(f"    {q},\n" for q in quoted) + "]\n"
        entries.append(line)
    MAP.write_text(header + "".join(entries))


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--write", action="store_true", help="apply the report to src/name_map.toml"
    )
    args = parser.parse_args()
    # Python 3.13 removed the last PEP 594 modules, such as `cgi`. On older
    # versions, removable() would drop backports such as legacy-cgi.
    if sys.version_info < (3, 13):
        sys.exit("run on Python 3.13 or newer, which decides the stdlib names")

    rank = {
        normalize(r["project"]): i
        for i, r in enumerate(json.loads(TOP.read_text())["rows"], 1)
    }
    data = {normalize(p): info for p, info in json.loads(MODULES.read_text()).items()}
    module_sets = {p: set(info["modules"]) for p, info in data.items() if "modules" in info}
    for package, dirs in PTH_DIRS.items():
        if package in module_sets:
            module_sets[package] = {
                next((m.removeprefix(f"{d}.") for d in dirs if m.startswith(f"{d}.")), m)
                for m in module_sets[package]
            }
    header, known = read_map()
    additions, suspect = audit(known, module_sets)
    for package, modules in additions.items():
        known[package] = sorted({*known.get(package, []), *modules})
    # Judged after the additions, which can cover modules already listed.
    drop = removable(known)

    def report(title, entries):
        print(f"\n{title}: {len(entries)}")
        for package in sorted(entries, key=lambda p: rank.get(p, sys.maxsize)):
            print(f"  #{rank.get(package, '?'):<5} {package:45} {entries[package]}")

    errors = sum("error" in info for info in data.values())
    empty = sum(not modules for modules in module_sets.values())
    print(f"{len(data)} packages: {errors} unreadable, {empty} ship no modules")
    report("additions", additions)
    report("removable map modules", drop)
    report("suspect map modules, dropped or shared (review by hand)", suspect)

    if args.write:
        for package, modules in drop.items():
            if kept := [m for m in known[package] if m not in modules]:
                known[package] = kept
            else:
                del known[package]
        write_map(header, known)
        print(f"\nwrote {MAP}")


if __name__ == "__main__":
    main()
