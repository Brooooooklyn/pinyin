"""Build a recorded native baseline; temporary sources are removed on exit."""

import argparse
import hashlib
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile

from baselines import ROOT, STAGES, restore

parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument("stage", choices=STAGES)
args = parser.parse_args()
target = ROOT / "target" / f"{args.stage}-baseline"
with tempfile.TemporaryDirectory(prefix=f"pinyin-{args.stage}-baseline-") as directory:
    source = Path(directory)
    record = restore(args.stage, source)
    subprocess.run(
        ["cargo", "build", "--locked", "--release"], cwd=source,
        env=dict(os.environ, CARGO_TARGET_DIR=str(target)), check=True,
    )
    library = {"darwin": "libnapi_pinyin.dylib", "win32": "napi_pinyin.dll"}.get(sys.platform, "libnapi_pinyin.so")
    artifact = target / "before.node"
    shutil.copy2(target / "release" / library, artifact)
    metadata = {
        "stage": args.stage, "artifact": str(artifact),
        "sha256": hashlib.sha256(artifact.read_bytes()).hexdigest(),
        "patchSha256": record["patchSha256"], "sourceHashesVerified": True,
        "rustc": subprocess.check_output(["rustc", "-Vv"], text=True).strip(),
    }
    (target / "build.json").write_text(json.dumps(metadata, indent=2) + "\n")
    print(json.dumps(metadata, indent=2))
