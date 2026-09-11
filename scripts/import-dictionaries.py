"""Import pinned, MIT-licensed source data; never needed during a Cargo build.

Usage: python3 scripts/import-dictionaries.py RUST_PINYIN_0_11_DIR PINYIN_PRO_CHECKOUT
"""

import hashlib
import json
from pathlib import Path
import re
import subprocess
import sys
import urllib.request

legacy, pro = map(Path, sys.argv[1:])
destination = Path(__file__).resolve().parents[1] / "crates/pinyin-core/data"
destination.mkdir(parents=True, exist_ok=True)
revision = subprocess.check_output(["git", "-C", str(pro), "rev-parse", "HEAD"], text=True).strip()
assert revision == "14b898d6aff00c9df75be351667bcc4155da395b", revision
assert json.loads((legacy / ".cargo_vcs_info.json").read_text())["git"]["sha1"] == "76fc7d30de40c85f4cc7e8e138da677a172a78b4"

sources = []
def record(path, upstream):
    raw = path.read_bytes()
    sources.append({"file": str(path.relative_to(legacy if path.is_relative_to(legacy) else pro)),
                    "url": upstream, "sha256": hashlib.sha256(raw).hexdigest()})
    return raw

data_revision = "fa9761fff402f8560196b1ba085c437c52b56d7c"
base = f"https://github.com/mozillazg/pinyin-data/blob/{data_revision}/"
raw_base = f"https://raw.githubusercontent.com/mozillazg/pinyin-data/{data_revision}/"
characters = record(legacy / "pinyin-data/pinyin.txt", base + "pinyin.txt")
assert characters == urllib.request.urlopen(raw_base + "pinyin.txt").read()
(destination / "characters.txt").write_bytes(characters)
(destination / "LICENSE.pinyin-data").write_bytes(urllib.request.urlopen(raw_base + "LICENSE").read())
(destination / "LICENSE.rust-pinyin").write_bytes((legacy / "LICENSE").read_bytes())
(destination / "LICENSE.pinyin-pro").write_bytes((pro / "LICENSE").read_bytes())

phrases = {}
for length in range(2, 6):
    path = pro / f"packages/pinyin-pro/lib/data/dict{length}.ts"
    source = record(path, f"https://github.com/zh-lx/pinyin-pro/blob/{revision}/packages/pinyin-pro/lib/data/dict{length}.ts").decode()
    # Only parse the data object, never execute the upstream TypeScript.
    body = source.split(" = {", 1)[1].split("\n}", 1)[0]
    for line in body.splitlines():
        line = line.strip()
        if not line or line.startswith("//"):
            continue
        match = re.fullmatch(r'''([^:]+):\s*(['"])(.*?)\2,?\s*(?://.*)?''', line)
        assert match, line
        word, _, readings = match.groups()
        word = word.strip("'\"")
        assert "\\" not in word + readings
        assert len(word) == len(readings.split()) == length, (word, readings)
        phrases[word] = readings

(destination / "phrases.tsv").write_text("# phrase\tspace-separated tone-marked syllables\n" +
    "".join(f"{word}\t{phrases[word]}\n" for word in sorted(phrases)))
(destination / "sources.json").write_text(json.dumps({"pinyin_pro_revision": revision, "pinyin_data_revision": data_revision, "sources": sources}, indent=2) + "\n")
print(f"Imported {len(phrases)} phrases and pinned character data")
