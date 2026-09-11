"""Regenerate the SIMD report tables and complete CSV from persisted samples."""

import csv
import json
import statistics
from collections import defaultdict
from pathlib import Path

root = Path(__file__).resolve().parents[1]
directory = root / "benchmark/results/simdutf8"
validation = defaultdict(lambda: defaultdict(list))
for line in (directory / "validation.jsonl").read_text().splitlines():
    row = json.loads(line)
    validation[row["fixture"]][row["implementation"]].append(row["ns"])

validator_table = [
    "| Input | std, µs | SIMD compat, µs | Speedup | Basic + std error fallback, µs |",
    "| --- | ---: | ---: | ---: | ---: |",
]
for fixture, implementations in validation.items():
    assert all(len(samples) == 7 for samples in implementations.values()), fixture
    medians = {name: statistics.median(samples) / 1000 for name, samples in implementations.items()}
    before, after = medians["std"], medians["simd-compat"]
    validator_table.append(
        f"| {fixture} | {before:.3f} | {after:.3f} | {before / after:.2f}× | "
        f"{medians['simd-basic-std-fallback']:.3f} |"
    )

conversion = json.loads((directory / "conversion.json").read_text())
core = defaultdict(lambda: defaultdict(list))
for line in (directory / "core-conversion.jsonl").read_text().splitlines():
    row = json.loads(line)
    core[row["fixture"], row["resolver"]][row["implementation"]].append(row["ns"])
core_table = [
    "| Input | Resolver | std + conversion, µs | SIMD + conversion, µs | Change in time |",
    "| --- | --- | ---: | ---: | ---: |",
]
assert len(core) == 4
for (fixture, resolver), implementations in core.items():
    assert all(len(samples) == 7 for samples in implementations.values())
    before, after = [statistics.median(implementations[name]) / 1000 for name in ["std", "simd-compat"]]
    core_table.append(f"| {fixture} | {resolver} | {before:.2f} | {after:.2f} | {(after / before - 1) * 100:+.1f}% |")
assert len(conversion["rows"]) == 44
tables = {kind: [
    "| Input | Resolver | API | Before, µs | SIMD, µs | Change in time |",
    "| --- | --- | --- | ---: | ---: | ---: |",
] for kind in ["buffer", "string"]}
with (directory / "summary.csv").open("w", newline="") as handle:
    writer = csv.writer(handle)
    writer.writerow(["fixture", "input_type", "resolver", "api", "before_ns", "after_ns", "speedup", "time_change_percent"])
    for row in conversion["rows"]:
        before, after = [result["medianNs"] for result in row["result"]]
        assert all(len(result["samples"]) == conversion["rounds"] for result in row["result"])
        change = (after / before - 1) * 100
        writer.writerow([row["fixture"], row["inputType"], row.get("resolver", "—"), row["api"], before, after, before / after, change])
        tables[row["inputType"]].append(
            f"| {row['fixture']} | {row.get('resolver', '—')} | {row['api']} | "
            f"{before / 1000:.2f} | {after / 1000:.2f} | {change:+.1f}% |"
        )

report = root / "docs/performance-research.md"
content = report.read_text()
for name, lines in [("validation", validator_table), ("core", core_table), *tables.items()]:
    start, end = f"<!-- {name}:start -->", f"<!-- {name}:end -->"
    before, remaining = content.split(start, 1)
    _, after = remaining.split(end, 1)
    content = before + start + "\n\n" + "\n".join(lines) + "\n\n" + end + after
report.write_text(content)
