import assert from 'node:assert/strict'
import { createHash } from 'node:crypto'
import { mkdirSync, readFileSync, writeFileSync } from 'node:fs'
import { createRequire } from 'node:module'
import { cpus, freemem, loadavg, totalmem } from 'node:os'
import { dirname, resolve } from 'node:path'
import { performance } from 'node:perf_hooks'

const require = createRequire(import.meta.url)
type Binding = typeof import('../index')
const baselinePath = process.env.PINYIN_BASELINE || 'target/remaining-simd-baseline/before.node'
assert.ok(baselinePath, 'Set PINYIN_BASELINE to the release addon saved before this optimization pass')
const currentPath = resolve(process.env.PINYIN_CURRENT || 'pinyin.darwin-arm64.node')
const before: Binding = require(resolve(baselinePath))
const after: Binding = require(currentPath)
const pro = require('pinyin-pro')
const rounds = Number(process.env.BENCH_ROUNDS || 7)
const duration = Number(process.env.BENCH_MS || 100)
const outputPath = process.env.BENCH_OUTPUT || 'benchmark/results/remaining-simd/conversion.json'
const filter = new RegExp(process.env.BENCH_FILTER || '')
const literature = readFileSync(new URL('./long.txt', import.meta.url), 'utf8')
let seed = 123456789
const alphabet = Array.from('文汉拼音山水风雨春夏秋冬天日月星光')
const matched = Array.from({ length: 100000 }, () => {
  seed = (Math.imul(seed, 1664525) + 1013904223) >>> 0
  return alphabet[(seed >>> 16) % alphabet.length]
}).join('')
const fixtures = [
  { name: 'short', text: '重庆银行音乐你好' },
  { name: 'ascii-300kb', text: 'Hello Rust / api?v=2026; '.repeat(14000).slice(0, 300000) },
  { name: 'literature-1k', text: literature.slice(0, 1000) },
  { name: 'literature-100k', text: literature.repeat(6).slice(0, 100000) },
  { name: 'mixed-100k', text: '中国 API é🙂 \0音乐，2026! '.repeat(5000) },
  { name: 'matched-100k', text: matched },
  { name: 'long-ascii', text: '中'.repeat(32) + ' api=value '.repeat(10000) },
  { name: 'long-unicode', text: '中'.repeat(32) + 'é🙂 café ∑ '.repeat(10000) },
  { name: 'long-escaped', text: '中'.repeat(32) + 'é🙂 "api"\\\n\t\0'.repeat(10000) },
]
let sink = 0
const hash = (data: string | Buffer) => createHash('sha256').update(data).digest('hex')
function fileInfo(path: string) {
  const data = readFileSync(path)
  return { path, bytes: data.length, sha256: hash(data) }
}
function sample(fn: () => number) {
  const start = performance.now()
  let iterations = 0
  let elapsed = 0
  do {
    for (let i = 0; i < 16; i++) sink ^= fn()
    iterations += 16
    elapsed = performance.now() - start
  } while (elapsed < duration)
  return { ns: (elapsed * 1e6) / iterations, iterations, elapsedMs: elapsed }
}
function measure(cases: { name: string; fn: () => number }[]) {
  const samples = cases.map(() => [] as ReturnType<typeof sample>[])
  for (const c of cases) for (let i = 0; i < 32; i++) sink ^= c.fn()
  for (let round = 0; round < rounds; round++) {
    for (let j = 0; j < cases.length; j++) {
      const i = (round + j) % cases.length
      samples[i].push(sample(cases[i].fn))
    }
  }
  return cases.map((c, i) => {
    const times = samples[i].map((s) => s.ns).sort((a, b) => a - b)
    return {
      implementation: c.name,
      medianNs: times[Math.floor(times.length / 2)],
      minNs: times[0],
      maxNs: times.at(-1),
      samples: samples[i],
    }
  })
}
const rows: object[] = []
const report = {
  timestamp: new Date().toISOString(),
  node: process.version,
  v8: process.versions.v8,
  platform: process.platform,
  arch: process.arch,
  cpu: cpus()[0].model,
  totalmem: totalmem(),
  freemem: freemem(),
  loadavg: loadavg(),
  rounds,
  durationMs: duration,
  pinyinPro: require('pinyin-pro/package.json').version,
  pinyinProSha256: hash(readFileSync(require.resolve('pinyin-pro'))),
  artifacts: [fileInfo(resolve(baselinePath)), fileInfo(currentPath)],
  sources: [
    'Cargo.toml',
    'Cargo.lock',
    'src/lib.rs',
    'src/encoding.rs',
    'src/output.rs',
    'crates/pinyin-core/build.rs',
    'crates/pinyin-core/src/lib.rs',
    'crates/pinyin-core/src/utf16.rs',
    'crates/pinyin-core/src/jieba.rs',
    'benchmark/remaining-simd.ts',
    'crates/pinyin-kernels/src/lib.rs',
    'crates/pinyin-kernels/Cargo.toml',
    'crates/pinyin-kernels/src/wasm.rs',
    'vendor/jieba-rs/src/lib.rs',
    'vendor/jieba-rs/src/simd_classifier.rs',
    'vendor/jieba-rs/Cargo.toml',
  ].map(fileInfo),
  rows,
}
mkdirSync(dirname(outputPath), { recursive: true })
function record(row: object) {
  rows.push(row)
  writeFileSync(outputPath, JSON.stringify({ ...report, sink }, null, 2) + '\n')
}
// Correctness is outside timing, including lone JavaScript surrogates.
for (const text of ['', '你好', '中\0国', '\ud800中国\udc00', '中"\\\n\u001f🙂é\u2028国'.repeat(50)]) {
  for (const style of [0, 1, 2, 3, 4])
    for (const resolver of ['character', 'phrase', 'jieba']) {
      const options = {
        style,
        segment: resolver !== 'character',
        segmenter: resolver === 'jieba' ? ('jieba' as const) : ('phrase' as const),
      }
      for (const input of [text, Buffer.from(text)]) {
        assert.deepEqual(after.pinyin(input, options), before.pinyin(input, options))
        assert.deepEqual(
          after.pinyin(input, { ...options, heteronym: true }),
          before.pinyin(input, { ...options, heteronym: true }),
        )
        assert.equal(
          after.pinyinString(input, { ...options, separator: '\0🙂' }),
          before.pinyinString(input, { ...options, separator: '\0🙂' }),
        )
        assert.deepEqual(await after.asyncPinyin(input, options), await before.asyncPinyin(input, options))
      }
    }
}
for (const f of fixtures) {
  for (const resolver of f.name.startsWith('ascii') ? ['character'] : ['character', 'phrase', 'jieba']) {
    for (const style of [0, 1])
      for (const api of ['pinyin', 'pinyinString'] as const)
        for (const inputType of ['string', 'buffer']) {
          const label = `${f.name}/${resolver}/${style}/${api}/${inputType}`
          if (!filter.test(label)) continue
          const options = {
            style,
            segment: resolver !== 'character',
            segmenter: resolver === 'jieba' ? ('jieba' as const) : ('phrase' as const),
          }
          const input = inputType === 'string' ? f.text : Buffer.from(f.text)
          const expected = before[api](input, options)
          assert.deepEqual(after[api](input, options), expected, label)
          const cases = [
            { name: 'before', fn: () => before[api](input, options).length },
            { name: 'after', fn: () => after[api](input, options).length },
          ]
          let matchesPro: boolean | null = null
          if (
            inputType === 'string' &&
            (f.name === 'matched-100k' || (f.name === 'literature-100k' && resolver === 'jieba'))
          ) {
            const fn = () =>
              pro.pinyin(f.text, {
                type: api === 'pinyin' ? 'array' : 'string',
                toneType: style === 1 ? 'symbol' : 'none',
                toneSandhi: false,
                nonZh: 'consecutive',
              })
            matchesPro = hash(JSON.stringify(fn())) === hash(JSON.stringify(expected))
            if (f.name === 'matched-100k') assert.ok(matchesPro)
            cases.push({ name: 'pinyin-pro', fn: () => fn().length })
          }
          const result = measure(cases)
          record({
            fixture: f.name,
            resolver,
            style,
            api,
            inputType,
            inputBytes: Buffer.byteLength(f.text),
            inputCharacters: Array.from(f.text).length,
            inputHash: hash(f.text),
            outputHash: hash(JSON.stringify(expected)),
            matchesPro,
            result,
          })
          console.log(label, result.map((r) => `${r.implementation} ${(r.medianNs / 1000).toFixed(2)}µs`).join(' | '))
        }
  }
}

