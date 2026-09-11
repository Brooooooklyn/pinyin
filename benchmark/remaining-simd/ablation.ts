import assert from 'node:assert/strict'
import { createHash } from 'node:crypto'
import { mkdirSync, readFileSync, writeFileSync } from 'node:fs'
import { createRequire } from 'node:module'
import { cpus, loadavg } from 'node:os'
import { resolve } from 'node:path'
import { performance } from 'node:perf_hooks'

const require = createRequire(import.meta.url)
type Binding = typeof import('../../index')
const productionPath = resolve('pinyin.darwin-arm64.node')
const probePath = resolve('target/remaining-simd-probe/probe.node')
const production: Binding = require(productionPath)
const probe: Binding & {
  researchSetMode(mode: number): void
} = require(probePath)
const names = ['none', 'trie', 'decode', 'trie+decode', 'reuse', 'trie+reuse', 'decode+reuse', 'all']
const rounds = Number(process.env.BENCH_ROUNDS || 5)
const duration = Number(process.env.BENCH_MS || 60)
const filter = new RegExp(process.env.BENCH_FILTER || '')
const outputPath = process.env.BENCH_OUTPUT || 'benchmark/results/remaining-simd/ablation.json'
const hash = (data: string | Buffer) => createHash('sha256').update(data).digest('hex')
const fileInfo = (path: string) => ({ path, sha256: hash(readFileSync(path)) })
const literature = readFileSync(new URL('../long.txt', import.meta.url), 'utf8')
const controls = Array.from({ length: 32 }, (_, i) => String.fromCharCode(i)).join('')
const fixtures = [
  { name: 'literature-100k', text: literature.repeat(6).slice(0, 100000) },
  { name: 'mixed', text: '中国 API é🙂 \0音乐，2026! '.repeat(5000) },
  { name: 'mixed-clean', text: '中国 API é🙂 音乐，2026! '.repeat(5000) },
  { name: 'escape-dense', text: '中国 "x"\\\n\t\0🙂 '.repeat(5000) },
  { name: 'long-ascii-run', text: '中'.repeat(32) + ' api=value '.repeat(10000) },
  { name: 'long-unicode-run', text: '中'.repeat(32) + 'é🙂 café ∑ '.repeat(10000) },
  { name: 'long-escaped-run', text: '中'.repeat(32) + 'é🙂 "api"\\\n\t\0'.repeat(10000) },
]
const cases = [
  '',
  '重庆银行音乐你好',
  '\ud800中国\udc00',
  '中'.repeat(33) + controls + '"\\🙂é\u2028\u2029\ufeff',
  ('中'.repeat(5) + controls + '🙂é"\\').repeat(8),
  ...Array.from({ length: 130 }, (_, i) => '中'.repeat(33) + 'x'.repeat(i) + '"\\\0🙂é'),
]
let checks = 0
// Compare every mode with production, outside timing. Cover all styles,
// resolvers, buffer/string input, multibyte tails, controls and heteronyms.
for (let mode = 0; mode < names.length; mode++) {
  probe.researchSetMode(mode)
  for (const text of cases) {
    for (const style of [0, 1, 2, 3, 4]) {
      for (const resolver of ['character', 'phrase', 'jieba']) {
        const options = {
          style,
          segment: resolver !== 'character',
          segmenter: resolver === 'jieba' ? ('jieba' as const) : ('phrase' as const),
        }
        for (const input of [text, Buffer.from(text)]) {
          assert.deepEqual(probe.pinyin(input, options), production.pinyin(input, options))
          assert.deepEqual(
            probe.pinyin(input, { ...options, heteronym: true }),
            production.pinyin(input, { ...options, heteronym: true }),
          )
          assert.equal(
            probe.pinyinString(input, { ...options, separator: '\0🙂' }),
            production.pinyinString(input, { ...options, separator: '\0🙂' }),
          )
          checks += 3
        }
      }
    }
  }
  for (const text of cases.slice(0, 5)) {
    for (const style of [0, 1]) {
      const options = { style, heteronym: true, segment: true, segmenter: 'jieba' as const }
      assert.deepEqual(await probe.asyncPinyin(text, options), await production.asyncPinyin(text, options))
      checks++
    }
  }
}
console.log(checks + ' correctness comparisons passed')
let sink = 0
function sample(fn: () => number) {
  const start = performance.now()
  let iterations = 0
  let elapsed = 0
  do {
    for (let i = 0; i < 8; i++) sink ^= fn()
    iterations += 8
    elapsed = performance.now() - start
  } while (elapsed < duration)
  return { ns: (elapsed * 1e6) / iterations, iterations, elapsedMs: elapsed }
}
function measure(fn: () => number) {
  const samples = names.map(() => [] as ReturnType<typeof sample>[])
  for (let mode = 0; mode < names.length; mode++) {
    probe.researchSetMode(mode)
    for (let i = 0; i < 32; i++) sink ^= fn()
  }
  for (let round = 0; round < rounds; round++) {
    for (let j = 0; j < names.length; j++) {
      const mode = (round + j) % names.length
      probe.researchSetMode(mode)
      samples[mode].push(sample(fn))
    }
  }
  return names.map((name, i) => {
    const times = samples[i].map((s) => s.ns).sort((a, b) => a - b)
    return { name, medianNs: times[Math.floor(times.length / 2)], samples: samples[i] }
  })
}
const rows: object[] = []
const report = {
  timestamp: new Date().toISOString(),
  node: process.version,
  cpu: cpus()[0].model,
  platform: process.platform,
  arch: process.arch,
  loadavg: loadavg(),
  rounds,
  durationMs: duration,
  checks,
  artifacts: [fileInfo(productionPath), fileInfo(probePath)],
  harness: fileInfo(resolve('benchmark/remaining-simd/ablation.ts')),
  profiles: fixtures.map((f) => ({
    name: f.name,
    utf16Units: f.text.length,
    utf8Bytes: Buffer.byteLength(f.text),
    sha256: hash(f.text),
    // Character resolver: tokens, unmapped runs, unmapped bytes,
    // runs >=64 bytes, bytes in long runs, escapable bytes.
    counts: [],
  })),
  rows,
}
mkdirSync(resolve(outputPath, '..'), { recursive: true })
const jobs = fixtures
  .filter((f) => ['literature-100k', 'mixed-clean', 'long-ascii-run'].includes(f.name))
  .flatMap((fixture) =>
    ['phrase', 'jieba'].flatMap((resolver) =>
      (['pinyin', 'pinyinString'] as const).map((api) => ({ fixture, resolver, style: 1, api, heteronym: false })),
    ),
  )
for (const { fixture, resolver, style, api, heteronym } of jobs) {
  const label = [fixture.name, resolver, style, api, heteronym].join('/')
  if (!filter.test(label)) continue
  const options = {
    style,
    heteronym,
    segment: resolver !== 'character',
    segmenter: resolver === 'jieba' ? ('jieba' as const) : ('phrase' as const),
  }
  const expected = production[api](fixture.text, options)
  for (let mode = 0; mode < names.length; mode++) {
    probe.researchSetMode(mode)
    assert.deepEqual(probe[api](fixture.text, options), expected, label)
  }
  const results = measure(() => probe[api](fixture.text, options).length)
  const baseline = results[0].medianNs
  const row = {
    label,
    fixture: fixture.name,
    resolver,
    style,
    api,
    heteronym,
    outputSha256: hash(JSON.stringify(expected)),
    results: results.map((r) => ({ ...r, speedup: baseline / r.medianNs })),
  }
  rows.push(row)
  writeFileSync(outputPath, JSON.stringify({ ...report, sink }, null, 2) + '\n')
  console.log(label, row.results.map((r) => r.name + ': ' + (r.medianNs / 1e6).toFixed(3) + 'ms').join(', '))
}
