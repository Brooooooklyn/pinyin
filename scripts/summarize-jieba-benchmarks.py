"""Generate Jieba comparison tables from completed persisted measurements."""
import csv
import json
from pathlib import Path
import statistics

root = Path(__file__).resolve().parents[1]
results = root / 'benchmark/results/jieba'
lines = []
summary = []
names = ['baseline', 'rust', 'rust-jieba', 'pinyin-pro']
labels = dict(zip(names, ['Original binding', 'Phrase resolver', 'Jieba resolver', 'pinyin-pro']))
for mode in ['arrays', 'strings']:
    report = json.loads((results / f'{mode}.json').read_text())
    assert len(report['rows']) == 18
    lines += [f'### {mode.capitalize()}', '',
        'Median microseconds per call, seven rotated rounds. Tone marks and contextual conversion are enabled.', '',
        '| Workload | Original | Phrase | Jieba | pinyin-pro | Jieba speedup vs JS | Jieba output equals JS |',
        '| --- | ---: | ---: | ---: | ---: | ---: | --- |']
    for row in report['rows']:
        values = {item['implementation']: item for item in row['result']}
        assert all(len(values[name]['samples']) == 7 for name in names)
        times = [values[name]['medianNs'] / 1000 for name in names]
        same = values['rust-jieba']['matchesPro']
        summary.append([mode, row['fixture'], row['style'], *times, times[3]/times[2], same])
        if row['style'] == 1:
            lines.append(f"| {row['fixture']} | {' | '.join(f'{time:,.2f}' for time in times)} | {times[3]/times[2]:.2f}× | {'Yes' if same else 'No'} |")
        if row['style'] == 0 and row['fixture'] == 'matched-100k':
            plain = f'On the exact-output 100k synthetic input with plain pinyin, Jieba takes {times[2]/1000:.2f} ms versus {times[3]/1000:.2f} ms for pinyin-pro ({times[3]/times[2]:.2f}× faster).'
    lines += ['', plain, '', f'[All options, hashes, and samples](../benchmark/results/jieba/{mode}.json).', '']

runtime = json.loads((results / 'runtime.json').read_text())
lines += ['An equality flag refers to the new Jieba result. The synthetic matched workloads assert equality across all four implementations. Natural-text “No” rows compare matching formats with different pronunciations, not equivalent accuracy.', '',
    f"The final phrase and Jieba resolvers differ at **{len(runtime['disagreements'])} of {runtime['corpusTokens']:,} output tokens** in the supplied corpus. Its repetitions are not independent accuracy samples. [Differences and pronunciation examples](../benchmark/results/jieba/runtime.json).", '',
    '### Cold initialization', '', 'Medians from nine fresh processes per candidate. The filesystem cache is not cleared. RSS includes Node and the first result.', '',
    '| Implementation | Module load, ms | First conversion, ms | Combined, ms | RSS, MiB |', '| --- | ---: | ---: | ---: | ---: |']
for name in names:
    records = [r for r in runtime['cold'] if r['implementation'] == name]
    assert len(records) == 9
    medians = [statistics.median(r[key] for r in records) for key in ['loadMs', 'firstConversionMs', 'totalMs', 'rssBytes']]
    lines.append(f"| {labels[name]} | {medians[0]:.3f} | {medians[1]:.3f} | {medians[2]:.3f} | {medians[3]/2**20:.1f} |")
lines += ['', 'Jieba initialization is paid on its first non-ASCII conversion; it is absent from warmed throughput samples. Selecting the phrase resolver does not initialize Jieba.', '',
    '### Asynchronous conversion', '', 'Median milliseconds for the supplied corpus, seven warmed rounds. Input Buffer creation is outside timing; the binding snapshot, queueing, segmentation, formatting, and JS result creation are included.', '',
    '| Implementation | End-to-end latency, ms |', '| --- | ---: |']
for name in names[:-1]:
    records = [r for r in runtime['asynchronous'] if r['implementation'] == name]
    assert len(records) == 7
    lines.append(f"| {labels[name]} | {statistics.median(r['elapsedMs'] for r in records):.3f} |")
lines += ['', '### Large natural-text scaling', '', 'Tone-marked arrays, three fresh-process runs per size and candidate, warmed on the original corpus. Readings differ from pinyin-pro.', '',
    '| Characters | Implementation | Median conversion, ms | Median peak RSS, MiB |', '| ---: | --- | ---: | ---: |']
scaling = json.loads((results / 'scaling.json').read_text())
for size in [1_000_000, 10_000_000]:
    for name in names:
        records = [r for r in scaling['rows'] if r['length'] == size and r['implementation'] == name]
        assert len(records) == 3
        assert len({r['outputHash'] for r in records}) == 1
        lines.append(f"| {size:,} | {labels[name]} | {statistics.median(r['elapsedMs'] for r in records):,.2f} | {statistics.median(r['maxRssKiB'] for r in records)/1024:,.1f} |")
lines += ['', '[Scaling samples and output hashes](../benchmark/results/jieba/scaling.json).', '']
build = json.loads((results / 'build.json').read_text())
lines += ['### Artifact size', '', f"The native binary containing both resolvers is {build['native']['bytes']:,} bytes. The pre-integration binary was {build['before_jieba']['bytes']:,} bytes; the original binding was {build['original']['bytes']:,} bytes. Optional Rust consumers can disable Jieba entirely; the shipped Node binary includes its dictionary even when callers select the phrase resolver.", '']
path = root / 'docs/performance-research.md'
report = path.read_text()
start = report.index('<!-- jieba-results:start -->') + len('<!-- jieba-results:start -->')
end = report.index('<!-- jieba-results:end -->')
path.write_text(report[:start] + '\n\n' + '\n'.join(lines) + '\n' + report[end:])
with (results / 'summary.csv').open('w') as f:
    writer = csv.writer(f)
    writer.writerow(['mode','workload','style','original_us','phrase_us','jieba_us','pinyin_pro_us','jieba_speedup_js','jieba_equals_js'])
    writer.writerows(summary)
print('Updated Jieba report and summary.csv')
