from pathlib import Path
import hashlib
import json
import os
import shutil
import subprocess
import tempfile
import atexit

root = Path(__file__).resolve().parents[2]
workspace = tempfile.TemporaryDirectory(prefix="pinyin-jieba-parity-")
atexit.register(workspace.cleanup)
tmp = Path(workspace.name)
(tmp/"src").mkdir()
shutil.copy2(root/"benchmark/remaining-simd/verify-jieba.rs", tmp/"src/main.rs")
shutil.copy2(root/"benchmark/long.txt", tmp/"src/literature.txt")
(tmp/"Cargo.toml").write_text('''[package]
name = "pinyin-jieba-parity"
version = "0.0.0"
edition = "2021"
[workspace]
[dependencies]
original = { package = "jieba-rs", version = "=0.10.3" }
patched = { package = "jieba-rs", path = "''' + str(root/"vendor/jieba-rs") + '''", features = ["pinyin-simd"] }
''')
result = subprocess.run(["cargo", "run", "--release"], cwd=tmp, capture_output=True, text=True,
  env=dict(os.environ, CARGO_TARGET_DIR=str(root/"target/jieba-parity")), check=True)
report = json.loads(result.stdout)
report["source"] = str(tmp)
report["files"] = [{"path":str(p), "sha256":hashlib.sha256(p.read_bytes()).hexdigest()} for p in
  [root/"vendor/jieba-rs/src/lib.rs", root/"vendor/jieba-rs/src/simd_classifier.rs", root/"vendor/jieba-rs/Cargo.toml", tmp/"Cargo.lock", Path(__file__)]]
(root/"benchmark/results/remaining-simd/jieba-parity.json").write_text(json.dumps(report,indent=2)+"\n")
print(json.dumps(report))
