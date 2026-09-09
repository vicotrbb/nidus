#!/usr/bin/env python3
"""Reject stale or workspace-sourced Nidus dependencies in release consumers."""
import json
import os
import shutil
from pathlib import Path
import subprocess
import sys

manifest, version, mode = sys.argv[1:]
root = Path(__file__).resolve().parent.parent
cohort = {p.name for p in (root / "crates").iterdir() if (p / "Cargo.toml").exists()}
cohort.discard("nidus")
cohort.add("nidus-rs")
metadata = json.loads(subprocess.check_output([
    "cargo", "metadata", "--locked", "--format-version", "1", "--manifest-path", manifest
]))
resolved = [p for p in metadata["packages"] if p["name"] in cohort]
assert resolved, "consumer resolved no Nidus packages"
for package in resolved:
    assert package["version"] == version, (package["name"], package["version"], version)
    if mode == "registry":
        assert package["source"] == "registry+https://github.com/rust-lang/crates.io-index", package["name"]
        assert not Path(package["manifest_path"]).is_relative_to(root / "crates"), package["manifest_path"]
    elif mode != "workspace":
        raise ValueError(mode)
print(f"verified {len(resolved)} exact-version {version} Nidus dependencies ({mode})")

evidence = os.environ.get("NIDUS_CONSUMER_EVIDENCE_DIR")
if evidence:
    directory = Path(evidence)
    directory.mkdir(parents=True, exist_ok=True)
    name = Path(manifest).parent.name
    (directory / f"{name}.metadata.json").write_text(json.dumps(metadata, indent=2) + "\n")
    shutil.copyfile(Path(manifest).with_name("Cargo.lock"), directory / f"{name}.Cargo.lock")
