"""Compatibility entry point for the recorded pre-encoding baseline."""
from pathlib import Path
import subprocess
import sys

subprocess.run(
    [sys.executable, str(Path(__file__).with_name("build-baseline.py")), "encoding"],
    check=True,
)
