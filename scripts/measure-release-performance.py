#!/usr/bin/env python3
"""Build isolated paired binaries, then measure on an otherwise idle machine."""
import hashlib
import io
import json
import os
from pathlib import Path
import shutil
import statistics
import subprocess
import sys
import tarfile
import tomllib

root = Path(__file__).resolve().parent.parent
output = Path(sys.argv[1]).resolve()
phase = sys.argv[2]
baseline = "99642ed0018c6da5c921db92d966f3b52a7e7d77"
output.mkdir(parents=True, exist_ok=True)
env = dict(os.environ, CARGO_NET_OFFLINE="true")
env.pop("CARGO_TARGET_DIR", None)

def run(arguments, **kwargs):
    return subprocess.run(list(map(str, arguments)), check=True, env=env, **kwargs)

def source_identity():
    run(["python3", root / "scripts/release-snapshot.py", output / "source-check"])
    return json.loads((output / "source-check/source.json").read_text())["source_sha256"]

def binaries():
    return {f"{label}-{instrument}": hashlib.sha256((output / f"{label}-{instrument}").read_bytes()).hexdigest()
            for label in ["baseline", "candidate"] for instrument in ["timing", "allocations"]}

if phase == "prepare":
    old = output / "baseline-source"
    old.mkdir(exist_ok=True)
    with tarfile.open(fileobj=io.BytesIO(subprocess.check_output(["git", "archive", baseline], cwd=root))) as archive:
        archive.extractall(old, filter="data")
    dependencies = {}
    for label, source in [("baseline", old), ("candidate", root)]:
        project = output / label
        shutil.copytree(root / "scripts/release-perf/src", project / "src", dirs_exist_ok=True)
        manifest = (root / "scripts/release-perf/Cargo.toml").read_text()
        manifest = manifest.replace('"../../crates/', '"' + str(source / "crates") + '/')
        (project / "Cargo.toml").write_text(manifest)
        # Both builds start from the candidate lock: isolate source changes from dependency drift.
        shutil.copyfile(root / "scripts/release-perf/Cargo.lock", project / "Cargo.lock")
        for instrument in ["timing", "allocations"]:
            features = (["managed"] if label == "candidate" else []) + (["allocations"] if instrument == "allocations" else [])
            command = ["cargo", "build", "--release", "--manifest-path", project / "Cargo.toml"]
            if features:
                command += ["--features", ",".join(features)]
            run(command)
            shutil.copyfile(project / "target/release/nidus-release-perf", output / f"{label}-{instrument}")
            (output / f"{label}-{instrument}").chmod(0o755)
        lock = tomllib.loads((project / "Cargo.lock").read_text())
        dependencies[label] = sorted((p["name"], p["version"], p.get("checksum")) for p in lock["package"] if p.get("source"))
    assert dependencies["baseline"] == dependencies["candidate"], "third-party dependency drift invalidates paired measurement"
    (output / "build.json").write_text(json.dumps({"baseline": baseline, "source_sha256": source_identity(), "binary_sha256": binaries(), "rustc": subprocess.check_output(["rustc", "-Vv"]).decode(), "third_party_dependencies": dependencies["candidate"]}, indent=2) + "\n")
elif phase == "measure":
    build = json.loads((output / "build.json").read_text())
    assert build["source_sha256"] == source_identity(), "candidate source changed: prepare again"
    assert build["binary_sha256"] == binaries(), "measurement binaries changed"
    results = []
    modes = ["raw", "metrics", "production_metrics"]
    for repetition in range(10):
        order = ["baseline", "candidate"] if repetition % 2 == 0 else ["candidate", "baseline"]
        for mode in modes:
            for label in order:
                result = json.loads(subprocess.check_output([output / f"{label}-timing", mode, "200000"], env=env))
                results.append(dict(result, variant=label, repetition=repetition, instrument="timing"))
    for label in ["baseline", "candidate"]:
        for mode in modes:
            result = json.loads(subprocess.check_output([output / f"{label}-allocations", mode, "10000"], env=env))
            results.append(dict(result, variant=label, instrument="allocations"))
    for instrument, count in [("timing", "10000"), ("allocations", "10000")]:
        result = json.loads(subprocess.check_output([output / f"candidate-{instrument}", "managed_start_stop", count], env=env))
        results.append(dict(result, variant="candidate", instrument=instrument))
    (output / "measurements.json").write_text(json.dumps(results, indent=2) + "\n")
    summary = []
    for mode in modes:
        medians = {label: statistics.median(r["ns_per_operation"] for r in results if r["mode"] == mode and r["variant"] == label and r["instrument"] == "timing") for label in ["baseline", "candidate"]}
        summary.append(dict(mode=mode, **medians, delta_percent=100*(medians["candidate"]/medians["baseline"]-1)))
    (output / "summary.json").write_text(json.dumps(summary, indent=2) + "\n")
    print(json.dumps(summary, indent=2))
else:
    raise ValueError("phase must be prepare or measure")
