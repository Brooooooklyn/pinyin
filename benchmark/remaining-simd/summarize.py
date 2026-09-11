"""Update report tables from persisted complete-call measurements."""
from pathlib import Path
import json

root = Path(__file__).resolve().parents[2]
folder = root / "benchmark/results/remaining-simd"
native = json.loads((folder / "conversion.json").read_text())
wasm = json.loads((folder / "wasm.json").read_text())
buffer = json.loads((folder / "buffer.json").read_text())
assert len(native["rows"]) == 112
assert len(wasm["rows"]) == 24
assert len(buffer["rows"]) == 6


def table(headers, rows):
    return "\n".join([
        "| " + " | ".join(headers) + " |",
        "| " + " | ".join(["---"] * len(headers)) + " |",
        *("| " + " | ".join(row) + " |" for row in rows),
    ])


def times(row):
    return {r["implementation"]: r["medianNs"] for r in row["result"]}


def change(before, after):
    return f"{(after / before - 1) * 100:+.1f}%"


def selected(row):
    fixture, resolver = row["fixture"], row.get("resolver")
    style, api = row.get("style"), row["api"]
    if fixture == "literature-100k" and style == 1 and api in ["pinyin", "pinyinString"]:
        return True
    if fixture == "mixed-100k" and ((style == 0 and api == "pinyin") or (style == 1 and api == "pinyinString")):
        return True
    if resolver != "character":
        return False
    return ((fixture == "long-ascii" and api == "pinyinString")
            or (fixture == "long-unicode" and style == 1 and api in ["pinyin", "pinyinString"])
            or (fixture == "long-escaped" and ((style == 0 and api == "pinyin") or (style == 1 and api == "pinyinString"))))


native_rows = []
for row in native["rows"]:
    if not selected(row):
        continue
    t = times(row)
    label = " / ".join([row["fixture"], row["resolver"], "tone" if row["style"] else "plain", row["api"]])
    native_rows.append([label, f'{t["before"] / 1000:.2f}', f'{t["after"] / 1000:.2f}', change(t["before"], t["after"])])

parts = [
    "Medians of complete synchronous calls with JavaScript string input. Times are microseconds; a negative time change means less time. All before/after outputs match. The full 112-row run also retains short inputs, ASCII-only input, heteronyms, sequential async calls, and all matched-corpus comparisons.",
    table(["Fixture / resolver / output", "Before, µs", "Current, µs", "Time change"], native_rows),
]

comparisons = []
for row in native["rows"]:
    if row["api"] == "compare":
        t = times(row)
        comparisons.append([row["fixture"], f'{t["before"] / 1000:.3f}', f'{t["after"] / 1000:.3f}', change(t["before"], t["after"])])
parts += ["Comparator calls use precomputed legacy-compatible keys:", table(["Comparison", "Before, µs", "Current, µs", "Time change"], comparisons)]

pro_rows = []
for row in native["rows"]:
    if row.get("resolver") != "jieba" or row["fixture"] not in ["literature-100k", "matched-100k"] or "pinyin-pro" not in times(row):
        continue
    t = times(row)
    label = " / ".join([row["fixture"], "tone" if row["style"] else "plain", row["api"]])
    pro_rows.append([label, f'{t["after"] / 1e6:.3f}', f'{t["pinyin-pro"] / 1e6:.3f}', f'{t["pinyin-pro"] / t["after"]:.2f}×', "yes" if row["matchesPro"] else "no"])
parts += [
    f"The final Jieba-enabled binding was also compared with pinyin-pro **{native['pinyinPro']}**. The matched corpus is a deterministic 100,000-character synthetic corpus with identical outputs. Natural prose has dictionary/contextual reading differences and is not an equivalent linguistic workload. pinyin-pro uses `toneSandhi: false` and `nonZh: 'consecutive'`; the binding uses Jieba with HMM disabled. Ratios on different outputs describe runtime only, not accuracy.",
    table(["Fixture / output", "Current, ms", "pinyin-pro, ms", "Runtime ratio", "Outputs match"], pro_rows),
]

buffer_rows = []
for row in buffer["rows"]:
    t = times(row)
    buffer_rows.append([" / ".join([row["fixture"], row["resolver"], row["api"]]), f'{t["before"] / 1000:.2f}', f'{t["after"] / 1000:.2f}', change(t["before"], t["after"])])
parts += ["Additional tone-output Buffer-input controls:", table(["Fixture / resolver / API", "Before, µs", "Current, µs", "Time change"], buffer_rows)]

wasm_rows = []
for row in wasm["rows"]:
    if row["fixture"] == "short":
        continue
    t = times(row)
    wasm_rows.append([" / ".join([row["fixture"], row["resolver"], row["api"]]),
                      f'{t["before"] / 1e6:.3f}', f'{t["portable"] / 1e6:.3f}', f'{t["simd"] / 1e6:.3f}', change(t["portable"], t["simd"])])
wasm_parts = [
    "Tone-output calls through the Node WASI loader, in milliseconds. The last column isolates the current SIMD build against the current standard build; it does not compare against the older baseline. All three variants produce identical outputs in these cases.",
    table(["Fixture / resolver / API", "Before, ms", "Current standard, ms", "Current SIMD, ms", "SIMD time change"], wasm_rows),
]

report = root / "docs/performance-research.md"
text = report.read_text()
for name, content in [("native", parts), ("wasm", wasm_parts)]:
    start, end = f"<!-- {name}-results:start -->", f"<!-- {name}-results:end -->"
    head, tail = text.split(start)
    _, tail = tail.split(end)
    text = head + start + "\n\n" + "\n\n".join(content) + "\n\n" + end + tail
report.write_text(text)
print(report)
