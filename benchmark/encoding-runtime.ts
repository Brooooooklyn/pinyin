import assert from 'node:assert/strict'
import { spawnSync } from 'node:child_process'
import { createHash } from 'node:crypto'
import { mkdirSync, readFileSync, writeFileSync } from 'node:fs'
import { createRequire } from 'node:module'
import { cpus } from 'node:os'
import { resolve } from 'node:path'

const require = createRequire(import.meta.url)
assert.ok(process.env.PINYIN_BASELINE)
const candidates = [
  { name: 'before', path: resolve(process.env.PINYIN_BASELINE) },
  { name: 'after', path: resolve(process.env.PINYIN_CURRENT || 'pinyin.darwin-arm64.node') },
]
const child = `
const {performance} = require('node:perf_hooks');
const {readFileSync} = require('node:fs');
const {createHash} = require('node:crypto');
const info = JSON.parse(process.argv[1]);
const start = performance.now();
const library = require(info.path);
const loaded = performance.now();
const first = library.pinyin('重庆银行音乐', info.options);
const ready = performance.now();
if (info.mode === 'cold') {
  console.log(JSON.stringify({loadMs:loaded-start,firstMs:ready-loaded,totalMs:ready-start,rssBytes:process.memoryUsage().rss,result:first}));
} else {
  const corpus = readFileSync(info.corpus, 'utf8');
  const text = corpus.repeat(Math.ceil(1000000 / corpus.length)).slice(0, 1000000);
  global.gc();
  const before = process.memoryUsage().rss;
  const start = performance.now();
  const result = library[info.api](text, info.options);
  const elapsedMs = performance.now()-start;
  const after = process.memoryUsage().rss;
  const maxRssKiB = process.resourceUsage().maxRSS;
  const digest = createHash('sha256').update(JSON.stringify(result)).digest('hex');
  console.log(JSON.stringify({inputCodeUnits:text.length,inputBytes:Buffer.byteLength(text),elapsedMs,rssBeforeBytes:before,rssAfterBytes:after,maxRssKiB,outputHash:digest,outputLength:result.length}));
}
`
const rows: object[] = []
const path = 'benchmark/results/encoding/runtime.json'
mkdirSync('benchmark/results/encoding', { recursive: true })
for (const mode of ['cold', 'large'])
  for (const resolver of ['character', 'phrase', 'jieba']) {
    for (const api of mode === 'cold' ? ['pinyin'] : ['pinyin', 'pinyinString']) {
      for (let round = 0; round < (mode === 'cold' ? 9 : 5); round++) {
        const pair: any[] = []
        for (let j = 0; j < candidates.length; j++) {
          const candidate = candidates[(round + j) % candidates.length]
          const info = {
            ...candidate,
            mode,
            api,
            corpus: resolve('benchmark/long.txt'),
            options: {
              style: 1,
              segment: resolver !== 'character',
              segmenter: resolver === 'jieba' ? 'jieba' : 'phrase',
            },
          }
          const result = spawnSync(process.execPath, ['--expose-gc', '-e', child, JSON.stringify(info)], {
            encoding: 'utf8',
            timeout: 30000,
          })
          assert.equal(result.status, 0, result.stderr)
          const value = JSON.parse(result.stdout)
          pair.push(value)
          rows.push({ mode, resolver, api, round, implementation: candidate.name, ...value })
        }
        if (mode === 'cold') assert.deepEqual(pair[0].result, pair[1].result)
        else assert.equal(pair[0].outputHash, pair[1].outputHash)
      }
      console.log(mode, resolver, api, 'complete')
    }
  }
writeFileSync(
  path,
  JSON.stringify(
    {
      timestamp: new Date().toISOString(),
      node: process.version,
      cpu: cpus()[0].model,
      platform: process.platform,
      arch: process.arch,
      candidates: candidates.map((c) => ({
        ...c,
        sha256: createHash('sha256').update(readFileSync(c.path)).digest('hex'),
      })),
      scriptSha256: createHash('sha256').update(readFileSync('benchmark/encoding-runtime.ts')).digest('hex'),
      rows,
    },
    null,
    2,
  ) + '\n',
)
// Ensure the comparison never accidentally loads the package's WASI fallback.
assert.ok(require(candidates[1].path).pinyinString)
