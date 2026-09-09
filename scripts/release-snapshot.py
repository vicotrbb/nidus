#!/usr/bin/env python3
"""Capture the tracked and nonignored candidate, including uncommitted files."""
import hashlib
import json
from pathlib import Path
import subprocess
import sys
import tarfile

root = Path(__file__).resolve().parent.parent
output = Path(sys.argv[1]).resolve()
output.mkdir(parents=True, exist_ok=True)
excluded = {"docs/release-readiness-checklist.md", "docs/validation/release-1.1.0.md"}
files = sorted(set(subprocess.check_output(
    ["git", "ls-files", "-co", "--exclude-standard", "-z"], cwd=root
).decode().strip("\0").split("\0")))
records = []
reporting_records = []
digest = hashlib.sha256()
with tarfile.open(output / "source.tar", "w") as archive:
    for name in files:
        path = root / name
        if not path.is_file():
            continue
        data = path.read_bytes()
        record = {"path": name, "sha256": hashlib.sha256(data).hexdigest()}
        if name in excluded:
            reporting_records.append(record)
        else:
            digest.update(name.encode() + b"\0" + data + b"\0")
            records.append(record)
        archive.add(path, arcname=name, recursive=False)
    # A private copy allows read-only Git status/package provenance checks in Linux.
    archive.add(root / ".git", arcname=".git")
metadata = {"baseline": subprocess.check_output(["git", "rev-parse", "HEAD"], cwd=root).decode().strip(),
            "source_sha256": digest.hexdigest(), "excluded": sorted(excluded), "files": records, "reporting_files": reporting_records}
(output / "source.json").write_text(json.dumps(metadata, indent=2) + "\n")
print(json.dumps({"source_sha256": digest.hexdigest(), "files": len(records)}))
