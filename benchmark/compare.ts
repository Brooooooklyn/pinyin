import assert from 'node:assert/strict'
import { createHash } from 'node:crypto'
import { readFileSync, writeFileSync } from 'node:fs'
import { createRequire } from 'node:module'
import { cpus, freemem, loadavg, platform, totalmem } from 'node:os'
import { resolve } from 'node:path'
import { performance } from 'node:perf_hooks'

const require = createRequire(import.meta.url)
const native = require('../index.js')
const pro = require('pinyin-pro')
const baseline = process.env.PINYIN_BASELINE ? require(resolve(process.env.PINYIN_BASELINE)) : undefined
const text = readFileSync(new URL('./long.txt', import.meta.url), 'utf8')
const rounds = Number(process.env.BENCH_ROUNDS || 7)
const duration = Number(process.env.BENCH_MS || 100)
const outputMode = process.env.BENCH_MODE || 'array'
const jiebaComparison = process.env.BENCH_JIEBA === 'true'
let sink = 0
let seed = 123456789
const alphabet = Array.from('文汉拼音山水风雨春夏秋冬天日月星光')
const matched = Array.from({ length: 100000 }, () => {
  seed = (Math.imul(seed, 1664525) + 1013904223) >>> 0
  return alphabet[(seed >>> 16) % alphabet.length]
}).join('')

const cases = [
  { name: 'short', inputs: ['你好拼音', '汉语拼音', '中国文化', '学习中文'] },
  {
    name: 'mixed',
    inputs: ['你好，Rust 2026! Hello 👋', '中文 API v2.0 🚀', '文档：https://example.com/path?a=1', '拼音(pinyin)'],
  },
  { name: 'ascii-4k', inputs: ['Hello Rust / api?v=2026; '.repeat(180)] },
  { name: 'literature-1k', inputs: [text.slice(0, 1000), text.slice(3000, 4000), text.slice(6000, 7000)] },
  { name: 'literature-10k', inputs: [text.slice(0, 10000), text.slice(1000, 11000)] },
  { name: 'literature-100k', inputs: [text.repeat(6).slice(0, 100000)] },
  { name: 'matched-10k', inputs: [matched.slice(0, 10000), matched.slice(20000, 30000)] },
  { name: 'matched-100k', inputs: [matched] },
  { name: 'phrases', inputs: ['重庆银行行长重新处理音乐，快乐成长。', '研究生命起源，长大以后参加音乐会。'] },
]

function checksum(value: unknown) {
  return createHash('sha256').update(JSON.stringify(value)).digest('hex')
}

function sample(fn: () => any) {
  const probe = performance.now()
  sink ^= fn().length
  const batch = Math.max(1, Math.min(512, Math.floor(0.25 / Math.max(performance.now() - probe, 0.000001))))
  let iterations = 0
  const start = performance.now()
  let elapsed = 0
  do {
    for (let i = 0; i < batch; i++) {
      const value = fn()
      sink ^= value.length
      iterations++
    }
    elapsed = performance.now() - start
  } while (elapsed < duration)
  return { ns: (elapsed * 1e6) / iterations, iterations, elapsedMs: elapsed }
}

const rows = []
for (const style of [0, 1]) {
  for (const segment of jiebaComparison ? [true] : [false, true]) {
    for (const fixture of cases) {
      if (process.env.BENCH_FILTER && !fixture.name.includes(process.env.BENCH_FILTER)) continue
      const options = { style, segment, heteronym: outputMode === 'heteronym' }
      const jiebaOptions = { ...options, segmenter: 'jieba' }
      const proOptions = {
        type: outputMode === 'string' ? 'string' : 'array',
        toneType: style === 0 ? 'none' : 'symbol',
        nonZh: 'consecutive',
        toneSandhi: false,
      }
      const rustCall = (library: typeof native, input: string, selectedOptions = options) =>
        outputMode === 'string'
          ? library.pinyinString
            ? library.pinyinString(input, selectedOptions)
            : library.pinyin(input, selectedOptions).join(' ')
          : library.pinyin(input, selectedOptions)
      const implementations = [
        ...(baseline ? [{ name: 'baseline', fn: (input: string) => rustCall(baseline, input) }] : []),
        { name: 'rust', fn: (input: string) => rustCall(native, input) },
        ...(jiebaComparison
          ? [{ name: 'rust-jieba', fn: (input: string) => rustCall(native, input, jiebaOptions) }]
          : []),
        {
          name: 'pinyin-pro',
          fn: (input: string) =>
            outputMode === 'heteronym' ? pro.polyphonic(input, proOptions) : pro.pinyin(input, proOptions),
        },
      ]
      const outputs = implementations.map(({ fn }) => fixture.inputs.map(fn))
      if (baseline && !segment) assert.deepEqual(outputs[0], outputs[1], `${fixture.name}: legacy compatibility`)
      const reference = checksum(outputs.find((_, i) => implementations[i].name === 'pinyin-pro'))
      if (fixture.name.startsWith('matched-') && outputMode !== 'heteronym') {
        for (const output of outputs)
          assert.equal(checksum(output), reference, `${fixture.name}: output must match pinyin-pro`)
      }
      const samples = implementations.map(() => [] as ReturnType<typeof sample>[])
      // Warm every implementation, then rotate execution order to distribute drift.
      for (const impl of implementations) {
        for (let i = 0; i < 32; i++) sink ^= impl.fn(fixture.inputs[i % fixture.inputs.length]).length
      }
      for (let round = 0; round < rounds; round++) {
        for (let j = 0; j < implementations.length; j++) {
          const index = (round + j) % implementations.length
          let cursor = 0
          samples[index].push(sample(() => implementations[index].fn(fixture.inputs[cursor++ % fixture.inputs.length])))
        }
      }
      const result = implementations.map((impl, index) => {
        const times = samples[index].map((s) => s.ns).sort((a, b) => a - b)
        return {
          implementation: impl.name,
          medianNs: times[Math.floor(times.length / 2)],
          minNs: times[0],
          maxNs: times[times.length - 1],
          outputHash: checksum(outputs[index]),
          matchesPro: checksum(outputs[index]) === reference,
          samples: samples[index],
        }
      })
      const row = {
        fixture: fixture.name,
        style,
        segment,
        outputMode,
        ...(jiebaComparison ? { segmenters: ['phrase', 'jieba'], jiebaHmm: false, jiebaBoundaryPenalty: 1 } : {}),
        inputHashes: fixture.inputs.map((s) => createHash('sha256').update(s).digest('hex')),
        inputCharacters: fixture.inputs.map((s) => Array.from(s).length),
        inputBytes: fixture.inputs.map((s) => Buffer.byteLength(s)),
        result,
      }
      rows.push(row)
      console.log(
        fixture.name,
        { style, segment },
        result
          .map((r) => `${r.implementation}: ${(r.medianNs / 1000).toFixed(2)} µs${r.matchesPro ? ' =' : ' ≠'}`)
          .join(' | '),
      )
    }
  }
}
const report = {
  timestamp: new Date().toISOString(),
  node: process.version,
  platform: platform(),
  arch: process.arch,
  cpu: cpus()[0].model,
  totalmem: totalmem(),
  freemem: freemem(),
  loadavg: loadavg(),
  pinyinPro: require('pinyin-pro/package.json').version,
  baseline: process.env.PINYIN_BASELINE || null,
  rounds,
  durationMs: duration,
  sink,
  rows,
}
writeFileSync(process.env.BENCH_OUTPUT || 'benchmark/results/comparison.json', JSON.stringify(report, null, 2) + '\n')
