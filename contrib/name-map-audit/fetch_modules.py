"""Resolve the public modules of the most-downloaded PyPI packages.

For each package: PyPI JSON -> pick a wheel -> HTTP range-read the zip
central directory from files.pythonhosted.org -> list its modules.
Results are cached in modules.json, so reruns only fetch new packages.

Usage: python fetch_modules.py [N]   (default N = 5000)
"""

import json
import struct
import sys
import urllib.request
from concurrent.futures import ThreadPoolExecutor, as_completed
from pathlib import Path

HERE = Path(__file__).parent
TOP = HERE / "top.json"
MODULES = HERE / "modules.json"
TOP_URL = "https://hugovk.github.io/top-pypi-packages/top-pypi-packages.min.json"
UA = {"User-Agent": "pyproject-udeps-name-map-audit/0.2 (lukehsiao)"}


def get(url, rng=None, timeout=30):
    headers = dict(UA)
    if rng:
        headers["Range"] = rng
    req = urllib.request.Request(url, headers=headers)
    with urllib.request.urlopen(req, timeout=timeout) as r:
        # A 200 would return the whole file from offset 0, so the offsets
        # computed for the range would point at the wrong bytes.
        if rng and r.status != 206:
            raise ValueError(f"range request answered with {r.status}")
        return r.read()


def central_dir_names(url, size):
    """List filenames in a remote zip by range-reading its central directory."""
    # files.pythonhosted.org returns 501 for suffix ranges, so compute an
    # absolute range from the size PyPI's JSON API reports. The end of
    # central directory record is 22 bytes plus a comment of at most 65535
    # bytes, so the last 66000 bytes always contain it.
    start = max(0, size - 66000)
    tail = get(url, rng=f"bytes={start}-{size - 1}")
    eocd_pos = tail.rfind(b"PK\x05\x06")
    if eocd_pos < 0:
        raise ValueError("no EOCD in tail")
    cd_size, cd_offset = struct.unpack("<II", tail[eocd_pos + 12 : eocd_pos + 20])
    if cd_size == 0xFFFFFFFF or cd_offset == 0xFFFFFFFF:
        loc_pos = tail.rfind(b"PK\x06\x07", 0, eocd_pos)
        if loc_pos < 0:
            raise ValueError("zip64 locator missing")
        (z64_eocd_off,) = struct.unpack("<Q", tail[loc_pos + 8 : loc_pos + 16])
        z64 = get(url, rng=f"bytes={z64_eocd_off}-{z64_eocd_off + 56}")
        if z64[:4] != b"PK\x06\x06":
            raise ValueError("bad zip64 EOCD")
        cd_size, cd_offset = struct.unpack("<QQ", z64[40:56])
    cd = get(url, rng=f"bytes={cd_offset}-{cd_offset + cd_size - 1}")
    names = []
    i = 0
    while i + 46 <= len(cd):
        if cd[i : i + 4] != b"PK\x01\x02":
            break
        name_len, extra_len, comment_len = struct.unpack("<HHH", cd[i + 28 : i + 34])
        names.append(cd[i + 46 : i + 46 + name_len].decode("utf-8", "replace"))
        i += 46 + name_len + extra_len + comment_len
    return names


def module_path(name):
    """The dotted module a wheel member defines, or None if it defines none.

    `a/b.py` and `a/b.cpython-312-x86_64-linux-gnu.so` define `a.b`, and
    `a/b/__init__.py` defines the package `a.b`. Installers put the
    `*.data/purelib` and `*.data/platlib` directories in site-packages, so
    their contents count as top-level. Stubs (`.pyi`), data files, and the
    rest of `.dist-info` and `.data` define nothing.
    """
    *dirs, file = name.split("/")
    if len(dirs) >= 2 and dirs[0].endswith(".data") and dirs[1] in ("purelib", "platlib"):
        dirs = dirs[2:]
    if file.endswith(".py"):
        stem = file[:-3]
    elif file.endswith((".so", ".pyd")):
        stem = file.split(".", 1)[0]
    else:
        return None
    path = dirs if stem == "__init__" else [*dirs, stem]
    if not path or not all(part.isidentifier() for part in path):
        return None
    return tuple(path)


def public_modules(names):
    """Every public module a wheel defines, as sorted dotted paths.

    Modules with a `_`-prefixed segment are private, and mypyc's generated
    helper extensions, named `*__mypyc`, are not meant for import. Both
    are skipped.
    """
    paths = {path for name in names if (path := module_path(name))}
    return sorted(
        ".".join(path)
        for path in paths
        if not any(p.startswith("_") or p.endswith("__mypyc") for p in path)
    )


def pick_wheel(pkg):
    data = json.loads(get(f"https://pypi.org/pypi/{pkg}/json"))
    wheels = [u for u in data["urls"] if u["packagetype"] == "bdist_wheel"]
    if not wheels:
        return None
    # A pure wheel lists the same modules as any platform wheel, and the
    # smallest wheel is the cheapest to read.
    wheels.sort(key=lambda u: ("none-any" not in u["filename"], u["size"]))
    return wheels[0]["url"], wheels[0]["size"]


def resolve(pkg):
    try:
        picked = pick_wheel(pkg)
        if not picked:
            return pkg, {"error": "no wheel"}
        return pkg, {"modules": public_modules(central_dir_names(*picked))}
    except Exception as e:
        return pkg, {"error": f"{type(e).__name__}: {e}"}


def main():
    n = int(sys.argv[1]) if len(sys.argv) > 1 else 5000
    if not TOP.exists():
        TOP.write_bytes(get(TOP_URL))
    pkgs = [r["project"] for r in json.loads(TOP.read_text())["rows"][:n]]

    cache = json.loads(MODULES.read_text()) if MODULES.exists() else {}
    todo = [p for p in pkgs if p not in cache]
    print(f"{len(pkgs)} packages, {len(todo)} to fetch")

    with ThreadPoolExecutor(max_workers=8) as ex:
        futs = [ex.submit(resolve, p) for p in todo]
        for done, fut in enumerate(as_completed(futs), 1):
            pkg, res = fut.result()
            cache[pkg] = res
            if done % 100 == 0:
                print(f"  {done}/{len(todo)}")
                MODULES.write_text(json.dumps(cache))
    MODULES.write_text(json.dumps(cache))
    errs = sum(1 for p in pkgs if "error" in cache[p])
    print(f"done: {len(pkgs)} resolved, {errs} errors")


if __name__ == "__main__":
    main()
