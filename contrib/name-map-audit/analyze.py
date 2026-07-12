"""Simulate pyproject-udeps matching against real wheel module data.

Replicates import_names_for() and candidate_keys() from src/matching.rs and
KNOWN_NAMES from src/name_map.rs, then reports popular packages whose
imports would NOT mark the declared dependency as used. Run fetch_modules.py
first to populate modules.json.
"""

import json
import re
import sys
from pathlib import Path

HERE = Path(__file__).parent
REPO = HERE.parent.parent


def load_known_names():
    src = (REPO / "src/name_map.rs").read_text()
    return dict(re.findall(r'"([^"]+)"\s*=>\s*"([^"]+)"', src))


KNOWN = load_known_names()


def pep503(name):
    return re.sub(r"[-_.]+", "-", name).lower()


def candidate_keys(module, item=None):
    cands = []
    if item:
        cands.append(module.replace(".", "-") + "-" + item)
    parts = module.split(".")
    if module.startswith("dbt.adapters") and len(parts) >= 3:
        cands.append(parts[0] + "-" + parts[2])
    if len(parts) >= 2:
        for k in range(2, len(parts) + 1):
            cands.append("-".join(parts[:k]))
        cands.append(parts[0])
    cands.append(module)
    return cands


def keys_for(declared):
    keys = {declared}
    n = pep503(declared)
    if n in KNOWN:
        keys.add(KNOWN[n])
    underscored = n.replace("-", "_")
    if underscored != declared:
        keys.add(underscored)
    return keys


def covered(declared, modules):
    keys = keys_for(declared)
    for m in modules:
        # Model both `import a.b.c` and `from a.b import c`.
        cands = set(candidate_keys(m))
        if "." in m:
            parent, _, child = m.rpartition(".")
            cands.update(candidate_keys(parent, child))
        if keys & cands:
            return True
    return False


def main():
    top = json.loads((HERE / "top.json").read_text())["rows"]
    data = json.loads((HERE / "modules.json").read_text())

    failures = []
    errors = []
    for rank, row in enumerate(top, 1):
        pkg = row["project"]
        if pkg not in data:
            continue
        info = data[pkg]
        if "error" in info:
            errors.append((rank, pkg, info["error"]))
            continue
        mods = [m for m in info["modules"] if m not in ("test", "tests", "docs")]
        if not mods:
            failures.append((rank, pkg, [], "NO-MODULES"))
            continue
        if not covered(pkg, mods) or not covered(pep503(pkg), mods):
            failures.append((rank, pkg, mods, "MISMATCH"))

    print(f"errors: {len(errors)}")
    for r, p, e in errors[:15]:
        print(f"  #{r} {p}: {e}")
    print(f"\nfailures: {len(failures)}")
    for r, p, mods, tag in failures:
        print(f"#{r:4d} {p:45s} [{tag}] modules={mods[:8]}")


if __name__ == "__main__":
    sys.exit(main())