for (const f of fixtures.filter((f) => ['short', 'literature-100k', 'mixed-100k'].includes(f.name))) {
  for (const style of [0, 1])
    for (const inputType of ['string', 'buffer']) {
      const label = `${f.name}/character/${style}/heteronym/${inputType}`
      if (!filter.test(label)) continue
      const input = inputType === 'string' ? f.text : Buffer.from(f.text)
      const options = { style, heteronym: true }
      const expected = before.pinyin(input, options)
      assert.deepEqual(after.pinyin(input, options), expected, label)
      const result = measure([
        { name: 'before', fn: () => before.pinyin(input, options).length },
        { name: 'after', fn: () => after.pinyin(input, options).length },
      ])
      record({
        fixture: f.name,
        resolver: 'character',
        style,
        api: 'heteronym',
        inputType,
        inputBytes: Buffer.byteLength(f.text),
        inputHash: hash(f.text),
        outputHash: hash(JSON.stringify(expected)),
        result,
      })
      console.log(label, result.map((r) => `${r.implementation} ${(r.medianNs / 1000).toFixed(2)}µs`).join(' | '))
    }
}

const asyncFixture = fixtures.find((f) => f.name === 'literature-100k')!
for (const resolver of ['character', 'jieba'])
  for (const inputType of ['string', 'buffer']) {
    const label = `${asyncFixture.name}/${resolver}/1/asyncPinyin/${inputType}`
    if (!filter.test(label)) continue
    const input = inputType === 'string' ? asyncFixture.text : Buffer.from(asyncFixture.text)
    const options = { style: 1, segment: resolver === 'jieba', segmenter: 'jieba' as const }
    const expected = await before.asyncPinyin(input, options)
    assert.deepEqual(await after.asyncPinyin(input, options), expected)
    const libraries = [before, after]
    const samples: ReturnType<typeof sample>[][] = [[], []]
    for (let round = 0; round < rounds; round++)
      for (let j = 0; j < libraries.length; j++) {
        const i = (round + j) % libraries.length
        const start = performance.now()
        let iterations = 0
        let elapsed = 0
        do {
          sink ^= (await libraries[i].asyncPinyin(input, options)).length
          iterations++
          elapsed = performance.now() - start
        } while (elapsed < duration)
        samples[i].push({ ns: (elapsed * 1e6) / iterations, iterations, elapsedMs: elapsed })
      }
    const result = samples.map((values, i) => {
      const times = values.map((s) => s.ns).sort((a, b) => a - b)
      return {
        implementation: i ? 'after' : 'before',
        medianNs: times[Math.floor(times.length / 2)],
        minNs: times[0],
        maxNs: times.at(-1),
        samples: values,
      }
    })
    record({
      fixture: asyncFixture.name,
      resolver,
      style: 1,
      api: 'asyncPinyin',
      inputType,
      inputBytes: Buffer.byteLength(asyncFixture.text),
      inputHash: hash(asyncFixture.text),
      outputHash: hash(JSON.stringify(expected)),
      result,
    })
    console.log(label, result.map((r) => `${r.implementation} ${(r.medianNs / 1000).toFixed(2)}µs`).join(' | '))
  }

if (filter.test('compare')) {
  for (const [name, a, b] of [
    ['short-chinese', '重庆银行', '中国银行'],
    ['common-chinese-prefix', '重庆'.repeat(64) + '银行', '重庆'.repeat(64) + '音乐'],
    ['identical', '音乐重庆银行'.repeat(32), '音乐重庆银行'.repeat(32)],
    ['accented', 'éāǖế🙂', 'éāǖề🙂'],
  ]) {
    assert.equal(after.compare(a, b), before.compare(a, b))
    record({
      fixture: name,
      api: 'compare',
      inputHash: hash(JSON.stringify([a, b])),
      result: measure([
        { name: 'before', fn: () => before.compare(a, b) },
        { name: 'after', fn: () => after.compare(a, b) },
      ]),
    })
  }
}
