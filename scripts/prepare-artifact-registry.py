#!/usr/bin/env python3
"""Build a closed local registry from verified cohort archives and locked dependencies."""
import hashlib
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys
import tarfile
import tomllib

root = Path(__file__).resolve().parent.parent
output = Path(sys.argv[1]).resolve()
output.mkdir(parents=True, exist_ok=True)
metadata = json.loads(subprocess.check_output(
    ["cargo", "metadata", "--locked", "--no-deps", "--format-version", "1"], cwd=root
))
packages = [p for p in metadata["packages"] if p["publish"] != []]
assert len(packages) == 25, "review the expected publishable cohort before changing its size"
version = next(p["version"] for p in packages if p["name"] == "nidus-rs")
assert all(p["version"] == version for p in packages)
cohort = {p["name"] for p in packages}
registry = output / "registry"
registry.mkdir()  # Refuse stale entries from a previous candidate.
source_registry = root / "target/package/tmp-registry"
cargo_home = Path(os.environ.get("CARGO_HOME", Path.home() / ".cargo"))
records = []

def index_path(name):
    if len(name) < 3:
        return f"{len(name)}/{name}"
    if len(name) == 3:
        return f"3/{name[0]}/{name}"
    return f"{name[:2]}/{name[2:4]}/{name}"

def checksum(path):
    with path.open("rb") as source:
        return hashlib.file_digest(source, "sha256").hexdigest()

for package in packages:
    name = package["name"]
    archive = root / "target/package" / f"{name}-{version}.crate"
    with tarfile.open(archive) as tar:
        manifest = tomllib.loads(tar.extractfile(f"{name}-{version}/Cargo.toml").read().decode())
    assert manifest["package"]["version"] == version
    sections = [manifest] + list(manifest.get("target", {}).values())
    for section in sections:
        for kind in ["dependencies", "build-dependencies", "dev-dependencies"]:
            for alias, dependency in section.get(kind, {}).items():
                if not isinstance(dependency, dict):
                    continue
                dependency_name = dependency.get("package", alias)
                if dependency_name in cohort:
                    assert dependency.get("version", "").lstrip("^=") == version, (name, alias, dependency)
                    assert "path" not in dependency, (name, alias, "workspace path survived")
    index = source_registry / "index" / index_path(name)
    entries = [json.loads(line) for line in index.read_text().splitlines()]
    entry = next(e for e in entries if e["vers"] == version)
    assert entry["cksum"] == checksum(archive)
    destination = registry / "index" / index_path(name)
    destination.parent.mkdir(parents=True, exist_ok=True)
    destination.write_text(json.dumps(entry) + "\n")
    shutil.copyfile(archive, registry / archive.name)
    records.append({"name": name, "version": version, "sha256": entry["cksum"], "features": list(package["features"])})

lock = tomllib.loads((root / "Cargo.lock").read_text())
for package in lock["package"]:
    if "source" not in package:
        continue
    assert package["source"].startswith("registry+"), "non-registry dependencies require explicit handling"
    name, dependency_version = package["name"], package["version"]
    archive_name = f"{name}-{dependency_version}.crate"
    archives = list((cargo_home / "registry/cache").glob(f"*/{archive_name}"))
    assert archives, f"run cargo fetch --locked first: missing {archive_name}"
    archive = next(a for a in archives if checksum(a) == package["checksum"])
    entries = []
    for cache in (cargo_home / "registry/index").glob(f"*/.cache/{index_path(name)}"):
        for chunk in cache.read_bytes().split(b"\0"):
            if chunk.startswith(b'{"name":'):
                entry = json.loads(chunk)
                if entry["vers"] == dependency_version and entry["cksum"] == package["checksum"]:
                    entries.append(entry)
    assert entries, (name, dependency_version, "index record missing")
    destination = registry / "index" / index_path(name)
    destination.parent.mkdir(parents=True, exist_ok=True)
    existing = destination.read_text().splitlines() if destination.exists() else []
    if not any(json.loads(e)["vers"] == dependency_version for e in existing):
        existing.append(json.dumps(entries[0]))
    destination.write_text("\n".join(existing) + "\n")
    shutil.copyfile(archive, registry / archive_name)

(output / "cohort.json").write_text(json.dumps({"version": version, "packages": records}, indent=2) + "\n")
consumer_home = output / "cargo-home"
consumer_home.mkdir(exist_ok=True)
(consumer_home / "config.toml").write_text(
    '[source.crates-io]\nreplace-with = "release-artifacts"\n\n'
    f'[source.release-artifacts]\nlocal-registry = {json.dumps(str(registry))}\n\n[net]\noffline = true\n'
)
print(json.dumps({"version": version, "archives": len(records), "registry": str(registry), "cargo_home": str(consumer_home)}))
