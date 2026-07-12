"""Resolve importable top-level modules for top PyPI packages.

For each package: PyPI JSON -> pick a wheel -> HTTP range-read the zip
central directory from files.pythonhosted.org -> extract module names.
Results cached to modules.json so reruns are cheap.
"""

import json
import struct
import sys
import urllib.request
from concurrent.futures import ThreadPoolExecutor, as_completed
from pathlib import Path

OUT = Path(__file__).parent / "modules.json"
TOP = Path(__file__).parent / "top.json"
N = int(sys.argv[1]) if len(sys.argv) > 1 else 1500

UA = {"User-Agent": "pyproject-udeps-name-map-audit/0.1 (lukehsiao)"}


def get(url, rng=None, timeout=30):
    headers = dict(UA)
    if rng:
        headers["Range"] = rng
    req = urllib.request.Request(url, headers=headers)
    with urllib.request.urlopen(req, timeout=timeout) as r:
        return r.read()


def central_dir_names(url, size):
    """List filenames in a remote zip by range-reading its central directory."""
    # files.pythonhosted.org returns 501 for suffix ranges, so compute an
    # absolute range from the size PyPI's JSON API reports.
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
    # The central directory may already be inside the tail we fetched.
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


def modules_from_names(names):
    """Derive importable module paths from wheel member paths."""
    tops = set()
    deep = set()
    for n in names:
        if not n or n.startswith((".", "__")):
            continue
        first = n.split("/", 1)[0]
        if first.endswith((".dist-info", ".data")):
            continue
        if n.endswith(".py"):
            parts = n[:-3].split("/")
            if parts[-1] == "__init__":
                parts = parts[:-1]
                if parts:
                    deep.add(".".join(parts))
            elif len(parts) == 1:
                tops.add(parts[0])
            tops.add(parts[0])
        elif ".cpython-" in n or n.endswith((".so", ".pyd")):
            parts = n.split("/")
            base = parts[-1].split(".", 1)[0]
            if len(parts) == 1:
                tops.add(base)
            else:
                tops.add(parts[0])
    # Keep every __init__-bearing package path up to 4 segments so namespace
    # packages (google.cloud.storage, azure.storage.blob) are all visible to
    # the matcher simulation, not just one path per top-level namespace.
    mods = set(tops)
    for d in deep:
        if d.count(".") <= 3 and not any(p.startswith("_") for p in d.split(".")):
            mods.add(d)
    return sorted(m for m in mods if m and not m.startswith("_"))


def pick_wheel(pkg):
    data = json.loads(get(f"https://pypi.org/pypi/{pkg}/json"))
    wheels = [u for u in data["urls"] if u["packagetype"] == "bdist_wheel"]
    if not wheels:
        return None
    wheels.sort(key=lambda u: (0 if "none-any" in u["filename"] else 1, u["size"]))
    return wheels[0]["url"], wheels[0]["size"]


def resolve(pkg):
    try:
        picked = pick_wheel(pkg)
        if not picked:
            return pkg, {"error": "no wheel"}
        url, size = picked
        return pkg, {"modules": modules_from_names(central_dir_names(url, size))}
    except Exception as e:
        return pkg, {"error": f"{type(e).__name__}: {e}"}


def main():
    if not TOP.exists():
        raw = get(
            "https://hugovk.github.io/top-pypi-packages/top-pypi-packages.min.json"
        )
        TOP.write_bytes(raw)
    top = json.loads(TOP.read_text())["rows"][:N]
    pkgs = [r["project"] for r in top]

    cache = json.loads(OUT.read_text()) if OUT.exists() else {}
    todo = [p for p in pkgs if p not in cache]
    print(f"{len(pkgs)} packages, {len(todo)} to fetch")

    done = 0
    with ThreadPoolExecutor(max_workers=8) as ex:
        futs = {ex.submit(resolve, p): p for p in todo}
        for fut in as_completed(futs):
            pkg, res = fut.result()
            cache[pkg] = res
            done += 1
            if done % 100 == 0:
                print(f"  {done}/{len(todo)}")
                OUT.write_text(json.dumps(cache))
    OUT.write_text(json.dumps(cache))
    errs = sum(1 for p in pkgs if "error" in cache.get(p, {}))
    print(f"done: {len(pkgs)} resolved, {errs} errors")


if __name__ == "__main__":
    main()
