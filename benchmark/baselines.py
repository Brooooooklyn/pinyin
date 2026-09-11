"""Restore recorded intermediate sources without changing the working checkout."""

import hashlib
import json
from pathlib import Path
import shutil
import subprocess

ROOT = Path(__file__).resolve().parents[1]
STAGES = ("encoding", "remaining-simd")


def restore(stage: str, destination: Path) -> dict:
    if stage not in STAGES:
        raise ValueError(f"Unknown baseline: {stage}")
    if any(destination.iterdir()):
        raise ValueError("Baseline destination must be empty")
    folder = ROOT / "benchmark/results" / stage
    patch = folder / "source-before.patch"
    record = json.loads((folder / "source-before.json").read_text())
    if hashlib.sha256(patch.read_bytes()).hexdigest() != record["patchSha256"]:
        raise ValueError("Baseline patch checksum mismatch")
    for name in ("Cargo.toml", "Cargo.lock", "build.rs"):
        shutil.copy2(ROOT / name, destination / name)
    for name in ("src", "crates", "benches", "vendor"):
        shutil.copytree(ROOT / name, destination / name)
    # Fail on source drift instead of silently benchmarking a different baseline.
    subprocess.run(["git", "apply", "--check", str(patch)], cwd=destination, check=True)
    subprocess.run(["git", "apply", str(patch)], cwd=destination, check=True)
    for name, expected in record["files"].items():
        if hashlib.sha256((destination / name).read_bytes()).hexdigest() != expected:
            raise ValueError(f"Restored baseline source checksum mismatch: {name}")
    return record
