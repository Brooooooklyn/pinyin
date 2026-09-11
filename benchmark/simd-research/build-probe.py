"""Build an isolated addon with same-binary scalar/SIMD output selection."""
import hashlib
import json
import os
from pathlib import Path
import shutil
import subprocess
import tempfile
import sys
import atexit

root = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(root / "benchmark"))
from baselines import restore

workspace = tempfile.TemporaryDirectory(prefix="pinyin-simd-output-probe-")
atexit.register(workspace.cleanup)
probe = Path(workspace.name)
restore("encoding", probe)
baseline_source_hash = hashlib.sha256((probe / "src/lib.rs").read_bytes()).hexdigest()
manifest = (probe / "Cargo.toml").read_text()
manifest = manifest.replace('simdutf8 = "0.1.5"', 'simdutf8 = "0.1.5"\nsimdutf = "=0.7.0"')
(probe / "Cargo.toml").write_text(manifest)
source = (probe / "src/lib.rs").read_text()
assert source.count("Either::B(value.into())") == 1
source = source.replace("Either::B(value.into())", "Either::B(research_utf16(value))")
source += r'''

// Research-only exports: never included in the production binding.
static RESEARCH_SIMD: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

#[napi]
pub fn research_set_simd(enabled: bool) {
  RESEARCH_SIMD.store(enabled, std::sync::atomic::Ordering::Relaxed);
}

fn research_utf16(value: String) -> Utf16String {
  if !RESEARCH_SIMD.load(std::sync::atomic::Ordering::Relaxed) { return value.into(); }
  let mut out: Vec<u16> = Vec::with_capacity(value.len());
  // SAFETY: valid Rust string, disjoint aligned allocation, at most one u16
  // per input byte. Only code units written by the transcoder are published.
  unsafe {
    let len = simdutf::convert_valid_utf8_to_utf16(value.as_ptr(), value.len(), out.as_mut_ptr());
    assert!(len <= out.capacity());
    out.set_len(len);
  }
  out.into()
}

#[napi]
pub fn research_input_utf8_length(input: String) -> u32 { input.len() as u32 }

#[napi]
pub fn research_input_utf16_length(input: Utf16String) -> u32 { input.len() as u32 }
'''
(probe / "src/lib.rs").write_text(source)
target = root / "target/simd-research"
env = dict(os.environ, CARGO_TARGET_DIR=str(target))
subprocess.run(["cargo", "build", "--release", "--manifest-path", str(probe / "Cargo.toml")], env=env, check=True)
library = target / "release/libnapi_pinyin.dylib"
destination = target / "probe.node"
shutil.copy2(library, destination)
def digest(path):
    data = path.read_bytes()
    return {"bytes": len(data), "sha256": hashlib.sha256(data).hexdigest()}
report = root / "benchmark/results/simd-research"
report.mkdir(parents=True, exist_ok=True)
(report / "probe-build.json").write_text(json.dumps({
    "source_directory": str(probe), "artifact": str(destination),
    "binary": digest(destination),
    "baseline_source_sha256": baseline_source_hash,
    "probe_source": digest(probe / "src/lib.rs"),
    "probe_cargo_lock": digest(probe / "Cargo.lock"),
    "builder": digest(Path(__file__)),
    "rustc": subprocess.check_output(["rustc", "-Vv"], text=True).strip(),
}, indent=2) + "\n")
print(destination)
