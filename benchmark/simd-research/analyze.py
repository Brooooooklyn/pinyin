"""Summarize the experiments and inspect the static pronunciation trie."""
import csv
import json
from pathlib import Path
import statistics
from collections import Counter, defaultdict

root = Path(__file__).resolve().parents[2]
directory = root / 'benchmark/results/simd-research'
groups = defaultdict(list)
for line in (directory / 'kernels.jsonl').read_text().splitlines():
    row = json.loads(line)
    groups[row['fixture'], row['operation'], row['implementation']].append(row['ns'])
assert len(groups) == 128
assert all(len(values) == 7 for values in groups.values())
medians = {key: statistics.median(values) / 1000 for key, values in groups.items()}
with (directory / 'kernels.csv').open('w', newline='') as handle:
    writer = csv.writer(handle)
    writer.writerow(['fixture', 'operation', 'implementation', 'median_us', 'min_us', 'max_us'])
    for key, values in groups.items():
        writer.writerow([*key, medians[key], min(values)/1000, max(values)/1000])

nodes = [{}]
for line in (root / 'crates/pinyin-core/data/phrases.tsv').read_text().splitlines():
    if not line or line.startswith('#'): continue
    word = line.split('\t')[0]
    state = 0
    for ch in word:
        if ch not in nodes[state]:
            nodes[state][ch] = len(nodes)
            nodes.append({})
        state = nodes[state][ch]
fanouts = Counter(len(n) for n in nodes[1:])
traversals = {}
corpus = (root / 'benchmark/long.txt').read_text()
for name, text in [('literature-100k', (corpus * (100000 // len(corpus) + 1))[:100000]),
                   ('mixed-100k', '中国 API é🙂 \0音乐，2026! ' * 5000)]:
    # Static trace of child() requests, not a CPU profile or runtime percentage.
    degrees = Counter()
    hits = 0
    for i, ch in enumerate(text):
        state = nodes[0].get(ch)
        if state is None: continue
        hits += 1
        for following in text[i+1:i+5]:
            degrees[len(nodes[state])] += 1
            state = nodes[state].get(following)
            if state is None: break
    traversals[name] = {'characters':len(text), 'root_hits':hits, 'child_requests_by_degree': dict(sorted(degrees.items()))}
stats = {'nodes':len(nodes), 'nonroot_nodes':len(nodes)-1, 'root_degree':len(nodes[0]),
         'nonroot_degree_histogram':dict(sorted(fanouts.items())), 'trace':traversals}
(directory / 'trie.json').write_text(json.dumps(stats, indent=2)+'\n')

tables = {}
lines = ['| Input | Scalar decode, µs | SIMD count + decode, µs | SIMD time change |', '| --- | ---: | ---: | ---: |']
for fixture in ['short','literature-1k','literature-100k','mixed-100k','ascii-run']:
    a,b = [medians[fixture,'decode-input-scalars',impl] for impl in ['std-chars','simdutf-count-and-decode']]
    lines.append(f'| {fixture} | {a:.2f} | {b:.2f} | {(b/a-1)*100:+.1f}% |')
tables['decode'] = lines
lines = ['| Input / output | std collect, µs | std reserved, µs | encoding_rs stable, µs | simdutf, µs |', '| --- | ---: | ---: | ---: | ---: |']
for fixture in ['short','literature-100k','mixed-100k','ascii-run']:
    for kind in ['joined','json']:
        v=[medians[fixture,'utf16-'+kind,impl] for impl in ['std-collect','std-reserved','encoding-rs-stable','simdutf']]
        lines.append(f'| {fixture} / {kind} | '+' | '.join(f'{value:.2f}' for value in v)+' |')
tables['transcode'] = lines
lines = ['| Input / resolver | Current string preparation, µs | SIMD transcode, µs | Direct cached UTF-16, µs |', '| --- | ---: | ---: | ---: |']
for fixture in ['literature-100k','mixed-100k']:
    for resolver in ['character','phrase','jieba']:
        values=[medians[fixture,'joined-total-'+resolver,impl] for impl in ['current-scalar-utf16','simd-utf16','direct-cached-utf16']]
        lines.append(f'| {fixture} / {resolver} | '+' | '.join(f'{v:.2f}' for v in values)+' |')
tables['direct'] = lines
lines = ['| Input / resolver | SIMD transcode only, µs | SIMD transcode + escape scan, µs | Additional time change |', '| --- | ---: | ---: | ---: |']
for fixture in ['literature-100k','mixed-100k']:
    for resolver in ['character','jieba']:
        a,b=[medians[fixture,'json-total-'+resolver,impl] for impl in ['simd-utf16','simd-utf16-and-escape']]
        lines.append(f'| {fixture} / {resolver} | {a:.2f} | {b:.2f} | {(b/a-1)*100:+.1f}% |')
tables['escape'] = lines

node = json.loads((directory/'node.json').read_text())
assert len(node['rows']) == 42
assert all(len(r['samples'])==7 for row in node['rows'] for r in row['result'])
with (directory/'node.csv').open('w',newline='') as handle:
    writer=csv.writer(handle)
    writer.writerow(['fixture','api','resolver','style','implementation','median_us','matches_pro'])
    for row in node['rows']:
        for value in row['result']:
            writer.writerow([row['fixture'],row['api'],row.get('resolver',''),row.get('style',''),value['implementation'],value['medianNs']/1000,row.get('matchesPro','')])
lines=['| Input / resolver / API | Scalar output, µs | SIMD output, µs | Time change | pinyin-pro, µs |', '| --- | ---: | ---: | ---: | ---: |']
for row in node['rows']:
    if row.get('style')!=1 or row['fixture'] not in ['literature-100k','mixed-100k','matched-100k'] or row['resolver']=='phrase': continue
    values=[r['medianNs']/1000 for r in row['result']]
    a,b=values[:2]
    pro=f'{values[2]:.2f}' if len(values)>2 else '—'
    lines.append(f"| {row['fixture']} / {row['resolver']} / {row['api']} | {a:.2f} | {b:.2f} | {(b/a-1)*100:+.1f}% | {pro} |")
tables['node']=lines
lines=['| Input | N-API UTF-8 input extraction, µs | N-API UTF-16 input extraction, µs |', '| --- | ---: | ---: |']
for row in node['rows']:
    if row['api']=='input-copy-only':
        a,b=[r['medianNs']/1000 for r in row['result']]
        lines.append(f"| {row['fixture']} | {a:.2f} | {b:.2f} |")
tables['input']=lines
report=root/'docs/performance-research.md'
if report.exists():
    text=report.read_text()
    for name,lines in tables.items():
        start,end=f'<!-- {name}:start -->',f'<!-- {name}:end -->'
        before,remaining=text.split(start,1)
        _,after=remaining.split(end,1)
        text=before+start+'\n\n'+'\n'.join(lines)+'\n\n'+end+after
    report.write_text(text)
print(json.dumps(stats,indent=2))
