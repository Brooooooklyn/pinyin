import assert from 'node:assert/strict'
import { createRequire } from 'node:module'
import { writeFileSync } from 'node:fs'
import { resolve } from 'node:path'
import { performance } from 'node:perf_hooks'

const require = createRequire(import.meta.url)
const candidate = require('../index.js')
const baseline = require(resolve(process.env.PINYIN_BASELINE!))
const rows = []
let sink = 0
for (const multi of [false, true]) {
  for (const length of [1, 2, 4, 8, 16, 32, 64, 128, 256, 10000]) {
    const input = '你好拼音重庆银行音乐'.repeat(Math.ceil(length / 10)).slice(0, length)
    for (const string of multi ? [false] : [false, true]) {
      const call = (lib: typeof candidate) =>
        string ? lib.pinyinString(input, { style: 1 }) : lib.pinyin(input, { style: 1, heteronym: multi })
      assert.deepEqual(call(baseline), call(candidate))
      const samples: { baseline: number[]; candidate: number[] } = { baseline: [], candidate: [] }
      for (let round = 0; round < 5; round++) {
        for (const name of round % 2 ? (['candidate', 'baseline'] as const) : (['baseline', 'candidate'] as const)) {
          const lib = name === 'baseline' ? baseline : candidate
          let count = 0
          const start = performance.now()
          do {
            for (let i = 0; i < 64; i++) {
              sink ^= call(lib).length
              count++
            }
          } while (performance.now() - start < 35)
          samples[name].push(((performance.now() - start) * 1e6) / count)
        }
      }
      const median = (arr: number[]) => [...arr].sort((a, b) => a - b)[2]
      const row = {
        multi,
        length,
        string,
        beforeNs: median(samples.baseline),
        afterNs: median(samples.candidate),
        samples,
      }
      rows.push(row)
      console.log(length, { multi, string }, row.beforeNs.toFixed(1), row.afterNs.toFixed(1), 'ns')
    }
  }
}
writeFileSync(
  process.env.BENCH_OUTPUT || 'benchmark/results/thresholds.json',
  JSON.stringify({ node: process.version, rows, sink }, null, 2) + '\n',
)
