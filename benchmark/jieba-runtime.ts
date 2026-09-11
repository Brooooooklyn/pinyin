import assert from 'node:assert/strict'
import { spawnSync } from 'node:child_process'
import { createHash } from 'node:crypto'
import { readFileSync, writeFileSync, mkdirSync } from 'node:fs'
import { createRequire } from 'node:module'
import { cpus, platform, totalmem } from 'node:os'
import { resolve } from 'node:path'
import { performance } from 'node:perf_hooks'

const require = createRequire(import.meta.url)
const native = require('../index.js')
const pro = require('pinyin-pro')
const nativePath = Object.keys(require.cache).find(
  (path) => path.endsWith('.node') && require.cache[path]?.exports.pinyin === native.pinyin,
)!
const baseline = process.env.PINYIN_BASELINE ? require(resolve(process.env.PINYIN_BASELINE)) : null
const candidates = [
  ...(baseline
    ? [
        {
          name: 'baseline',
          path: resolve(process.env.PINYIN_BASELINE!),
          library: baseline,
          options: { style: 1, segment: true },
        },
      ]
    : []),
  { name: 'rust', path: nativePath, library: native, options: { style: 1, segment: true, segmenter: 'phrase' } },
  { name: 'rust-jieba', path: nativePath, library: native, options: { style: 1, segment: true, segmenter: 'jieba' } },
  {
    name: 'pinyin-pro',
    path: require.resolve('pinyin-pro'),
    library: pro,
    options: { type: 'array', toneType: 'symbol', nonZh: 'consecutive', toneSandhi: false },
  },
]
const child = `
const {performance} = require('node:perf_hooks');
const info=JSON.parse(process.argv[1]);
const start=performance.now();
const library=require(info.path);
const loaded=performance.now();
const result=library.pinyin('重庆银行音乐',info.options);
const converted=performance.now();
console.log(JSON.stringify({loadMs:loaded-start,firstConversionMs:converted-loaded,totalMs:converted-start,rssBytes:process.memoryUsage().rss,result}));
`
const cold = []
for (let round = 0; round < 9; round++) {
  for (let j = 0; j < candidates.length; j++) {
    const candidate = candidates[(round + j) % candidates.length]
    const result = spawnSync(
      process.execPath,
      ['-e', child, JSON.stringify({ path: candidate.path, options: candidate.options })],
      { encoding: 'utf8', timeout: 30000 },
    )
    assert.equal(result.status, 0, result.stderr)
    cold.push({ round, implementation: candidate.name, ...JSON.parse(result.stdout) })
  }
}

const text = readFileSync(new URL('./long.txt', import.meta.url), 'utf8')
const buffer = Buffer.from(text)
const asynchronous = []
const asyncCandidates = candidates.filter((candidate) => candidate.name !== 'pinyin-pro')
for (const candidate of asyncCandidates) await candidate.library.asyncPinyin(buffer, candidate.options)
for (let round = 0; round < 7; round++) {
  for (let j = 0; j < asyncCandidates.length; j++) {
    const candidate = asyncCandidates[(round + j) % asyncCandidates.length]
    const expected = candidate.library.pinyin(text, candidate.options)
    const start = performance.now()
    const output = await candidate.library.asyncPinyin(buffer, candidate.options)
    const elapsedMs = performance.now() - start
    assert.deepEqual(output, expected)
    asynchronous.push({ round, implementation: candidate.name, elapsedMs })
  }
}

// These examples expose policy differences and known limitations; they are not
// an accuracy benchmark or an oracle derived from another implementation.
const examples = [
  '重庆银行音乐',
  '银行行长',
  '长大以后',
  '他穿着长大衣',
  '快乐成长',
  '重要的重复',
  '研究生命起源',
  '划分为',
  '统称为',
]
const pronunciation = examples.map((input) => ({
  input,
  outputs: candidates.map((candidate) => ({
    implementation: candidate.name,
    output: candidate.library.pinyin(input, candidate.options),
  })),
}))
const phraseOutput = native.pinyin(text, { segment: true, style: 1 })
const jiebaOutput = native.pinyin(text, { segment: true, segmenter: 'jieba', style: 1 })
assert.equal(phraseOutput.length, jiebaOutput.length)
const disagreements = phraseOutput.flatMap((value: string, index: number) =>
  value === jiebaOutput[index] ? [] : [{ index, phrase: value, jieba: jiebaOutput[index] }],
)
const hash = (path: string) => createHash('sha256').update(readFileSync(path)).digest('hex')
const files = [
  'Cargo.toml',
  'Cargo.lock',
  'src/lib.rs',
  'crates/pinyin-core/src/lib.rs',
  'crates/pinyin-core/src/jieba.rs',
  'benchmark/compare.ts',
  'benchmark/jieba-runtime.ts',
  'benchmark/scaling.ts',
  'benchmark/long.txt',
  nativePath,
  require.resolve('pinyin-pro'),
]
mkdirSync('benchmark/results/jieba', { recursive: true })
writeFileSync(
  'benchmark/results/jieba/runtime.json',
  JSON.stringify(
    {
      timestamp: new Date().toISOString(),
      node: process.version,
      cpu: cpus()[0].model,
      platform: platform(),
      arch: process.arch,
      totalmem: totalmem(),
      pinyinPro: require('pinyin-pro/package.json').version,
      jieba: '0.10.3',
      hmm: false,
      files: files.map((path) => ({ path, sha256: hash(path), bytes: readFileSync(path).length })),
      cold,
      asynchronous,
      pronunciation,
      corpusTokens: phraseOutput.length,
      disagreements,
    },
    null,
    2,
  ) + '\n',
)
console.log('Saved Jieba cold-start, async, pronunciation and build evidence.')
