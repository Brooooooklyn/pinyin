"""Regenerate the report tables from the retained benchmark samples."""
import csv
import json
from pathlib import Path
import statistics

root = Path(__file__).resolve().parents[1]
results = root / 'benchmark/results'
lines = []
csv_rows = []

for filename, title in [('arrays', 'Array output'), ('strings', 'String output'), ('heteronyms', 'All-reading output')]:
    report = json.loads((results / f'{filename}.json').read_text())
    lines.extend([f'### {title}', '',
        f"Runtime: {report['node']}; pinyin-pro {report['pinyinPro']}. Median microseconds per call, seven rounds. Tone marks and `segment: true` are explicit. String-mode baseline includes joining its array.", '',
        '| Workload | Previous Rust | New Rust | pinyin-pro | Speedup vs previous | Speedup vs JS | New output equals JS |',
        '| --- | ---: | ---: | ---: | ---: | ---: | --- |'])
    for row in report['rows']:
        values = {r['implementation']: r for r in row['result']}
        before, after, js = (values[k]['medianNs'] / 1000 for k in ['baseline', 'rust', 'pinyin-pro'])
        equal = values['rust']['matchesPro']
        csv_rows.append([filename, row['fixture'], row['style'], row['segment'], before, after, js, before/after, js/after, equal])
        if row['style'] != 1 or not row['segment']:
            continue
        lines.append(f"| {row['fixture']} | {before:,.2f} | {after:,.2f} | {js:,.2f} | {before/after:.2f}× | {js/after:.2f}× | {'Yes' if equal else 'No'} |")
    lines.extend(['', f'[All styles, options, output hashes, and individual samples](../benchmark/results/{filename}.json).', ''])

lines.extend(['An exact-output “Yes” is a checked equality on these inputs. It is not a general pronunciation-equivalence claim. Natural-text “No” rows compare the same output format with different readings. Heteronym dictionaries also differ in reading coverage and order; very short calls can still favor JavaScript.', ''])
runtime = json.loads((results / 'runtime.json').read_text())
lines.extend(['### Cold initialization', '', 'Median milliseconds in nine fresh processes per implementation. The filesystem cache is not cleared. RSS includes the Node process, input, library, and first result.', '',
    '| Implementation | Load | First contextual call | Combined | RSS after first call, MiB |',
    '| --- | ---: | ---: | ---: | ---: |'])
for name in ['baseline', 'rust', 'pinyin-pro']:
    records = [r for r in runtime['cold'] if r['implementation'] == name]
    medians = [statistics.median(r[k] for r in records) for k in ['loadMs', 'firstConversionMs', 'totalMs', 'rssBytes']]
    lines.append(f'| {name} | {medians[0]:.3f} | {medians[1]:.3f} | {medians[2]:.3f} | {medians[3]/2**20:.1f} |')
lines.extend(['', '### Comparison and asynchronous calls', '', '| Operation | Previous, µs | New, µs | Speedup |', '| --- | ---: | ---: | ---: |'])
for operation in sorted({r['name'] for r in runtime['operations']}):
    before, after = [statistics.median(r['ns'] for r in runtime['operations'] if r['name'] == operation and r['implementation'] == name)/1000 for name in ['baseline', 'rust']]
    lines.append(f'| {operation} | {before:.2f} | {after:.2f} | {before/after:.2f}× |')
before, after = [statistics.median(r['elapsedMs'] for r in runtime['asynchronous'] if r['implementation'] == name)*1000 for name in ['baseline', 'rust']]
lines.append(f'| async literary corpus, tone marks, contextual mode | {before:.2f} | {after:.2f} | {before/after:.2f}× |')
lines.extend(['', '[Runtime samples](../benchmark/results/runtime.json). Native comparison still pays for copying JavaScript input strings even when the Rust comparator exits early.', ''])

if (results / 'scaling.json').exists():
    scaling = json.loads((results / 'scaling.json').read_text())
    lines.extend(['### Large natural-text scaling', '', 'Three fresh-process runs per size and implementation, each warmed on the original corpus. Tone-marked arrays and contextual mode are explicit. Readings differ across implementations; these are throughput and memory measurements, not equivalent-accuracy results.', '',
        '| Characters | Implementation | Median conversion, ms | Median peak RSS, MiB |', '| ---: | --- | ---: | ---: |'])
    for length in [1_000_000, 10_000_000]:
        for name in ['baseline', 'rust', 'pinyin-pro']:
            records = [r for r in scaling['rows'] if r['length'] == length and r['implementation'] == name]
            if records:
                assert len({r['outputHash'] for r in records}) == 1
                lines.append(f"| {length:,} | {name} | {statistics.median(r['elapsedMs'] for r in records):,.2f} | {statistics.median(r['maxRssKiB'] for r in records)/1024:,.1f} |")
    lines.extend(['', '[Scaling samples and output hashes](../benchmark/results/scaling.json). Peak RSS includes module loading, the source and expanded input, scratch allocations, and the retained output; it is sampled before the post-timing output hashing.', ''])

build = json.loads((results / 'build.json').read_text())
old_size, new_size = build['baseline_native']['bytes'], build['files']['pinyin.darwin-arm64.node']['bytes']
lines.extend(['### Build and core measurements', '',
    f'The fresh native release binary falls from **{old_size:,} bytes to {new_size:,} bytes**, a **{100*(1-new_size/old_size):.1f}% reduction**. The older pre-existing artifact was not used for this size comparison.', '',
    '[Rust-only benchmark samples](../benchmark/results/core.txt) separate lookup and reusable output from contextual processing and owned output. These measurements do not include Node-API or JavaScript allocation.', ''])

report_path = root / 'docs/performance-research.md'
report = report_path.read_text()
start = report.index('<!-- RESULTS -->') + len('<!-- RESULTS -->')
end = report.index('<!-- END RESULTS -->')
report_path.write_text(report[:start] + '\n' + '\n'.join(lines) + '\n' + report[end:])
with (results / 'summary.csv').open('w') as file:
    writer = csv.writer(file)
    writer.writerow(['mode','workload','style','segment','previous_us','new_us','pinyin_pro_us','speedup_previous','speedup_js','same_as_js'])
    writer.writerows(csv_rows)
print('Updated report tables and benchmark/results/summary.csv')
