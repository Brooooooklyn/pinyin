"""Experiment on a temporary copy of the locked Jieba source; never vendor it."""
from pathlib import Path
import hashlib
import json
import os
import shutil
import subprocess
import tempfile
import atexit

root = Path(__file__).resolve().parents[2]
registry = next((Path.home()/".cargo/registry/src").glob("*/jieba-rs-0.10.3"))
workspace = tempfile.TemporaryDirectory(prefix="pinyin-jieba-classifier-")
atexit.register(workspace.cleanup)
tmp = Path(workspace.name)
shutil.copytree(registry, tmp/"jieba")
shutil.copy2(root/"benchmark/remaining-simd/jieba-classifier.rs", tmp/"jieba/src/research_classifier.rs")
p = tmp/"jieba/src/lib.rs"
s = p.read_text()
s += "\npub mod research_classifier;\n"
s = s.replace("    classify: F,\n}", "    classify: F,\n    fast: bool,\n}", 1)
s = s.replace("{ text, pos: 0, classify }", "{ text, pos: 0, classify, fast: false }")
s = s.replace("let splitter = SplitByCharacterClass::new(sentence, is_han_default);",
              "let mut splitter = SplitByCharacterClass::new(sentence, is_han_default);\n        splitter.fast = true;")
start = s.index("            for c in remaining[first_char.len_utf8()..].chars() {")
end = s.index("            self.pos = end;", start)
s = s[:start] + """            let mut rest = &remaining[first_char.len_utf8()..];
            while !rest.is_empty() {
                if self.fast && research_classifier::enabled() {
                    let n = research_classifier::prefix(rest.as_bytes());
                    if n != 0 { end += n; rest = &rest[n..]; continue; }
                }
                let c = rest.chars().next().unwrap();
                if !(self.classify)(c) { break; }
                end += c.len_utf8();
                rest = &rest[c.len_utf8()..];
            }
""" + s[end:]
p.write_text(s)
(tmp/"src").mkdir()
shutil.copy2(root/"benchmark/remaining-simd/jieba-main.rs", tmp/"src/main.rs")
shutil.copy2(root/"benchmark/long.txt", tmp/"src/literature.txt")
(tmp/"Cargo.toml").write_text('''[package]
name = "pinyin-jieba-classifier-probe"
version = "0.0.0"
edition = "2021"
[workspace]
[dependencies]
jieba-rs = { path = "jieba" }
[profile.release]
lto = true
codegen-units = 1
''')
target = root/"target/jieba-classifier-probe"
env = dict(os.environ, CARGO_TARGET_DIR=str(target))
subprocess.run(["cargo", "build", "--release"], cwd=tmp, env=env, check=True)
folder = root/"benchmark/results/remaining-simd"
with (folder/"jieba-classifier.jsonl").open("w") as output:
    subprocess.run([str(target/"release/pinyin-jieba-classifier-probe")], cwd=tmp, stdout=output, check=True)
def info(p):
    return {"path": str(p), "sha256": hashlib.sha256(p.read_bytes()).hexdigest()}
(folder/"jieba-classifier-build.json").write_text(json.dumps({
    "source": str(tmp), "baseline_source": info(registry/"src/lib.rs"),
    "probe_source": info(p), "artifact": info(target/"release/pinyin-jieba-classifier-probe"),
    "lockfile": info(tmp/"Cargo.lock"), "builder": info(Path(__file__)),
}, indent=2)+"\n")
print((folder/"jieba-classifier.jsonl").read_text())
