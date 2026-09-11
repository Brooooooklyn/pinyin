import assert from 'node:assert/strict'
import { createHash } from 'node:crypto'
import { createRequire } from 'node:module'
import { readFileSync, writeFileSync } from 'node:fs'
import { resolve } from 'node:path'
import { performance } from 'node:perf_hooks'
import { cpus } from 'node:os'

const require = createRequire(import.meta.url)
type Binding = typeof import('../../index')
const hash = (s: Buffer | string) => createHash('sha256').update(s).digest('hex')
const names = ['before', 'portable', 'simd']
const paths = names.map((n) => resolve('target/remaining-simd-wasi', n, 'pinyin.wasi.cjs'))
const libraries: Binding[] = paths.map((p) => require(p))
const literature = readFileSync(new URL('../long.txt', import.meta.url), 'utf8')
const fixtures = [
  ['short', '重庆银行音乐你好'],
  ['literature-100k', literature.repeat(6).slice(0, 100000)],
  ['mixed', '中国 API é🙂 \0音乐，2026! '.repeat(5000)],
  ['long-ascii', '中'.repeat(32) + ' api=value '.repeat(10000)],
]
const rows: object[] = []
const report = {
  timestamp: new Date().toISOString(),
  node: process.version,
  cpu: cpus()[0].model,
  rounds: 5,
  durationMs: 60,
  artifacts: paths.map((p) => {
    const wasm = resolve(p, '../pinyin.wasm32-wasi.wasm')
    return { loader: p, wasm, sha256: hash(readFileSync(wasm)) }
  }),
  sources: [
    'benchmark/remaining-simd/wasm.ts',
    'src/lib.rs',
    'crates/pinyin-core/src/lib.rs',
    'crates/pinyin-kernels/src/lib.rs',
    'crates/pinyin-kernels/src/wasm.rs',
  ].map((path) => ({ path, sha256: hash(readFileSync(path)) })),
  rows,
}
let sink = 0
for (const text of [
  '',
  '中'.repeat(33) + '"\\\0🙂é\u2028\ud800',
  '音'.repeat(64) + 'x'.repeat(128) + '乐'.repeat(64),
]) {
  for (const style of [0, 1, 2, 3, 4])
    for (const input of [text, Buffer.from(text)]) {
      for (const library of libraries.slice(1)) {
        assert.deepEqual(
          library.pinyin(input, { style, heteronym: true }),
          libraries[0].pinyin(input, { style, heteronym: true }),
        )
        assert.equal(
          library.pinyinString(input, { style, separator: '\0🙂' }),
          libraries[0].pinyinString(input, { style, separator: '\0🙂' }),
        )
      }
    }
}
for (const [fixture, text] of fixtures)
  for (const resolver of ['character', 'phrase', 'jieba']) {
    for (const api of ['pinyin', 'pinyinString'] as const) {
      const options = {
        style: 1,
        segment: resolver !== 'character',
        segmenter: resolver === 'jieba' ? ('jieba' as const) : ('phrase' as const),
      }
      const expected = libraries[0][api](text, options)
      for (const library of libraries) {
        assert.deepEqual(library[api](text, options), expected)
        for (let i = 0; i < 16; i++) sink ^= library[api](text, options).length
      }
      const samples = names.map(() => [] as number[])
      for (let round = 0; round < 5; round++)
        for (let j = 0; j < libraries.length; j++) {
          const i = (round + j) % libraries.length
          const start = performance.now()
          let iterations = 0
          while (performance.now() - start < 60) {
            sink ^= libraries[i][api](text, options).length
            iterations++
          }
          samples[i].push(((performance.now() - start) * 1e6) / iterations)
        }
      const result = names.map((implementation, i) => ({
        implementation,
        samplesNs: samples[i],
        medianNs: [...samples[i]].sort((a, b) => a - b)[2],
      }))
      rows.push({ fixture, resolver, api, inputHash: hash(text), outputHash: hash(JSON.stringify(expected)), result })
      writeFileSync('benchmark/results/remaining-simd/wasm.json', JSON.stringify({ ...report, sink }, null, 2) + '\n')
      console.log(
        [fixture, resolver, api].join('/'),
        result.map((r) => r.implementation + ' ' + (r.medianNs / 1e6).toFixed(3) + 'ms').join(' | '),
      )
    }
  }
