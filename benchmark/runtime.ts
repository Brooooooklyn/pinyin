import assert from 'node:assert/strict'
import { spawnSync } from 'node:child_process'
import { writeFileSync, readFileSync } from 'node:fs'
import { createRequire } from 'node:module'
import { resolve } from 'node:path'
import { performance } from 'node:perf_hooks'

const require = createRequire(import.meta.url)
const native = require('../index.js')
const baseline = process.env.PINYIN_BASELINE ? require(resolve(process.env.PINYIN_BASELINE)) : null
const nativePath = Object.keys(require.cache).find(
  (path) => path.endsWith('.node') && require.cache[path]?.exports.pinyin === native.pinyin,
)!
const cold = []
const child = `
const { performance } = require('node:perf_hooks');
const info = JSON.parse(process.argv[1]);
const start = performance.now();
const lib = require(info.path);
const loaded = performance.now();
const value = lib.pinyin('重庆银行音乐', info.pro ? { type:'array',toneType:'symbol',nonZh:'consecutive',toneSandhi:false } : { style:1,segment:true });
const converted = performance.now();
console.log(JSON.stringify({ loadMs:loaded-start,firstConversionMs:converted-loaded,totalMs:converted-start,rssBytes:process.memoryUsage().rss,result:value }));
`
for (let round = 0; round < 9; round++) {
  const implementations = [
    ...(baseline ? [{ name: 'baseline', path: resolve(process.env.PINYIN_BASELINE!), pro: false }] : []),
    { name: 'rust', path: nativePath, pro: false },
    { name: 'pinyin-pro', path: require.resolve('pinyin-pro'), pro: true },
  ]
  for (let index = 0; index < implementations.length; index++) {
    const implementation = implementations[(round + index) % implementations.length]
    const childResult = spawnSync(process.execPath, ['-e', child, JSON.stringify(implementation)], { encoding: 'utf8' })
    assert.equal(childResult.status, 0, childResult.stderr)
    cold.push({ round, implementation: implementation.name, ...JSON.parse(childResult.stdout) })
  }
}

const operations = []
let sink = 0
for (const [name, pair] of [
  ['short-sort', ['蜘蛛侠1', '蜘蛛侠12']],
  ['early-exit-sort', ['北京' + '中国'.repeat(5000), '上海' + '中国'.repeat(5000)]],
  ['common-prefix-sort', ['中国'.repeat(5000) + '1', '中国'.repeat(5000) + '2']],
] as const) {
  const implementations = [
    ...(baseline ? [{ name: 'baseline', fn: () => baseline.compare(...pair) }] : []),
    { name: 'rust', fn: () => native.compare(...pair) },
  ]
  if (baseline) assert.equal(implementations[0].fn(), implementations[1].fn())
  for (let round = 0; round < 7; round++) {
    for (let index = 0; index < implementations.length; index++) {
      const implementation = implementations[(round + index) % implementations.length]
      const start = performance.now()
      let count = 0
      do {
        sink ^= implementation.fn()
        count++
      } while (performance.now() - start < 100)
      operations.push({
        name,
        round,
        implementation: implementation.name,
        ns: ((performance.now() - start) * 1e6) / count,
        count,
      })
    }
  }
}

const text = readFileSync(new URL('./long.txt', import.meta.url), 'utf8')
const asynchronous = []
for (let round = 0; round < 7; round++) {
  const implementations = [
    ...(baseline ? [{ name: 'baseline', library: baseline }] : []),
    { name: 'rust', library: native },
  ]
  for (let index = 0; index < implementations.length; index++) {
    const { name, library } = implementations[(round + index) % implementations.length]
    const expected = library.pinyin(text, { segment: true, style: 1 })
    const start = performance.now()
    const output = await library.asyncPinyin(Buffer.from(text), { segment: true, style: 1 })
    const elapsedMs = performance.now() - start
    assert.deepEqual(output, expected)
    asynchronous.push({ round, implementation: name, elapsedMs })
  }
}
const report = { timestamp: new Date().toISOString(), node: process.version, cold, operations, asynchronous, sink }
writeFileSync('benchmark/results/runtime.json', JSON.stringify(report, null, 2) + '\n')
console.log('Saved cold-load, comparison, and asynchronous measurements to benchmark/results/runtime.json')
