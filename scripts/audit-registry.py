#!/usr/bin/env python3
"""Offline integrity check after `cargo metadata --locked --format-version 1`.

Usage: python3 scripts/audit-registry.py metadata.json > integrity.json
Compares cached archives with Cargo.lock, then extracted files with those archives.
Does not execute build scripts or establish that upstream packages are benign.
"""
import hashlib
import json
import os
from pathlib import Path, PurePosixPath
import sys
import tarfile
import tomllib

metadata = json.loads(Path(sys.argv[1]).read_text())
lock = tomllib.loads(Path("Cargo.lock").read_text())
checksums = {(p["name"], p["version"]): p["checksum"] for p in lock["package"] if "checksum" in p}
cache = Path(os.environ.get("CARGO_HOME", Path.home() / ".cargo")) / "registry/cache"
report = {"archives_checked": 0, "source_files_checked": 0, "errors": [], "extra_files": []}
for package in metadata["packages"]:
    if package["source"] is None:
        continue
    name = f'{package["name"]}-{package["version"]}'
    root = Path(package["manifest_path"]).parent
    archives = list(cache.glob(f"*/{name}.crate"))
    if len(archives) != 1:
        report["errors"].append(f"{name}: missing/ambiguous archive")
        continue
    archive = archives[0]
    digest = hashlib.sha256(archive.read_bytes()).hexdigest()
    if digest != checksums[(package["name"], package["version"])]:
        report["errors"].append(f"{name}: archive checksum mismatch")
        continue
    report["archives_checked"] += 1
    expected = set()
    with tarfile.open(archive, mode="r|gz") as contents:
        for member in contents:
            if not member.isfile():
                continue
            relative = PurePosixPath(member.name).relative_to(name)
            if ".." in relative.parts or relative.is_absolute():
                report["errors"].append(f"{name}: unsafe archive path")
                continue
            expected.add(relative.as_posix())
            actual = root / relative
            baseline = contents.extractfile(member).read()
            report["source_files_checked"] += 1
            if not actual.is_file() or actual.is_symlink() or actual.read_bytes() != baseline:
                report["errors"].append(f"{name}/{relative}: extracted source mismatch")
    for actual in root.rglob("*"):
        if actual.is_file():
            relative = actual.relative_to(root).as_posix()
            if relative not in expected and relative not in {".cargo-ok", ".cargo-checksum.json"}:
                report["extra_files"].append(f"{name}/{relative}")
print(json.dumps(report, indent=2))
sys.exit(1 if report["errors"] or report["extra_files"] else 0)
