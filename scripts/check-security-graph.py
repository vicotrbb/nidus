#!/usr/bin/env python3
"""Keep the SQLx MySQL RSA exception narrow over the entire resolved graph."""
import json
from pathlib import Path
import sys

metadata = json.loads(Path(sys.argv[1]).read_text())
packages = {p["id"]: p for p in metadata["packages"]}
rsa = {key for key, value in packages.items() if value["name"] == "rsa"}
for package_id in rsa:
    parents = {packages[node["id"]]["name"] for node in metadata["resolve"]["nodes"]
               if package_id in node["dependencies"]}
    assert parents == {"sqlx-mysql"}, f"RSA exception escaped SQLx MySQL: {parents}"
sentry = {(p["name"], p["version"]) for p in packages.values()
          if p["name"] == "sentry" or p["name"].startswith("sentry-")}
assert len({version for _, version in sentry}) <= 1, f"split Sentry cohort: {sentry}"
print(f"security graph verified: {len(packages)} packages; RSA parents limited to SQLx MySQL ({len(rsa)} RSA versions); Sentry cohort coherent")
