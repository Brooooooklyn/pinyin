import assert from 'node:assert/strict'
import { spawnSync } from 'node:child_process'
import { readFileSync, writeFileSync } from 'node:fs'
import { createRequire } from 'node:module'
import { resolve } from 'node:path'

const require = createRequire(import.meta.url)
const jiebaComparison = process.env.BENCH_JIEBA === 'true'
const native = require('../index.js')
const nativePath = Object.keys(require.cache).find(
  (path) => path.endsWith('.node') && require.cache[path]?.exports.pinyin === native.pinyin,
)!
const corpusPath = new URL('./long.txt', import.meta.url).pathname
const child = `
const { performance } = require('node:perf_hooks');
const fs = require('node:fs');
const crypto = require('node:crypto');
const info = JSON.parse(process.argv[1]);
const source = fs.readFileSync(info.corpusPath, 'utf8');
const input = source.repeat(Math.ceil(info.length / source.length)).slice(0, info.length);
const lib = require(info.path);
const options = info.pro ? {type:'array',toneType:'symbol',nonZh:'consecutive',toneSandhi:false} : {style:1,segment:true,segmenter:info.segmenter || 'phrase'};
lib.pinyin(source, options);
const start = performance.now();
const output = lib.pinyin(input, options);
const elapsedMs = performance.now() - start;
const rssBytes = process.memoryUsage().rss;
const maxRssKiB = process.resourceUsage().maxRSS;
const hash = crypto.createHash('sha256');
for (const token of output) { hash.update(token); hash.update('\\0'); }
console.log(JSON.stringify({elapsedMs,rssBytes,maxRssKiB,tokens:output.length,outputHash:hash.digest('hex'),inputHash:crypto.createHash('sha256').update(input).digest('hex')}));
`
const rows = []
for (const length of [1_000_000, 10_000_000]) {
  for (let round = 0; round < 3; round++) {
    const implementations = [
      ...(process.env.PINYIN_BASELINE
        ? [{ name: 'baseline', path: resolve(process.env.PINYIN_BASELINE), pro: false }]
        : []),
      { name: 'rust', path: nativePath, pro: false },
      ...(jiebaComparison ? [{ name: 'rust-jieba', path: nativePath, pro: false, segmenter: 'jieba' }] : []),
      { name: 'pinyin-pro', path: require.resolve('pinyin-pro'), pro: true },
    ]
    for (let index = 0; index < implementations.length; index++) {
      const implementation = implementations[(round + index) % implementations.length]
      const result = spawnSync(
        process.execPath,
        ['-e', child, JSON.stringify({ ...implementation, length, corpusPath })],
        { encoding: 'utf8', timeout: 120000 },
      )
      assert.equal(result.status, 0, result.stderr)
      const row = { length, round, implementation: implementation.name, ...JSON.parse(result.stdout) }
      rows.push(row)
      console.log(
        length,
        round,
        row.implementation,
        row.elapsedMs.toFixed(2),
        'ms',
        (row.maxRssKiB / 1024).toFixed(1),
        'MiB peak RSS',
      )
    }
    writeFileSync(
      process.env.BENCH_OUTPUT || 'benchmark/results/scaling.json',
      JSON.stringify({ node: process.version, timestamp: new Date().toISOString(), rows }, null, 2) + '\n',
    )
  }
}
// Confirm the source used by the child processes is available and non-empty.
assert.ok(readFileSync(corpusPath).length)
