"""Compare json-escape-simd integrations in one isolated addon."""
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

workspace = tempfile.TemporaryDirectory(prefix="pinyin-json-escape-probe-")
atexit.register(workspace.cleanup)
probe = Path(workspace.name)
restore("remaining-simd", probe)
baseline_source_hash = hashlib.sha256((probe / "src/lib.rs").read_bytes()).hexdigest()
manifest = (probe / "Cargo.toml").read_text()
manifest = manifest.replace('simdutf8 = "0.1.5"', 'simdutf8 = "0.1.5"\njson-escape-simd = "=3.1.1"')
(probe / "Cargo.toml").write_text(manifest)
source = (probe / "src/lib.rs").read_text()
begin = source.index('      for ch in token.text(input, style).chars() {', source.index('fn json_output('))
end = source.index('\n    }\n    joined.push', begin)
original_loop = source[begin:end]
source = source[:begin] + '''      let value = token.text(input, style);
      if mode == 1 || mode == 2 || (mode == 4 && value.len() >= 64) {
        joined.pop(); // The external API supplies both quotes.
        // SAFETY: escape_into appends valid UTF-8 to an existing valid String.
        unsafe { json_escape_simd::escape_into(value, joined.as_mut_vec()); }
        joined.pop(); // The surrounding writer appends the closing quote.
      } else if mode == 3 {
        research_scan_utf8(value, &mut joined);
      } else {
''' + original_loop + '''
      }''' + source[end:]
source = source.replace('  let mut joined = String::with_capacity', '  let mode = JSON_ESCAPE_MODE.load(std::sync::atomic::Ordering::Relaxed);\n  let mut joined = String::with_capacity', 1)
source += r'''

// Research-only exports; never included in the production binding.
static JSON_ESCAPE_MODE: std::sync::atomic::AtomicU8 = std::sync::atomic::AtomicU8::new(0);

#[napi]
pub fn research_set_escape_mode(mode: u8) {
  assert!(mode <= 4);
  JSON_ESCAPE_MODE.store(mode, std::sync::atomic::Ordering::Relaxed);
}

#[napi]
pub fn research_unmapped_runs(input: String) -> Vec<u32> {
  let mut result = vec![0; 6];
  for token in pinyin_core::tokens(&input, false) {
    result[0] += 1;
    if token.syllable().is_none() {
      let bytes = &input.as_bytes()[token.range()];
      result[1] += 1;
      result[2] += bytes.len() as u32;
      if bytes.len() >= 64 { result[3] += 1; result[4] += bytes.len() as u32; }
      result[5] += bytes.iter().filter(|&&b| b < 32 || b == b'"' || b == b'\\').count() as u32;
    }
  }
  result
}

fn research_scan_utf8(input: &str, output: &mut String) {
  let mut start = 0;
  while let Some(relative) = encoding::json_escape(&input.as_bytes()[start..]) {
    let i = start + relative;
    let byte = input.as_bytes()[i];
    output.push_str(&input[start..i]);
    match byte {
      b'"' => output.push_str("\\\""),
      b'\\' => output.push_str("\\\\"),
      b'\n' => output.push_str("\\n"),
      b'\r' => output.push_str("\\r"),
      b'\t' => output.push_str("\\t"),
      _ => {
        const HEX: &[u8] = b"0123456789abcdef";
        output.push_str("\\u00");
        output.push(HEX[(byte >> 4) as usize] as char);
        output.push(HEX[(byte & 15) as usize] as char);
      }
    }
    start = i + 1;
  }
  output.push_str(&input[start..]);
}
'''
(probe / "src/lib.rs").write_text(source)
output = (probe / "src/output.rs").read_text()
output = output.replace('  let mut output = Vec::with_capacity', '  let bridge = crate::JSON_ESCAPE_MODE.load(std::sync::atomic::Ordering::Relaxed) == 2;\n  let mut scratch = Vec::new();\n  let mut output = Vec::with_capacity', 1)
output = output.replace('      json_text(token.text(input, style), &mut output);', '''      if bridge {
        scratch.clear();
        json_escape_simd::escape_into(token.text(input, style), &mut scratch);
        // SAFETY: the escaper produces valid UTF-8, with ASCII quotes at both ends.
        let text = unsafe { std::str::from_utf8_unchecked(&scratch[1..scratch.len() - 1]) };
        append_utf16(text, &mut output);
      } else {
        json_text(token.text(input, style), &mut output);
      }''')
(probe / "src/output.rs").write_text(output)
target = root / "target/json-escape-research"
env = dict(os.environ, CARGO_TARGET_DIR=str(target))
subprocess.run(["cargo", "fmt", "--all"], cwd=probe, check=True)
subprocess.run(["cargo", "build", "--release"], cwd=probe, env=env, check=True)
destination = target / "probe.node"
shutil.copy2(target / "release/libnapi_pinyin.dylib", destination)
def info(path):
    data = path.read_bytes()
    return {"path": str(path), "bytes": len(data), "sha256": hashlib.sha256(data).hexdigest()}
folder = root / "benchmark/results/json-escape"
folder.mkdir(parents=True, exist_ok=True)
(folder / "build.json").write_text(json.dumps({
    "source": str(probe), "artifact": info(destination),
    "baselineStage": "remaining-simd", "baselineSourceSha256": baseline_source_hash,
    "production": [info(root / p) for p in ["src/lib.rs", "src/encoding.rs", "src/output.rs", "Cargo.toml", "Cargo.lock", "pinyin.darwin-arm64.node"]],
    "probe": [info(probe / p) for p in ["src/lib.rs", "src/output.rs", "Cargo.toml", "Cargo.lock"]],
    "builder": info(Path(__file__)), "dependency": "json-escape-simd 3.1.1",
}, indent=2) + "\n")
print(destination)
