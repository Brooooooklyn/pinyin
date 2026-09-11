import assert from 'node:assert/strict'
import { createHash } from 'node:crypto'
import { mkdirSync, readFileSync, writeFileSync } from 'node:fs'
import { createRequire } from 'node:module'
import { cpus, freemem, loadavg, totalmem } from 'node:os'
import { dirname, resolve } from 'node:path'
import { performance } from 'node:perf_hooks'

const require = createRequire(import.meta.url)
type Binding = typeof import('../index')
const baselinePath = process.env.PINYIN_BASELINE
assert.ok(baselinePath, 'Set PINYIN_BASELINE to the release addon saved before changing UTF-8 validation')
const currentPath = resolve(process.env.PINYIN_CURRENT || 'pinyin.darwin-arm64.node')
const libraries: [string, Binding][] = [
  ['before-std', require(resolve(baselinePath))],
  ['after-simd-compat', require(currentPath)],
]
const rounds = Number(process.env.BENCH_ROUNDS || 7)
const duration = Number(process.env.BENCH_MS || 150)
const outputPath = process.env.BENCH_OUTPUT || 'benchmark/results/simdutf8/conversion.json'
const literature = readFileSync(new URL('./long.txt', import.meta.url), 'utf8')
const fixtures = [
  { name: 'short', text: '重庆银行音乐你好' },
  { name: 'ascii-300kb', text: 'Hello Rust / api?v=2026; '.repeat(14000).slice(0, 300000) },
  { name: 'literature-1k', text: literature.slice(0, 1000) },
  { name: 'literature-100k', text: literature.repeat(6).slice(0, 100000) },
  { name: 'mixed-100k', text: '中国 API é🙂 \0音乐，2026! '.repeat(5000) },
]
let sink = 0
function hash(data: string | Buffer) {
  return createHash('sha256').update(data).digest('hex')
}
function fileInfo(path: string) {
  const data = readFileSync(path)
  return { path, bytes: data.length, sha256: hash(data) }
}
function sample(fn: () => number) {
  const probe = performance.now()
  sink ^= fn()
  const batch = Math.max(1, Math.min(512, Math.floor(0.25 / Math.max(performance.now() - probe, 0.000001))))
  const start = performance.now()
  let iterations = 0
  let elapsed = 0
  do {
    for (let i = 0; i < batch; i++) sink ^= fn()
    iterations += batch
    elapsed = performance.now() - start
  } while (elapsed < duration)
  return { ns: (elapsed * 1e6) / iterations, iterations, elapsedMs: elapsed }
}
function measure(calls: (() => number)[]) {
  const samples = calls.map(() => [] as ReturnType<typeof sample>[])
  for (const call of calls) for (let i = 0; i < 32; i++) sink ^= call()
  for (let round = 0; round < rounds; round++) {
    for (let j = 0; j < calls.length; j++) {
      const index = (round + j) % calls.length
      samples[index].push(sample(calls[index]))
    }
  }
  return summarize(samples)
}
function summarize(samples: ReturnType<typeof sample>[][]) {
  return samples.map((values, index) => {
    const times = values.map((value) => value.ns).sort((a, b) => a - b)
    return {
      implementation: libraries[index][0],
      medianNs: times[Math.floor(times.length / 2)],
      minNs: times[0],
      maxNs: times[times.length - 1],
      samples: values,
    }
  })
}
const rows: object[] = []
const report = {
  timestamp: new Date().toISOString(),
  node: process.version,
  platform: process.platform,
  arch: process.arch,
  cpu: cpus()[0].model,
  totalmem: totalmem(),
  freemem: freemem(),
  loadavg: loadavg(),
  rounds,
  durationMs: duration,
  simdutf8: '0.1.5',
  artifacts: [fileInfo(resolve(baselinePath)), fileInfo(currentPath)],
  sources: ['Cargo.toml', 'Cargo.lock', 'src/lib.rs', 'benchmark/utf8.ts'].map(fileInfo),
  rows,
}
mkdirSync(dirname(outputPath), { recursive: true })
function record(row: {
  fixture: string
  inputType: string
  api: string
  resolver?: string
  inputBytes: number
  inputHash: string
  error?: { code: string; message: string }
  result: ReturnType<typeof summarize>
}) {
  rows.push(row)
  writeFileSync(outputPath, JSON.stringify({ ...report, sink }, null, 2) + '\n')
  console.log(
    row.fixture,
    row.inputType,
    row.api,
    row.resolver || '',
    row.result.map((value) => `${value.implementation}: ${(value.medianNs / 1000).toFixed(2)} µs`).join(' | '),
  )
}
for (const fixture of fixtures) {
  for (const resolver of fixture.name.startsWith('ascii') ? ['character'] : ['character', 'jieba']) {
    const options = { style: 1, segment: resolver === 'jieba', segmenter: 'jieba' as const }
    for (const api of ['pinyin', 'pinyinString'] as const) {
      const expected = libraries[0][1][api](fixture.text, options)
      for (const inputType of ['buffer', 'string']) {
        const input = inputType === 'buffer' ? Buffer.from(fixture.text) : fixture.text
        const calls = libraries.map(([, library]) => {
          assert.deepEqual(library[api](input, options), expected, `${fixture.name}/${resolver}/${api}/${inputType}`)
          return () => library[api](input, options).length
        })
        const result = measure(calls)
        record({
          ...fixtureMetadata(fixture.text),
          fixture: fixture.name,
          inputType,
          api,
          resolver,
          result,
        })
      }
    }
  }
}
function fixtureMetadata(text: string) {
  return { inputBytes: Buffer.byteLength(text), inputCharacters: Array.from(text).length, inputHash: hash(text) }
}
function errorDetails(library: Binding, input: Buffer) {
  try {
    library.pinyinString(input)
    assert.fail('Malformed UTF-8 was accepted')
  } catch (error) {
    const value = error as Error & { code: string }
    assert.equal(value.code, 'InvalidArg')
    return { code: value.code, message: value.message }
  }
}
for (const [fixture, position] of [
  ['invalid-start', 0],
  ['invalid-middle', 1500000],
  ['invalid-end', 2999997],
  ['truncated-end', -1],
] as const) {
  let input = Buffer.from('中文'.repeat(500000))
  if (position < 0) input = input.subarray(0, -1)
  else input[position] = 0xff
  const expected = errorDetails(libraries[0][1], input)
  for (const [, library] of libraries) assert.deepEqual(errorDetails(library, input), expected)
  record({
    fixture,
    inputType: 'buffer',
    api: 'pinyinString',
    inputBytes: input.length,
    inputHash: hash(input),
    error: expected,
    result: measure(
      libraries.map(
        ([, library]) =>
          () =>
            errorDetails(library, input).message.length,
      ),
    ),
  })
}
// Sequential awaits include input copying, worker scheduling, validation, conversion, and JS results.
for (const resolver of ['character', 'jieba']) {
  const fixture = fixtures.find((value) => value.name === 'literature-100k')!
  const options = { style: 1, segment: resolver === 'jieba', segmenter: 'jieba' as const }
  for (const inputType of ['buffer', 'string']) {
    const input = inputType === 'buffer' ? Buffer.from(fixture.text) : fixture.text
    const expected = libraries[0][1].pinyin(fixture.text, options)
    for (const [, library] of libraries) {
      assert.deepEqual(await library.asyncPinyin(input, options), expected)
      for (let i = 0; i < 16; i++) sink ^= (await library.asyncPinyin(input, options)).length
    }
    const samples = libraries.map(() => [] as ReturnType<typeof sample>[])
    for (let round = 0; round < rounds; round++) {
      for (let j = 0; j < libraries.length; j++) {
        const index = (round + j) % libraries.length
        const start = performance.now()
        let iterations = 0
        let elapsed = 0
        do {
          sink ^= (await libraries[index][1].asyncPinyin(input, options)).length
          iterations++
          elapsed = performance.now() - start
        } while (elapsed < duration)
        samples[index].push({ ns: (elapsed * 1e6) / iterations, iterations, elapsedMs: elapsed })
      }
    }
    record({
      ...fixtureMetadata(fixture.text),
      fixture: fixture.name,
      resolver,
      inputType,
      api: 'asyncPinyin',
      result: summarize(samples),
    })
  }
}
