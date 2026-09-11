import assert from 'node:assert/strict'
import { createHash } from 'node:crypto'
import { readFileSync, writeFileSync } from 'node:fs'
import { createRequire } from 'node:module'
import { cpus, loadavg, totalmem } from 'node:os'
import { performance } from 'node:perf_hooks'
import { resolve } from 'node:path'

const require = createRequire(import.meta.url)
const addonPath = resolve('target/simd-research/probe.node')
const addon = require(addonPath)
const current = require('../../index.js')
const pro = require('pinyin-pro')
const corpus = readFileSync('benchmark/long.txt', 'utf8')
let seed = 123456789
const alphabet = Array.from('文汉拼音山水风雨春夏秋冬天日月星光')
const matched = Array.from({ length: 100000 }, () => {
  seed = (Math.imul(seed, 1664525) + 1013904223) >>> 0
  return alphabet[(seed >>> 16) % alphabet.length]
}).join('')
const fixtures = [
  { name: 'short', text: '重庆银行音乐你好' },
  { name: 'literature-1k', text: corpus.slice(0, 1000) },
  { name: 'literature-100k', text: corpus.repeat(6).slice(0, 100000) },
  { name: 'mixed-100k', text: '中国 API é🙂 \0音乐，2026! '.repeat(5000) },
  { name: 'matched-100k', text: matched },
]
const hash = (data: string | Buffer) => createHash('sha256').update(data).digest('hex')
const rounds = 7
const duration = 100
let sink = 0
const rows: object[] = []
const report = {
  timestamp: new Date().toISOString(),
  node: process.version,
  v8: process.versions.v8,
  cpu: cpus()[0].model,
  arch: process.arch,
  platform: process.platform,
  totalmem: totalmem(),
  loadavg: loadavg(),
  rounds,
  durationMs: duration,
  addonPath,
  addonSha256: hash(readFileSync(addonPath)),
  scriptSha256: hash(readFileSync('benchmark/simd-research/node.ts')),
  pinyinPro: require('pinyin-pro/package.json').version,
  pinyinProSha256: hash(readFileSync(require.resolve('pinyin-pro'))),
  rows,
}
function sample(fn: () => any) {
  const start = performance.now()
  let iterations = 0
  let elapsed = 0
  do {
    for (let i = 0; i < 8; i++) {
      const out = fn()
      sink ^= typeof out === 'number' ? out : out.length
    }
    iterations += 8
    elapsed = performance.now() - start
  } while (elapsed < duration)
  return { ns: (elapsed * 1e6) / iterations, iterations, elapsedMs: elapsed }
}
type Case = { name: string; setup: () => void; fn: () => any }
function measure(cases: Case[]) {
  const samples = cases.map(() => [] as ReturnType<typeof sample>[])
  for (const c of cases) {
    c.setup()
    for (let i = 0; i < 32; i++) c.fn()
  }
  for (let round = 0; round < rounds; round++) {
    for (let j = 0; j < cases.length; j++) {
      const i = (round + j) % cases.length
      cases[i].setup()
      samples[i].push(sample(cases[i].fn))
    }
  }
  return cases.map((c, i) => ({
    implementation: c.name,
    medianNs: samples[i].map((s) => s.ns).sort((a, b) => a - b)[3],
    samples: samples[i],
  }))
}
function record(row: object) {
  rows.push(row)
  writeFileSync('benchmark/results/simd-research/node.json', JSON.stringify({ ...report, sink }, null, 2) + '\n')
}
// Compare every style, arrays/nested arrays/strings, all resolvers, input kinds,
// unpaired JS surrogates, and scalar/SIMD modes with the unchanged production addon.
for (const text of ['', '你好', '中\0国', '\ud800中国\udc00', '中"\\\n\u001f🙂é\u2028国'.repeat(50)]) {
  for (const style of [0, 1, 2, 3, 4])
    for (const resolver of ['character', 'phrase', 'jieba']) {
      const options = { style, segment: resolver !== 'character', segmenter: resolver === 'jieba' ? 'jieba' : 'phrase' }
      for (const input of [text, Buffer.from(text)])
        for (const mode of [false, true]) {
          addon.researchSetSimd(mode)
          assert.deepEqual(addon.pinyin(input, options), current.pinyin(input, options))
          assert.deepEqual(
            addon.pinyin(input, { ...options, heteronym: true }),
            current.pinyin(input, { ...options, heteronym: true }),
          )
          assert.equal(
            addon.pinyinString(input, { ...options, separator: '\0🙂' }),
            current.pinyinString(input, { ...options, separator: '\0🙂' }),
          )
          assert.deepEqual(await addon.asyncPinyin(input, options), await current.asyncPinyin(input, options))
        }
    }
}
for (const f of fixtures) {
  for (const resolver of ['character', 'phrase', 'jieba'])
    for (const api of ['pinyin', 'pinyinString']) {
      const options = {
        style: 1,
        segment: resolver !== 'character',
        segmenter: resolver === 'jieba' ? 'jieba' : 'phrase',
      }
      const expected = current[api](f.text, options)
      const cases: Case[] = [false, true].map((enabled) => ({
        name: enabled ? 'simd-output' : 'scalar-output',
        setup: () => addon.researchSetSimd(enabled),
        fn: () => addon[api](f.text, options),
      }))
      for (const c of cases) {
        c.setup()
        assert.deepEqual(c.fn(), expected)
      }
      let matchesPro: boolean | null = null
      if (f.name === 'matched-100k' || (f.name === 'literature-100k' && resolver === 'jieba')) {
        const fn = () =>
          pro.pinyin(f.text, {
            type: api === 'pinyin' ? 'array' : 'string',
            toneType: 'symbol',
            toneSandhi: false,
            nonZh: 'consecutive',
          })
        matchesPro = hash(JSON.stringify(fn())) === hash(JSON.stringify(expected))
        if (f.name === 'matched-100k') assert.ok(matchesPro)
        cases.push({ name: 'pinyin-pro', setup: () => {}, fn })
      }
      const result = measure(cases)
      record({
        fixture: f.name,
        inputHash: hash(f.text),
        inputBytes: Buffer.byteLength(f.text),
        api,
        resolver,
        style: 1,
        matchesPro,
        outputHash: hash(JSON.stringify(expected)),
        result,
      })
      console.log(
        f.name,
        resolver,
        api,
        result.map((r) => `${r.implementation} ${(r.medianNs / 1000).toFixed(2)}µs`).join(' | '),
      )
    }
  const result = measure([
    { name: 'napi-get-utf8', setup: () => {}, fn: () => addon.researchInputUtf8Length(f.text) },
    { name: 'napi-get-utf16', setup: () => {}, fn: () => addon.researchInputUtf16Length(f.text) },
  ])
  record({ fixture: f.name, api: 'input-copy-only', result })
}
// Plain all-Han output is ASCII: this is an unchanged same-binary control.
for (const f of [fixtures[0], fixtures[4]])
  for (const api of ['pinyin', 'pinyinString']) {
    const result = measure(
      [false, true].map((enabled) => ({
        name: enabled ? 'simd-output' : 'scalar-output',
        setup: () => addon.researchSetSimd(enabled),
        fn: () => addon[api](f.text, { style: 0 }),
      })),
    )
    record({ fixture: f.name, api, resolver: 'character', style: 0, asciiControl: true, result })
  }
// Already-materialized JS JSON measures parsing only, without Rust or boundary copies.
for (const f of fixtures.filter((f) => f.name.endsWith('100k'))) {
  const text = JSON.stringify(current.pinyin(f.text, { style: 1, segment: true, segmenter: 'jieba' }))
  const result = measure([{ name: 'v8-json-parse', setup: () => {}, fn: () => JSON.parse(text) }])
  record({ fixture: f.name, api: 'parse-only', jsonBytes: Buffer.byteLength(text), result })
}
