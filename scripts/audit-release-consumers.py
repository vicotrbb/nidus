#!/usr/bin/env python3
"""Audit exact consumer locks and verify every third-party version against its registry."""
from concurrent.futures import ThreadPoolExecutor
import json
from pathlib import Path
import subprocess
import sys
import time
import tomllib
import urllib.request

root = Path(__file__).resolve().parent.parent
proof = Path(sys.argv[1]).resolve()
evidence = proof / "consumer-evidence"
cohort = json.loads((proof / "cohort.json").read_text())
owned = {p["name"]: p for p in cohort["packages"]}
metadata_files = sorted(evidence.glob("*.metadata.json"))
assert metadata_files, "no resolved consumer evidence"
third_party = set()
for metadata in metadata_files:
    lock = metadata.with_name(metadata.name.removesuffix(".metadata.json") + ".Cargo.lock")
    assert lock.is_file(), lock
    print(f"[consumer-security] {metadata.name}", flush=True)
    subprocess.run(["python3", root / "scripts/check-security-graph.py", metadata], check=True)
    for package in tomllib.loads(lock.read_text())["package"]:
        if package["name"] in owned:
            expected = owned[package["name"]]
            assert package["version"] == expected["version"] and package.get("checksum") == expected["sha256"], package
        elif "source" in package:
            assert package["source"] == "registry+https://github.com/rust-lang/crates.io-index", package
            third_party.add((package["name"], package["version"], package["checksum"]))
    subprocess.run(["cargo", "audit", "--file", lock, "--deny", "warnings", "--no-yanked", "--ignore", "RUSTSEC-2023-0071"], check=True)

# Staged Nidus archives do not exist in the public registry yet. Check their
# bytes above; query the official sparse index for ALL third-party versions.
# No network/index failure or missing version is interpreted as a pass.
def index_path(name):
    if len(name) < 3:
        return f"{len(name)}/{name}"
    if len(name) == 3:
        return f"3/{name[0]}/{name}"
    return f"{name[:2]}/{name[2:4]}/{name}"

def fetch(name):
    request = urllib.request.Request("https://index.crates.io/" + index_path(name), headers={"User-Agent": "nidus-release-proof"})
    for attempt in range(3):
        try:
            with urllib.request.urlopen(request, timeout=60) as response:
                return name, {entry["vers"]: entry for entry in map(json.loads, response.read().splitlines())}
        except (OSError, TimeoutError):
            if attempt == 2:
                raise
            time.sleep(attempt + 1)

with ThreadPoolExecutor(max_workers=8) as pool:
    indexes = dict(pool.map(fetch, sorted({p[0] for p in third_party})))
records = []
yanked = []
for name, version, checksum in sorted(third_party):
    entry = indexes[name][version]
    if entry["yanked"]:
        yanked.append((name, version))
    assert entry["cksum"] == checksum, (name, version, "checksum mismatch")
    records.append({"name": name, "version": version, "checksum": checksum, "yanked": entry["yanked"]})
(proof / "third-party-registry-check.json").write_text(json.dumps(records, indent=2) + "\n")
assert not yanked, f"yanked dependency versions: {yanked}"
print(f"audited {len(metadata_files)} resolved consumer graphs and locks; verified {len(records)} third-party versions and checksums against the official registry")
